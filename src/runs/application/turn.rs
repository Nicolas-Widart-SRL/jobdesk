use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::runs::domain::{ActivityKind, Run, RunStatus, StopPoint, stop_points};
use crate::shared_kernel::visible_text::visible_chars;

/// The question Claude closed its turn with, cut from its last message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosingQuestion {
    /// Plain text, without markup or hidden characters.
    pub text: String,
    /// Index into `Run::activity` of the message it comes from.
    pub activity: usize,
    /// Byte range of the question's paragraph in that message.
    pub range: Range<usize>,
}

/// The stop point the run waits at, when its turn ended on the step that has one.
pub fn stop_point(run: &Run) -> Option<&'static StopPoint> {
    if run.status != RunStatus::AwaitingReply || run.is_busy() || run.needs_user() {
        return None;
    }
    let rail = run.steps.as_ref()?;
    stop_points(&run.line).iter().find(|stop| stop.after_step == rail.current)
}

/// The last paragraph of this turn's last top-level message, when it asks something.
/// Claude's text is untrusted: hidden characters are dropped, and a statement is not passed off as a question.
pub fn closing_question(run: &Run) -> Option<ClosingQuestion> {
    let turn_start = run.activity.iter().rposition(|activity| matches!(activity.kind, ActivityKind::You(_))).map_or(0, |at| at + 1);
    let (activity, text) = run.activity.iter().enumerate().skip(turn_start).rev().find_map(|(at, activity)| match &activity.kind {
        ActivityKind::Claude(text) if !activity.nested => Some((at, text)),
        _ => None,
    })?;
    let (range, plain) = last_paragraph(text)?;
    let text = visible_chars(&plain).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    text.ends_with('?').then_some(ClosingQuestion { text, activity, range })
}

/// Same extensions as the viewer's `markdown_options` in `ui_kit::text`, so both read a message the same way.
/// The layers keep this module from reaching it.
fn markdown_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES | Options::ENABLE_DEFINITION_LIST
}

/// The last paragraph outside any list, quote, table or footnote, with its text read as one line.
fn last_paragraph(markdown: &str) -> Option<(Range<usize>, String)> {
    let mut depth = 0usize;
    let mut open: Option<(Range<usize>, String)> = None;
    let mut last = None;
    for (event, range) in Parser::new_ext(markdown, markdown_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Paragraph) if depth == 0 => open = Some((range, String::new())),
            Event::End(TagEnd::Paragraph) if depth == 0 => last = open.take().or(last),
            Event::Start(tag) if holds_blocks(&tag) => depth += 1,
            Event::End(tag) if ends_blocks(&tag) => depth = depth.saturating_sub(1),
            Event::Text(part) | Event::Code(part) => open.iter_mut().for_each(|(_, text)| text.push_str(&part)),
            Event::SoftBreak | Event::HardBreak => open.iter_mut().for_each(|(_, text)| text.push(' ')),
            _ => {}
        }
    }
    last
}

fn holds_blocks(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::FootnoteDefinition(_) | Tag::Table(_) | Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition
    )
}

fn ends_blocks(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item | TagEnd::FootnoteDefinition | TagEnd::Table | TagEnd::DefinitionList | TagEnd::DefinitionListTitle | TagEnd::DefinitionListDefinition
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::domain::{AgentEvent, Prompt, PromptKind, RunRequest};

    fn run(line: &str) -> Run {
        Run::new(&RunRequest { run_id: "1".into(), line: line.into(), model: None, budget_usd: 1.0, started_at: String::new(), resume_session: None })
    }

    fn says(run: &mut Run, text: &str) {
        run.apply(AgentEvent::AssistantText { text: text.into(), nested: false });
    }

    fn reads(run: &mut Run, summary: &str) {
        run.apply(AgentEvent::ToolStarted { id: summary.into(), name: "Read".into(), summary: summary.into(), nested: false });
    }

    fn ends_turn(run: &mut Run) {
        run.apply(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.5, duration_ms: 10 });
    }

    fn asked(markdown: &str) -> Option<ClosingQuestion> {
        let mut run = run("/apply x");
        says(&mut run, markdown);
        ends_turn(&mut run);
        closing_question(&run)
    }

    fn prompt() -> Prompt {
        Prompt { request_id: "a".into(), kind: PromptKind::Questions(vec![]), input_json: "{}".into(), suggestions_json: None }
    }

    #[test]
    fn the_last_paragraph_is_the_question() {
        let message = "# Fit\n\n**78**\n\nShall I *draft* it?";
        let question = asked(message).unwrap();
        assert_eq!(question.text, "Shall I draft it?");
        assert_eq!(question.activity, 1);
        assert_eq!(&message[question.range], "Shall I *draft* it?");
    }

    #[test]
    fn hidden_characters_are_removed() {
        let question = asked("Shall I\u{202E} go\u{200B} on\x1b[31m now?").unwrap();
        assert_eq!(question.text, "Shall I go on now?");
    }

    #[test]
    fn a_list_after_the_question_is_skipped() {
        let question = asked("Which one do you want?\n\n- the short CV\n- the long CV").unwrap();
        assert_eq!(question.text, "Which one do you want?");
    }

    #[test]
    fn lines_of_a_paragraph_read_as_one() {
        let question = asked("Should I proceed with `drafting`\nthe CV  and\nthe letter?").unwrap();
        assert_eq!(question.text, "Should I proceed with drafting the CV and the letter?");
    }

    #[test]
    fn no_text_this_turn_means_no_question() {
        let mut run = run("/apply x");
        says(&mut run, "Shall I go on?");
        run.note_reply("yes");
        reads(&mut run, "notes.md");
        ends_turn(&mut run);
        assert_eq!(closing_question(&run), None);
    }

    #[test]
    fn nested_text_is_not_the_question() {
        let mut run = run("/apply x");
        run.apply(AgentEvent::AssistantText { text: "Is this the one?".into(), nested: true });
        ends_turn(&mut run);
        assert_eq!(closing_question(&run), None);
        says(&mut run, "Shall I draft it?");
        run.apply(AgentEvent::AssistantText { text: "Is this the one?".into(), nested: true });
        assert_eq!(closing_question(&run).unwrap().text, "Shall I draft it?");
    }

    #[test]
    fn a_statement_is_not_a_question() {
        assert_eq!(asked("Both files are ready."), None);
    }

    #[test]
    fn stop_point_only_at_the_known_step() {
        let mut run = run("/apply https://x.example");
        ends_turn(&mut run);
        assert_eq!(stop_point(&run), None);
        run.note_reply("again");
        reads(&mut run, ".claude/skills/a/04-job-evaluation.md");
        assert_eq!(stop_point(&run), None, "the turn has not ended");
        ends_turn(&mut run);
        assert_eq!(stop_point(&run).unwrap().label, "proceed with drafting");
        run.apply(AgentEvent::PromptRaised(prompt()));
        assert_eq!(stop_point(&run), None);

        let mut rank = self::run("/rank");
        reads(&mut rank, "posting-scorer");
        ends_turn(&mut rank);
        assert_eq!(stop_point(&rank), None);
    }

    #[test]
    fn is_your_turn() {
        let mut run = run("/apply x");
        assert!(!run.is_your_turn());
        ends_turn(&mut run);
        assert!(run.is_your_turn());
        run.apply(AgentEvent::BackgroundTasksChanged { count: 1 });
        assert!(!run.is_your_turn());
        run.apply(AgentEvent::PromptRaised(prompt()));
        assert!(run.is_your_turn());
        run.apply(AgentEvent::PromptWithdrawn { request_id: "a".into() });
        run.apply(AgentEvent::BackgroundTasksChanged { count: 0 });
        run.note_reply("go");
        assert!(!run.is_your_turn());
    }
}
