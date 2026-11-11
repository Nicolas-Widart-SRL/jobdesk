//! What the Profile chapter shows while Claude works or after it stopped: the fixed progress lines, lit by the run's own
//! rows, and the rounds and changes answered so far. Read from the run, so it holds after a restore.

use std::path::Path;

use crate::onboarding::application::{Message, read_message};
use crate::onboarding::domain::{CV_LOOKS_GOOD, PATH_A_REPLY, PICK_UP_REPLY, PROGRESS_LINES, tool_line};
use crate::runs::application::{ActivityKind, Run, RunStatus};

/// The line Jobdesk's own copy lights, the reads light, Claude's first message lights, the profile writes light and the
/// writes after a review reply light, in `PROGRESS_LINES`.
const COPIED: usize = 0;
const READING: usize = 1;
const COMPARING: usize = 2;
const WRITING: usize = 3;
const APPLYING: usize = 4;

/// The fixed start of the review card's replies, as `review_reply` builds them.
const APPLY_ALL: &str = "Yes, apply these changes.";
const APPLY_EXCEPT: &str = "Apply all except ";

/// How a progress line stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineState {
    Done,
    /// Claude works on it now.
    Working,
    /// The run stopped on it.
    Stopped,
    /// Not reached yet.
    Ahead,
}

/// One progress line as the card shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkLine {
    pub text: &'static str,
    pub state: LineState,
}

/// The rounds the user answered and the changes they approved, in the run's order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoFar {
    /// How many questions each round answered had.
    pub rounds: Vec<usize>,
    /// How many proposed changes the user let Claude apply.
    pub changes: usize,
}

/// The progress lines for `run` in the job search folder `root`, in the handoff's order. Lines lit by the run are done,
/// but the last one lit, which works while the run does and stopped once it stopped. Lines the run passed without
/// lighting are left out, and Applying the changes you approved shows only once a review was answered. With no run yet,
/// the copy works.
pub fn work_lines(run: Option<&Run>, root: &Path) -> Vec<WorkLine> {
    let Some(run) = run else {
        return PROGRESS_LINES
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != APPLYING)
            .map(|(at, line)| WorkLine { text: line.text, state: if at == COPIED { LineState::Working } else { LineState::Ahead } })
            .collect();
    };
    let root = root.to_string_lossy();
    let mut lit = [false; PROGRESS_LINES.len()];
    lit[COPIED] = true;
    let mut latest = COPIED;
    let mut reviewed = false;
    let light = |at: usize, lit: &mut [bool], latest: &mut usize| {
        lit[at] = true;
        *latest = at;
    };
    // The command line is the run's first row, and no reply.
    for row in run.activity.iter().skip(1) {
        match &row.kind {
            ActivityKind::You(text) => reviewed = is_review_reply(text),
            ActivityKind::Tool { name, summary, .. } => {
                let Some(text) = tool_line(&root, name, summary) else { continue };
                let Some(mut at) = PROGRESS_LINES.iter().position(|line| line.text == text) else { continue };
                if at == WRITING && reviewed {
                    at = APPLYING;
                }
                light(at, &mut lit, &mut latest);
            }
            ActivityKind::Claude(_) if !row.nested && latest == READING && !lit[COMPARING] => light(COMPARING, &mut lit, &mut latest),
            _ => {}
        }
    }
    let busy = run.is_busy();
    let stopped = matches!(run.status, RunStatus::Failed(_) | RunStatus::Stopped) || (!busy && !run.has_turn_result());
    // While Claude works past the copy, it reads first.
    let working = if busy && latest == COPIED { READING } else { latest };
    PROGRESS_LINES
        .iter()
        .enumerate()
        .filter_map(|(at, line)| {
            let state = if at == working && busy {
                LineState::Working
            } else if at == working && stopped && at != COPIED {
                LineState::Stopped
            } else if lit[at] {
                LineState::Done
            } else if at > working && at != APPLYING {
                LineState::Ahead
            } else {
                return None;
            };
            Some(WorkLine { text: line.text, state })
        })
        .collect()
}

/// What the user answered so far in `run`: each reply to a message of numbered questions counts that round, and each
/// reply from the review card counts the changes it let through. Jobdesk's own replies count for nothing.
pub fn so_far(run: Option<&Run>) -> SoFar {
    let mut found = SoFar::default();
    let Some(run) = run else { return found };
    let mut last: Option<&str> = None;
    for row in run.activity.iter().skip(1) {
        match &row.kind {
            ActivityKind::Claude(text) if !row.nested => last = Some(text),
            ActivityKind::You(text) => {
                let answered = last.take();
                if [PATH_A_REPLY, PICK_UP_REPLY, CV_LOOKS_GOOD].contains(&text.as_str()) {
                    continue;
                }
                match answered.map(read_message) {
                    Some(Message::Questions(batch)) => found.rounds.push(batch.items().count()),
                    Some(Message::Proposal(batch)) if is_review_reply(text) => {
                        found.changes += batch.items().count().saturating_sub(skipped(text));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    found
}

/// Whether `text` is a reply the review card builds.
fn is_review_reply(text: &str) -> bool {
    text == APPLY_ALL || text.starts_with(APPLY_EXCEPT)
}

/// How many changes a review reply leaves out: the numbers it names.
fn skipped(text: &str) -> usize {
    text.strip_prefix(APPLY_EXCEPT).map_or(0, |rest| rest.split(|c: char| !c.is_ascii_digit()).filter(|word| !word.is_empty()).count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::application::{AgentEvent, RunRequest};

    const ROOT: &str = "/Users/sam/Documents/Job search";
    const FIVE: &str = include_str!("../../../tests/fixtures/setup/five_questions.md");
    const SIXTEEN: &str = include_str!("../../../tests/fixtures/setup/sixteen_changes.md");

    fn run() -> Run {
        let request = RunRequest {
            run_id: "r1".into(),
            line: "/setup".into(),
            model: None,
            budget_usd: 25.0,
            started_at: String::new(),
            resume_session: None,
            sign_in: crate::shared_kernel::SignIn::ClaudeSignIn,
        };
        Run::new(&request)
    }

    fn tool(run: &mut Run, id: &str, name: &str, summary: &str) {
        run.apply(AgentEvent::ToolStarted { id: id.into(), name: name.into(), summary: summary.into(), nested: false });
        run.apply(AgentEvent::ToolFinished { id: id.into(), outcome: Ok(()) });
    }

    fn says(run: &mut Run, text: &str) {
        run.apply(AgentEvent::AssistantText { text: text.into(), nested: false });
    }

    fn ends(run: &mut Run) {
        run.apply(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.1, duration_ms: 1 });
    }

    fn states(lines: &[WorkLine]) -> Vec<(&'static str, LineState)> {
        lines.iter().map(|line| (line.text, line.state)).collect()
    }

    #[test]
    fn the_lines_follow_the_run_and_the_working_one_is_the_last_lit() {
        let none = work_lines(None, Path::new(ROOT));
        assert_eq!(none[0].state, LineState::Working, "the copy works first");
        assert!(none.iter().all(|line| line.text != PROGRESS_LINES[APPLYING].text), "no review yet");

        let mut run = run();
        run.apply(AgentEvent::SessionStarted { session_id: "s".into(), model: "m".into(), api_key_source: "none".into() });
        let lines = work_lines(Some(&run), Path::new(ROOT));
        assert_eq!(lines[0].state, LineState::Done);
        assert_eq!(lines[1], WorkLine { text: "Reading your documents", state: LineState::Working }, "Claude reads first");

        tool(&mut run, "1", "Read", &format!("{ROOT}/documents/cv/Mira_Hale_CV.pdf"));
        says(&mut run, "I read your CV. Now the comparison.");
        let lines = states(&work_lines(Some(&run), Path::new(ROOT)));
        assert_eq!(lines[1], ("Reading your documents", LineState::Done));
        assert_eq!(lines[2], ("Comparing your CV with your other documents", LineState::Working));

        tool(&mut run, "2", "Write", &format!("{ROOT}/CLAUDE.md"));
        run.note_reply(APPLY_ALL);
        tool(&mut run, "3", "Edit", &format!("{ROOT}/CLAUDE.md"));
        let lines = states(&work_lines(Some(&run), Path::new(ROOT)));
        assert!(lines.contains(&("Writing your profile", LineState::Done)));
        assert!(lines.contains(&("Applying the changes you approved", LineState::Working)), "{lines:?}");
        assert!(lines.contains(&("Making your CV", LineState::Ahead)));

        run.apply(AgentEvent::ProcessExited { failure: None });
        let lines = states(&work_lines(Some(&run), Path::new(ROOT)));
        assert!(lines.contains(&("Applying the changes you approved", LineState::Stopped)), "a cut turn stops on its line: {lines:?}");
    }

    #[test]
    fn so_far_counts_the_rounds_and_the_changes_let_through() {
        let mut run = run();
        run.apply(AgentEvent::SessionStarted { session_id: "s".into(), model: "m".into(), api_key_source: "none".into() });
        says(&mut run, "Which path?");
        ends(&mut run);
        run.note_reply(PATH_A_REPLY);
        says(&mut run, FIVE);
        ends(&mut run);
        run.note_reply("1. Design Lead\n2. Skipped\n3. Skipped\n4. Skipped\n5. Skipped");
        says(&mut run, SIXTEEN);
        ends(&mut run);
        run.note_reply("Apply all except 3 and 7.");
        assert_eq!(so_far(Some(&run)), SoFar { rounds: vec![5], changes: 14 });
        assert_eq!(skipped("Apply all except 3, 7 and 12."), 3);
        assert_eq!(skipped(APPLY_ALL), 0);
    }
}
