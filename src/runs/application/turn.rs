use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::runs::domain::{ActivityKind, Run, StopPoint, stop_points};
use crate::shared_kernel::markdown;
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

/// Whether the turn ended and the user is the one to act, with no prompt open.
fn waiting_for_reply(run: &Run) -> bool {
    run.is_your_turn() && !run.needs_user()
}

/// The stop point the run waits at, when its turn ended on the step that has one
/// and the user has not answered since the rail reached it.
pub fn stop_point(run: &Run) -> Option<&'static StopPoint> {
    if !waiting_for_reply(run) || run.rail_moved_in_turn() != run.turn_number() {
        return None;
    }
    let rail = run.steps.as_ref()?;
    let step = rail.labels.get(rail.current)?;
    stop_points(&run.line).iter().find(|stop| stop.after == *step)
}

/// The last paragraph of this turn's last top-level message, when it asks something.
/// Claude's text is untrusted: hidden characters are dropped, and a statement is not passed off as a question.
pub fn closing_question(run: &Run) -> Option<ClosingQuestion> {
    if !waiting_for_reply(run) {
        return None;
    }
    let turn = run.activity.iter().enumerate().skip(run.current_turn_start());
    let (activity, message) = turn.rev().find_map(|(at, activity)| match &activity.kind {
        ActivityKind::Claude(text) if !activity.nested => Some((at, text)),
        _ => None,
    })?;
    let (range, plain) = last_paragraph(message)?;
    let text = visible_chars(&plain).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    text.ends_with('?').then(|| ClosingQuestion { text, activity, range: trimmed(message, range) })
}

/// The range without the whitespace around the text it holds.
fn trimmed(message: &str, range: Range<usize>) -> Range<usize> {
    let text = &message[range.clone()];
    let start = range.start + (text.len() - text.trim_start().len());
    start..start + text.trim().len()
}

/// The last paragraph outside any list, quote, table or footnote, with its text read as one line.
fn last_paragraph(text: &str) -> Option<(Range<usize>, String)> {
    let mut depth = 0usize;
    let mut open: Option<(Range<usize>, String)> = None;
    let mut last = None;
    for (event, range) in Parser::new_ext(text, markdown::options()).into_offset_iter() {
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
        let nested = AgentEvent::AssistantText { text: "Is this the one?".into(), nested: true };
        let mut only_nested = run("/apply x");
        only_nested.apply(nested.clone());
        ends_turn(&mut only_nested);
        assert_eq!(closing_question(&only_nested), None);

        let mut after_question = run("/apply x");
        says(&mut after_question, "Shall I draft it?");
        after_question.apply(nested);
        ends_turn(&mut after_question);
        assert_eq!(closing_question(&after_question).unwrap().text, "Shall I draft it?");
    }

    #[test]
    fn no_question_while_the_run_works_or_asks_through_a_prompt() {
        let mut run = run("/apply x");
        says(&mut run, "Shall I draft it?");
        assert_eq!(closing_question(&run), None);
        ends_turn(&mut run);
        assert!(closing_question(&run).is_some());
        run.apply(AgentEvent::PromptRaised(prompt()));
        assert_eq!(closing_question(&run), None);
    }

    #[test]
    fn a_turn_claude_starts_on_its_own_has_its_own_question() {
        let mut run = run("/apply x");
        says(&mut run, "Shall I draft?");
        ends_turn(&mut run);
        run.apply(AgentEvent::SessionStarted { session_id: "s".into(), model: "m".into(), api_key_source: "none".into() });
        reads(&mut run, "notes.md");
        ends_turn(&mut run);
        assert_eq!(closing_question(&run), None);
        run.apply(AgentEvent::SessionStarted { session_id: "s".into(), model: "m".into(), api_key_source: "none".into() });
        says(&mut run, "The agent is done. Shall I go on?");
        ends_turn(&mut run);
        assert_eq!(closing_question(&run).unwrap().text, "The agent is done. Shall I go on?");
    }

    #[test]
    fn the_range_holds_only_the_paragraph_text() {
        let message = "Shall I?\n\n- a\n- b";
        assert_eq!(&message[asked(message).unwrap().range], "Shall I?");
        let message = "Intro.\n\n   Shall I\n   draft it?  \n\n";
        let range = asked(message).unwrap().range;
        assert_eq!(&message[range.clone()], "Shall I\n   draft it?");
        assert!(message[..range.start].ends_with("\n   "));
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
    fn an_answered_stop_point_does_not_come_back() {
        let mut run = run("/apply https://x.example");
        reads(&mut run, ".claude/skills/a/04-job-evaluation.md");
        ends_turn(&mut run);
        assert!(stop_point(&run).is_some());
        run.note_reply("No, stop here. Do not draft.");
        ends_turn(&mut run);
        assert_eq!(stop_point(&run), None);
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
