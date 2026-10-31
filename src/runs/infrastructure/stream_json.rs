use serde_json::{Map, Value, json};

use crate::runs::domain::{AgentEvent, Decision, Prompt, PromptKind, Question, QuestionOption, Usage};

/// Longest tool input shown on a permission card. Longer input is cut with a clear marker.
const MAX_DETAIL: usize = 20_000;
/// Longest reason kept from a failed tool's output, in bytes. A longer reason ends with `…`.
const MAX_REASON: usize = 300;

fn text_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// Translates one line of `claude --output-format stream-json`. Unknown lines mean nothing here.
pub fn parse_line(line: &str) -> Vec<AgentEvent> {
    let Ok(event) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    match text_of(&event, "type") {
        "system" if text_of(&event, "subtype") == "init" => {
            vec![AgentEvent::SessionStarted {
                session_id: text_of(&event, "session_id").to_owned(),
                model: text_of(&event, "model").to_owned(),
                api_key_source: text_of(&event, "apiKeySource").to_owned(),
            }]
        }
        "system" if text_of(&event, "subtype") == "background_tasks_changed" => {
            vec![AgentEvent::BackgroundTasksChanged { count: event.get("tasks").and_then(Value::as_array).map_or(0, Vec::len) }]
        }
        "stream_event" => text_delta(&event).into_iter().collect(),
        "assistant" => assistant_message(&event),
        "user" => tool_results(&event),
        "rate_limit_event" => vec![AgentEvent::UsageChanged(usage(&event))],
        "control_request" => prompt(&event).map(AgentEvent::PromptRaised).into_iter().collect(),
        "control_cancel_request" => vec![AgentEvent::PromptWithdrawn { request_id: text_of(&event, "request_id").to_owned() }],
        "result" => vec![turn_ended(&event)],
        _ => Vec::new(),
    }
}

fn is_nested(event: &Value) -> bool {
    event.get("parent_tool_use_id").is_some_and(|parent| !parent.is_null())
}

fn text_delta(event: &Value) -> Option<AgentEvent> {
    if is_nested(event) {
        return None;
    }
    let delta = event.pointer("/event/delta")?;
    (text_of(delta, "type") == "text_delta").then(|| AgentEvent::TextDelta(text_of(delta, "text").to_owned()))
}

fn assistant_message(event: &Value) -> Vec<AgentEvent> {
    let nested = is_nested(event);
    let blocks = event.pointer("/message/content").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    blocks
        .iter()
        .filter_map(|block| match text_of(block, "type") {
            "text" => Some(AgentEvent::AssistantText { text: text_of(block, "text").to_owned(), nested }),
            "tool_use" => {
                let name = text_of(block, "name").to_owned();
                let summary = tool_summary(&name, block.get("input").unwrap_or(&Value::Null));
                Some(AgentEvent::ToolStarted { id: text_of(block, "id").to_owned(), name, summary, nested })
            }
            _ => None,
        })
        .collect()
}

/// The one thing about a tool call a person wants to read.
fn tool_summary(name: &str, input: &Value) -> String {
    match name {
        "Bash" => text_of(input, "command").to_owned(),
        "Agent" | "Task" => format!("{}: {}", text_of(input, "subagent_type"), text_of(input, "description")),
        _ => ["file_path", "path", "url", "pattern", "query", "skill", "command"]
            .iter()
            .map(|key| text_of(input, key))
            .find(|value| !value.is_empty())
            .unwrap_or_default()
            .to_owned(),
    }
}

fn tool_results(event: &Value) -> Vec<AgentEvent> {
    let blocks = event.pointer("/message/content").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    blocks
        .iter()
        .filter(|block| text_of(block, "type") == "tool_result")
        .map(|block| {
            let failed = block.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            let reason = if failed { failure_reason(block.get("content")) } else { None };
            AgentEvent::ToolFinished { id: text_of(block, "tool_use_id").to_owned(), failed, reason }
        })
        .collect()
}

/// The reason a tool failed, from its output: a string, or a list of text blocks.
fn failure_reason(content: Option<&Value>) -> Option<String> {
    match content {
        Some(Value::String(text)) => reason_from(unwrap_tool_error(text).lines()),
        Some(Value::Array(blocks)) => reason_from(blocks.iter().flat_map(|block| unwrap_tool_error(text_of(block, "text")).lines())),
        _ => None,
    }
}

/// The first line and the last two lines, because a traceback or a compiler log ends with the real error.
/// Blank lines and the CLI's leading exit code line are dropped, and the text is made safe to show.
fn reason_from<'a>(lines: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut lines = lines.filter(|line| !visible_chars(line).all(char::is_whitespace)).peekable();
    lines.next_if(|line| exit_code(line));
    let first = lines.next()?;
    let mut tail = (None, None);
    for line in lines {
        tail = (tail.1, Some(line));
    }
    let kept = [Some(first), tail.0, tail.1].into_iter().flatten();
    let reason = kept.map(|line| visible_chars(line).collect::<String>().trim_end().to_owned()).collect::<Vec<_>>().join("\n");
    Some(match cut(&reason, MAX_REASON) {
        Some(head) => format!("{}…", head.trim_end()),
        None => reason,
    })
}

/// Claude Code wraps many tool errors as `<tool_use_error>File does not exist.</tool_use_error>`.
fn unwrap_tool_error(text: &str) -> &str {
    let Some(inner) = text.trim().strip_prefix("<tool_use_error>") else { return text };
    inner.strip_suffix("</tool_use_error>").unwrap_or(inner)
}

/// The `Exit code N` line the CLI puts before a failed command's output.
fn exit_code(line: &str) -> bool {
    line.trim().strip_prefix("Exit code ").is_some_and(|code| !code.is_empty() && code.chars().all(|c| c.is_ascii_digit()))
}

/// The line without ANSI escape sequences, control characters other than tab, or bidi overrides
/// and isolates, so untrusted output cannot restyle or reorder what the screen shows.
fn visible_chars(line: &str) -> impl Iterator<Item = char> + '_ {
    let mut chars = line.chars();
    std::iter::from_fn(move || loop {
        let c = chars.next()?;
        if c == '\u{1b}' {
            skip_escape(&mut chars);
        } else if !(c.is_control() && c != '\t' || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')) {
            return Some(c);
        }
    })
}

/// Skips what follows an escape: a CSI sequence up to its final byte, an OSC sequence up to its terminator,
/// or one character.
fn skip_escape(chars: &mut std::str::Chars) {
    match chars.next() {
        Some('[') => {
            chars.find(|c| ('\u{40}'..='\u{7e}').contains(c));
        }
        Some(']') => {
            let mut escaped = false;
            chars.find(|&c| {
                let end = c == '\u{7}' || escaped && c == '\\';
                escaped = c == '\u{1b}';
                end
            });
        }
        _ => {}
    }
}

/// The longest start of `text` within `max` bytes that ends on a char boundary, or `None` when `text` fits.
fn cut(text: &str, max: usize) -> Option<&str> {
    if text.len() <= max {
        return None;
    }
    let end = (0..=max).rev().find(|at| text.is_char_boundary(*at)).unwrap_or(0);
    Some(&text[..end])
}

fn usage(event: &Value) -> Usage {
    let window = |name: &str| event.pointer(&format!("/rate_limit_info/unifiedWindows/{name}/utilization")).and_then(Value::as_f64).map(|value| value as f32);
    Usage { five_hour: window("five_hour"), seven_day: window("seven_day") }
}

fn prompt(event: &Value) -> Option<Prompt> {
    let request = event.get("request")?;
    if text_of(request, "subtype") != "can_use_tool" {
        return None;
    }
    let tool = text_of(request, "tool_name").to_owned();
    let input = request.get("input").cloned().unwrap_or(Value::Null);
    let suggestions = request.get("permission_suggestions").filter(|value| value.as_array().is_some_and(|list| !list.is_empty()));
    let kind = if tool == "AskUserQuestion" {
        PromptKind::Questions(questions(&input))
    } else {
        PromptKind::Permission { detail: permission_detail(&tool, &input), tool, can_remember: suggestions.is_some() }
    };
    Some(Prompt {
        request_id: text_of(event, "request_id").to_owned(),
        kind,
        input_json: input.to_string(),
        suggestions_json: suggestions.map(Value::to_string),
    })
}

fn permission_detail(tool: &str, input: &Value) -> String {
    let full = if tool == "Bash" { text_of(input, "command").to_owned() } else { serde_json::to_string_pretty(input).unwrap_or_default() };
    match cut(&full, MAX_DETAIL) {
        Some(head) => format!("{head}\n[Shortened: {} more bytes]", full.len() - head.len()),
        None => full,
    }
}

fn questions(input: &Value) -> Vec<Question> {
    let list = input.get("questions").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    list.iter()
        .map(|question| Question {
            text: text_of(question, "question").to_owned(),
            header: text_of(question, "header").to_owned(),
            multi_select: question.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
            options: question
                .get("options")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice)
                .iter()
                .map(|option| QuestionOption { label: text_of(option, "label").to_owned(), description: text_of(option, "description").to_owned() })
                .collect(),
        })
        .collect()
}

fn turn_ended(event: &Value) -> AgentEvent {
    let subtype = text_of(event, "subtype");
    let failed = event.get("is_error").and_then(Value::as_bool).unwrap_or(false) || subtype.starts_with("error");
    let message = match subtype {
        "error_max_budget_usd" => Some("Budget cap reached.".to_owned()),
        _ if failed => Some(event.get("result").and_then(Value::as_str).map_or_else(|| format!("Run ended: {subtype}"), str::to_owned)),
        _ => None,
    };
    AgentEvent::TurnEnded {
        failed,
        message,
        cost_usd: event.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
        duration_ms: event.get("duration_ms").and_then(Value::as_u64).unwrap_or(0),
    }
}

pub fn user_message(text: &str) -> String {
    json!({"type": "user", "message": {"role": "user", "content": text}}).to_string()
}

pub fn interrupt_request(request_id: &str) -> String {
    json!({"type": "control_request", "request_id": request_id, "request": {"subtype": "interrupt"}}).to_string()
}

/// The answer to a `can_use_tool` request.
pub fn decision_response(prompt: &Prompt, decision: &Decision) -> String {
    let input: Value = serde_json::from_str(&prompt.input_json).unwrap_or(Value::Null);
    let response = match decision {
        Decision::Deny => json!({"behavior": "deny", "message": "The user denied this action."}),
        Decision::Allow { remember } => {
            let mut allow = json!({"behavior": "allow", "updatedInput": input});
            if *remember {
                allow["updatedPermissions"] = session_scoped(prompt.suggestions_json.as_deref());
            }
            allow
        }
        Decision::Answer(answers) => {
            let answers: Map<String, Value> = answers.iter().map(|(question, answer)| (question.clone(), Value::from(answer.as_str()))).collect();
            let mut updated = input;
            if let Some(fields) = updated.as_object_mut() {
                fields.insert("answers".to_owned(), Value::Object(answers));
            }
            json!({"behavior": "allow", "updatedInput": updated})
        }
    };
    json!({"type": "control_response", "response": {"subtype": "success", "request_id": prompt.request_id, "response": response}}).to_string()
}

/// The CLI suggests saving rules to project settings. This app never writes to the checkout,
/// so every remembered rule lasts for the session only.
fn session_scoped(suggestions: Option<&str>) -> Value {
    let mut rules: Value = suggestions.and_then(|raw| serde_json::from_str(raw).ok()).unwrap_or_else(|| json!([]));
    if let Some(list) = rules.as_array_mut() {
        for rule in list {
            if rule.get("destination").is_some() {
                rule["destination"] = json!("session");
            }
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_session_start() {
        let events = parse_line(r#"{"type":"system","subtype":"init","session_id":"s","model":"m","apiKeySource":"none"}"#);
        assert_eq!(events, [AgentEvent::SessionStarted { session_id: "s".into(), model: "m".into(), api_key_source: "none".into() }]);
    }

    #[test]
    fn nested_tools_stay_nested_and_deltas_stay_top_level() {
        let tool = r#"{"type":"assistant","parent_tool_use_id":"p","message":{"content":[{"type":"tool_use","id":"t","name":"Bash","input":{"command":"ls"}}]}}"#;
        assert_eq!(parse_line(tool), [AgentEvent::ToolStarted { id: "t".into(), name: "Bash".into(), summary: "ls".into(), nested: true }]);
        let delta = r#"{"type":"stream_event","parent_tool_use_id":"p","event":{"delta":{"type":"text_delta","text":"x"}}}"#;
        assert_eq!(parse_line(delta), []);
    }

    #[test]
    fn captured_permission_request_becomes_a_prompt() {
        let line = r#"{"type":"control_request","request_id":"r","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"wc -l x"},"permission_suggestions":[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"wc -l x"}],"behavior":"allow","destination":"localSettings"}]}}"#;
        let [AgentEvent::PromptRaised(prompt)] = &parse_line(line)[..] else { panic!() };
        assert_eq!(prompt.kind, PromptKind::Permission { tool: "Bash".into(), detail: "wc -l x".into(), can_remember: true });

        let response: Value = serde_json::from_str(&decision_response(prompt, &Decision::Allow { remember: true })).unwrap();

        assert_eq!(response["response"]["request_id"], "r");
        assert_eq!(response["response"]["response"]["updatedPermissions"][0]["destination"], "session");
        assert_eq!(response["response"]["response"]["updatedInput"]["command"], "wc -l x");
    }

    #[test]
    fn captured_question_round_trips_with_answers() {
        let line = r#"{"type":"control_request","request_id":"q","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[{"question":"Blue?","header":"Colours","options":[{"label":"Yes","description":"d"}],"multiSelect":false}]}}}"#;
        let [AgentEvent::PromptRaised(prompt)] = &parse_line(line)[..] else { panic!() };
        let PromptKind::Questions(questions) = &prompt.kind else { panic!() };
        assert_eq!(questions[0].options[0].label, "Yes");

        let response: Value = serde_json::from_str(&decision_response(prompt, &Decision::Answer(vec![("Blue?".into(), "Yes".into())]))).unwrap();

        assert_eq!(response["response"]["response"]["updatedInput"]["answers"]["Blue?"], "Yes");
        assert_eq!(response["response"]["response"]["updatedInput"]["questions"][0]["header"], "Colours");
    }

    #[test]
    fn deny_sends_no_input() {
        let prompt = Prompt { request_id: "r".into(), kind: PromptKind::Questions(vec![]), input_json: "{}".into(), suggestions_json: None };
        let response: Value = serde_json::from_str(&decision_response(&prompt, &Decision::Deny)).unwrap();
        assert_eq!(response["response"]["response"]["behavior"], "deny");
    }

    #[test]
    fn budget_end_is_explained() {
        let events = parse_line(r#"{"type":"result","subtype":"error_max_budget_usd","is_error":true,"total_cost_usd":1.5,"duration_ms":9}"#);
        let [AgentEvent::TurnEnded { failed: true, message: Some(message), cost_usd, .. }] = &events[..] else { panic!() };
        assert!(message.contains("Budget"));
        assert_eq!(*cost_usd, 1.5);
    }

    #[test]
    fn failed_tool_text_gives_the_reason() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","is_error":true,"content":"Exit code 1\nDISALLOWED for Claude-User - do not retry, go to step 3\nfetch_page: refused - robots.txt does not permit this path"}]}}"#;
        let reason = "DISALLOWED for Claude-User - do not retry, go to step 3\nfetch_page: refused - robots.txt does not permit this path";
        assert_eq!(parse_line(line), [AgentEvent::ToolFinished { id: "t".into(), failed: true, reason: Some(reason.into()) }]);
    }

    #[test]
    fn failed_tool_blocks_give_the_reason() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","is_error":true,"content":[{"type":"text","text":"Exit code 2"},{"type":"text","text":"ls: invalid option -- 'Z'"}]}]}}"#;
        assert_eq!(parse_line(line), [AgentEvent::ToolFinished { id: "t".into(), failed: true, reason: Some("ls: invalid option -- 'Z'".into()) }]);
    }

    #[test]
    fn reason_keeps_the_first_line_and_the_end() {
        let traceback = "Traceback (most recent call last):\n  File \"score.py\", line 4, in <module>\n    main()\n  File \"score.py\", line 2, in main\n    int(\"x\")\nValueError: invalid literal for int() with base 10: 'x'\n";
        let reason = "Traceback (most recent call last):\n    int(\"x\")\nValueError: invalid literal for int() with base 10: 'x'";
        assert_eq!(failure_reason(Some(&json!(traceback))).as_deref(), Some(reason));
    }

    #[test]
    fn short_reason_repeats_no_line() {
        assert_eq!(failure_reason(Some(&json!("one\n\ntwo"))).as_deref(), Some("one\ntwo"));
        assert_eq!(failure_reason(Some(&json!("one"))).as_deref(), Some("one"));
    }

    #[test]
    fn reason_drops_only_the_leading_exit_code_and_keeps_indentation() {
        let reason = failure_reason(Some(&json!("Exit code 1\nrunning step\nExit code 3\n    at main.rs  \n")));
        assert_eq!(reason.as_deref(), Some("running step\nExit code 3\n    at main.rs"));
    }

    #[test]
    fn reason_loses_the_tool_use_error_wrapper() {
        let reason = failure_reason(Some(&json!("<tool_use_error>File does not exist.</tool_use_error>")));
        assert_eq!(reason.as_deref(), Some("File does not exist."));
    }

    #[test]
    fn reason_is_made_safe_to_show() {
        let raw = "\u{1b}[31merror:\u{1b}[0m bad\u{7}\r\n\u{1b}]8;;https://x.test\u{7}link\u{1b}]8;;\u{1b}\\ \u{202E}txt.exe\u{2066}\u{2069}\n\u{1b}[0m";
        assert_eq!(failure_reason(Some(&json!(raw))).as_deref(), Some("error: bad\nlink txt.exe"));
    }

    #[test]
    fn long_reason_is_cut() {
        let reason = failure_reason(Some(&json!("é".repeat(400)))).unwrap();
        assert_eq!(reason, format!("{}…", "é".repeat(MAX_REASON / 2)));
    }

    #[test]
    fn empty_output_gives_no_reason() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","is_error":true,"content":"Exit code 1\n\n"}]}}"#;
        assert_eq!(parse_line(line), [AgentEvent::ToolFinished { id: "t".into(), failed: true, reason: None }]);
    }

    #[test]
    fn finished_tool_has_no_reason() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}}"#;
        assert_eq!(parse_line(line), [AgentEvent::ToolFinished { id: "t".into(), failed: false, reason: None }]);
    }

    #[test]
    fn reads_usage_windows() {
        let line = r#"{"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"utilization":0.25},"seven_day":{"utilization":0.5}}}}"#;
        assert_eq!(parse_line(line), [AgentEvent::UsageChanged(Usage { five_hour: Some(0.25), seven_day: Some(0.5) })]);
    }

    #[test]
    fn counts_background_tasks() {
        let line = r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"a","task_type":"local_agent"}]}"#;
        assert_eq!(parse_line(line), [AgentEvent::BackgroundTasksChanged { count: 1 }]);
    }

    #[test]
    fn long_input_is_marked_as_shortened() {
        let detail = permission_detail("Bash", &json!({"command": "é".repeat(MAX_DETAIL)}));
        assert!(detail.ends_with("more bytes]"));
    }

    #[test]
    fn junk_is_ignored() {
        assert_eq!(parse_line("not json"), []);
        assert_eq!(parse_line(r#"{"type":"future_thing"}"#), []);
    }
}
