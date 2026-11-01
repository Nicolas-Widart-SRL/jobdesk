use crate::runs::application::{closing_question, command_word, first_line, last_line, stop_point, wants_reply};
use crate::runs::domain::{ActivityKind, Prompt, PromptKind, Run, RunStatus, ToolState, phrases_for};
use crate::shared_kernel::visible_text::visible_chars;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Done,
    Working,
    /// The run waits for the user in or before this step.
    Waiting,
    Failed,
    Next,
}

/// One started step, folded to a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepLine {
    pub step: usize,
    pub name: &'static str,
    pub state: StepState,
    /// Built from the command's fixed phrases and counts only, never from text in the run.
    pub sentence: String,
    /// Whether the step may still gain actions, so its toggle reads "so far".
    pub live: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandTone {
    Plain,
    Claude,
    Yours,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BandText {
    pub value: String,
    pub note: String,
    pub tone: BandTone,
}

/// The three cells at the top of a run: where it is, whose turn it is and what to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    pub place: BandText,
    pub who: BandText,
    pub next: BandText,
    /// One mark per step, empty for a command without a rail.
    pub marks: Vec<StepState>,
    pub tally: String,
}

/// The state of every step of the rail, or `None` for a command without one.
pub fn step_states(run: &Run) -> Option<Vec<StepState>> {
    let rail = run.steps.as_ref()?;
    let (current, next) = match &run.status {
        RunStatus::Failed(_) | RunStatus::Stopped => (StepState::Failed, StepState::Next),
        RunStatus::Closed => (StepState::Done, StepState::Next),
        _ if run.needs_user() => (StepState::Waiting, StepState::Next),
        // Claude is done for now and asks nothing, so no step waits.
        RunStatus::AwaitingReply if !run.is_busy() && !wants_reply(run) => (StepState::Done, StepState::Next),
        RunStatus::AwaitingReply if !run.is_busy() => {
            if stop_point(run).is_some() { (StepState::Done, StepState::Waiting) } else { (StepState::Waiting, StepState::Next) }
        }
        _ => (StepState::Working, StepState::Next),
    };
    let states = (0..rail.labels.len()).map(|step| match step {
        step if step < rail.current => StepState::Done,
        step if step == rail.current => current,
        step if step == rail.current + 1 => next,
        _ => StepState::Next,
    });
    Some(states.collect())
}

/// One line per step that holds rows, in order.
pub fn step_lines(run: &Run) -> Vec<StepLine> {
    let (Some(rail), Some(states)) = (run.steps.as_ref(), step_states(run)) else { return Vec::new() };
    (0..rail.labels.len())
        .filter(|step| run.activity.iter().any(|activity| activity.step == Some(*step)))
        .map(|step| StepLine {
            step,
            name: rail.labels[step],
            state: states[step],
            sentence: sentence(run, step),
            live: step == rail.current && matches!(states[step], StepState::Working | StepState::Waiting),
        })
        .collect()
}

fn tools_of(run: &Run, step: usize) -> impl Iterator<Item = (&str, &str, &ToolState)> {
    run.activity.iter().filter(move |activity| activity.step == Some(step)).filter_map(|activity| match &activity.kind {
        ActivityKind::Tool { name, summary, state, .. } => Some((name.as_str(), summary.as_str(), state)),
        _ => None,
    })
}

/// "Wrote the CV and the cover letter. Updating the tracker now."
/// Only the command's `&'static` phrases and counts go in, so nothing Claude or a posting wrote can reach it.
fn sentence(run: &Run, step: usize) -> String {
    let phrases = phrases_for(&run.line);
    let phrase_of = |name: &str, summary: &str| phrases.iter().find(|phrase| phrase.on.hits(name, summary));
    let rows: Vec<_> = tools_of(run, step).collect();
    let newest = rows.last().filter(|(_, _, state)| **state == ToolState::Running).and_then(|(name, summary, _)| phrase_of(name, summary));
    let mut clauses: Vec<&str> = Vec::new();
    let mut other = 0;
    for (at, (name, summary, state)) in rows.iter().enumerate() {
        match (state, phrase_of(name, summary)) {
            (ToolState::Done, Some(phrase)) => {
                if !clauses.contains(&phrase.done) {
                    clauses.push(phrase.done);
                }
            }
            (ToolState::Running, Some(_)) if newest.is_some() && at + 1 == rows.len() => {}
            _ => other += 1,
        }
    }
    let mut text = match clauses.as_slice() {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    };
    if other > 0 {
        if text.is_empty() {
            text = format!("{other} {}", action_noun(other));
        } else {
            text.push_str(&format!(", and {other} other {}", action_noun(other)));
        }
    }
    if !text.is_empty() {
        let mut chars = text.chars();
        text = chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default();
        text.push('.');
    }
    match newest {
        Some(phrase) if text.is_empty() => phrase.now.to_owned(),
        Some(phrase) => format!("{text} {}", phrase.now),
        None => text,
    }
}

/// "action" or "actions", for `count` of them.
pub fn action_noun(count: usize) -> &'static str {
    if count == 1 { "action" } else { "actions" }
}

pub fn band(run: &Run) -> Band {
    let states = step_states(run);
    let done = states.iter().flatten().filter(|state| **state == StepState::Done).count();
    let tally = states.as_ref().map(|states| format!("{done} of {} steps done", states.len())).unwrap_or_default();
    let (place, who, next) = cells(run, done);
    Band { place, who, next, marks: states.unwrap_or_default(), tally }
}

fn say(value: &str, note: &str, tone: BandTone) -> BandText {
    BandText { value: value.to_owned(), note: note.to_owned(), tone }
}

/// "2 Draft" with an optional ending, or the command word when the command has no rail.
fn where_value(run: &Run, ending: &str) -> String {
    match run.steps.as_ref().and_then(|rail| rail.labels.get(rail.current).map(|label| (rail.current, label))) {
        Some((at, label)) => format!("{at} {label}{ending}"),
        None => command_word(&run.line),
    }
}

fn what_is_asked(prompt: &Prompt) -> String {
    let asked = match &prompt.kind {
        PromptKind::Questions(questions) => questions.first().map(|question| first_line(&question.header)).unwrap_or_default(),
        PromptKind::Permission { tool, .. } => match first_line(tool) {
            tool if tool.is_empty() => "Allow or deny a tool".to_owned(),
            tool => format!("Allow or deny {tool}"),
        },
    };
    if asked.is_empty() { "Answer Claude's question".to_owned() } else { asked }
}

fn cells(run: &Run, done: usize) -> (BandText, BandText, BandText) {
    use BandTone::{Claude, Failed, Plain, Yours};
    let rail = run.steps.as_ref();
    let nobody = |note: &str| say("Nobody", note, Plain);
    let resume = say("Reply to resume", "Or leave it. Files so far stay.", Plain);
    match &run.status {
        RunStatus::Failed(reason) => {
            // A stderr tail ends on its error.
            let reason = last_line(reason);
            let reason = if reason.is_empty() { "The run ended with an error." } else { &reason };
            (say(&where_value(run, ", failed"), "", Failed), nobody(reason), resume)
        }
        RunStatus::Stopped => (say(&where_value(run, ", stopped"), "", Plain), nobody("You stopped this run"), resume),
        RunStatus::Closed => match rail {
            Some(rail) if rail.is_complete() => {
                let finished = run.line.split_whitespace().next() == Some("/apply");
                let next = if finished {
                    say("Read the documents", "Open the dossier, then send it yourself.", Plain)
                } else {
                    say("Nothing for now", "Reply to resume this session.", Plain)
                };
                (say(&format!("All {} steps", rail.labels.len()), "", Plain), nobody("The session is closed"), next)
            }
            Some(_) => (say(&where_value(run, ""), "", Plain), nobody("The session is closed"), say("Reply to resume", "", Plain)),
            None => (say(&where_value(run, ""), "", Plain), nobody("The session is closed"), say("Nothing for now", "Reply to resume this session.", Plain)),
        },
        _ if run.needs_user() => {
            let place_note = if rail.is_some() { "Paused inside the step" } else { "" };
            let asked = run.current_prompt().map(what_is_asked).unwrap_or_default();
            (say(&where_value(run, ""), place_note, Plain), say("Yours", "Claude asked", Yours), say(&asked, "The choices are right below.", Plain))
        }
        RunStatus::AwaitingReply if !run.is_busy() && !wants_reply(run) => {
            let place = match rail {
                Some(rail) if rail.is_complete() => format!("All {} steps", rail.labels.len()),
                _ => where_value(run, ""),
            };
            (say(&place, "", Plain), nobody("Claude is done for now"), say("Reply to continue", "Or end the session.", Plain))
        }
        RunStatus::AwaitingReply if !run.is_busy() => {
            let (place, place_note) = match (stop_point(run), rail) {
                (Some(_), Some(rail)) => (where_value(run, ", done"), rail.labels.get(rail.current + 1).map(|label| format!("{label} waits for your go")).unwrap_or_default()),
                (None, Some(_)) => (where_value(run, ""), "Paused inside the step".to_owned()),
                _ => (where_value(run, ""), String::new()),
            };
            let next = if closing_question(run).is_some() {
                say("Answer Claude", "Its question is right below.", Plain)
            } else {
                say("Reply to continue", "Or end the session.", Plain)
            };
            (say(&place, &place_note, Plain), say("Yours", "Claude stopped", Yours), next)
        }
        status => {
            let starting = *status == RunStatus::Starting;
            let stopping = *status == RunStatus::Stopping;
            let (place_note, who_note, next_note) = match (starting, stopping, rail) {
                (true, _, _) => ("Starting the CLI".to_owned(), "", ""),
                (_, true, Some(rail)) => (format!("{done} of {} steps done", rail.labels.len()), "Stopping", ""),
                (_, true, None) => (String::new(), "Stopping", ""),
                (_, _, Some(rail)) => (format!("{done} of {} steps done", rail.labels.len()), "Claude is working", "You can leave. The Runs tab marks it when it needs you."),
                (_, _, None) => (String::new(), "Claude is working", "You can leave. The Runs tab marks it when it needs you."),
            };
            (say(&where_value(run, ""), &place_note, Plain), say("Claude's", who_note, Claude), say("Nothing for now", next_note, Plain))
        }
    }
}

/// A tool's path as the folder and the file, with the workspace root cut from the front.
/// A path that climbs out with `..` stays whole, so it cannot pass for one inside the workspace.
/// The folder keeps its closing slash.
pub fn short_path(root: &str, path: &str) -> (String, String) {
    let path = path.replace('\\', "/");
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    let inside = match path.strip_prefix(root).and_then(|rest| rest.strip_prefix('/')) {
        Some(rest) if !root.is_empty() && !rest.split('/').any(|part| part == "..") => rest,
        _ => &path,
    };
    let (dir, file) = inside.split_at(inside.rfind('/').map_or(0, |at| at + 1));
    (visible_chars(dir).collect(), visible_chars(file).collect())
}

pub fn elapsed_text(ms: u64) -> String {
    let seconds = ms / 1000;
    match seconds / 60 {
        0 => format!("{seconds}s"),
        minutes => format!("{minutes}m {}s", seconds % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::domain::{AgentEvent, Question, RunRequest};

    fn run(line: &str) -> Run {
        Run::new(&RunRequest { run_id: "1".into(), line: line.into(), model: None, budget_usd: 1.0, started_at: String::new(), resume_session: None })
    }

    fn started(line: &str) -> Run {
        let mut run = run(line);
        run.apply(AgentEvent::SessionStarted { session_id: "s".into(), model: "m".into(), api_key_source: "none".into() });
        run
    }

    fn begin(run: &mut Run, id: &str, name: &str, summary: &str) {
        run.apply(AgentEvent::ToolStarted { id: id.into(), name: name.into(), summary: summary.into(), nested: false });
    }

    fn does(run: &mut Run, name: &str, summary: &str) {
        let id = run.activity.len().to_string();
        begin(run, &id, name, summary);
        run.apply(AgentEvent::ToolFinished { id, outcome: Ok(()) });
    }

    fn says(run: &mut Run, text: &str) {
        run.apply(AgentEvent::AssistantText { text: text.into(), nested: false });
    }

    fn ends_turn(run: &mut Run) {
        run.apply(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.5, duration_ms: 10 });
    }

    fn asks(run: &mut Run, header: &str) {
        let question = Question { text: "Which?".into(), header: header.into(), options: vec![], multi_select: false };
        let kind = PromptKind::Questions(vec![question]);
        run.apply(AgentEvent::PromptRaised(Prompt { request_id: "q".into(), kind, input_json: "{}".into(), suggestions_json: None }));
    }

    /// A run of `/apply` that evaluated and stopped for its go.
    fn at_the_stop() -> Run {
        let mut run = started("/apply https://jobs.example/1");
        does(&mut run, "Read", ".claude/skills/a/04-job-evaluation.md");
        says(&mut run, "# Job fit\n\nShall I go ahead and draft the documents?");
        ends_turn(&mut run);
        run
    }

    fn at_draft() -> Run {
        let mut run = at_the_stop();
        run.note_reply("Yes, proceed with drafting.");
        does(&mut run, "Write", "cv/main_acme.tex");
        does(&mut run, "Write", "cv/main_acme_2.tex");
        run
    }

    fn cell(value: &str, note: &str, tone: BandTone) -> BandText {
        say(value, note, tone)
    }

    #[test]
    fn starting_reads_nothing_for_now() {
        let band = band(&run("/apply x"));
        assert_eq!(band.place, cell("0 Fetch", "Starting the CLI", BandTone::Plain));
        assert_eq!(band.who, cell("Claude's", "", BandTone::Claude));
        assert_eq!(band.next, cell("Nothing for now", "", BandTone::Plain));
        assert_eq!(band.tally, "0 of 7 steps done");
    }

    #[test]
    fn working_counts_the_steps_done() {
        let band = band(&at_draft_working());
        assert_eq!(band.place, cell("2 Draft", "2 of 7 steps done", BandTone::Plain));
        assert_eq!(band.who, cell("Claude's", "Claude is working", BandTone::Claude));
        assert_eq!(band.next, cell("Nothing for now", "You can leave. The Runs tab marks it when it needs you.", BandTone::Plain));
        assert_eq!(band.tally, "2 of 7 steps done");
        assert_eq!(band.marks[..4], [StepState::Done, StepState::Done, StepState::Working, StepState::Next]);
    }

    fn at_draft_working() -> Run {
        let mut run = at_draft();
        run.apply(AgentEvent::SessionStarted { session_id: "s".into(), model: "m".into(), api_key_source: "none".into() });
        run
    }

    #[test]
    fn a_known_stop_is_yours_with_the_next_step_waiting() {
        let run = at_the_stop();
        let band = band(&run);
        assert_eq!(band.place, cell("1 Evaluate, done", "Draft waits for your go", BandTone::Plain));
        assert_eq!(band.who, cell("Yours", "Claude stopped", BandTone::Yours));
        assert_eq!(band.next, cell("Answer Claude", "Its question is right below.", BandTone::Plain));
        assert_eq!(band.tally, "2 of 7 steps done");
        assert_eq!(step_states(&run).unwrap()[..4], [StepState::Done, StepState::Done, StepState::Waiting, StepState::Next]);
    }

    #[test]
    fn an_unknown_stop_waits_in_its_step() {
        let mut run = started("/apply x");
        does(&mut run, "Write", "cv/main_a.tex");
        says(&mut run, "Which template do you prefer?");
        ends_turn(&mut run);
        let band = band(&run);
        assert_eq!(band.place, cell("2 Draft", "Paused inside the step", BandTone::Plain));
        assert_eq!(band.who, cell("Yours", "Claude stopped", BandTone::Yours));
        assert_eq!(band.next, cell("Answer Claude", "Its question is right below.", BandTone::Plain));
        assert_eq!(step_states(&run).unwrap()[2], StepState::Waiting);
    }

    #[test]
    fn a_stop_without_a_question_is_nobodys_turn() {
        let mut run = started("/apply x");
        says(&mut run, "Done for now.");
        ends_turn(&mut run);
        let band = band(&run);
        assert_eq!(band.place, cell("0 Fetch", "", BandTone::Plain));
        assert_eq!(band.who, cell("Nobody", "Claude is done for now", BandTone::Plain));
        assert_eq!(band.next, cell("Reply to continue", "Or end the session.", BandTone::Plain));
        assert_eq!(step_states(&run).unwrap()[..2], [StepState::Done, StepState::Next]);
    }

    #[test]
    fn a_finished_rank_that_asks_nothing_is_done_for_now() {
        let mut run = started("/rank");
        does(&mut run, "Bash", "python3 rank_state.py apply");
        says(&mut run, "Ranked 12 postings.");
        ends_turn(&mut run);
        let band = band(&run);
        assert_eq!(band.place, cell("All 3 steps", "", BandTone::Plain));
        assert_eq!(band.who, cell("Nobody", "Claude is done for now", BandTone::Plain));
        assert_eq!(band.next, cell("Reply to continue", "Or end the session.", BandTone::Plain));
        assert_eq!(band.tally, "3 of 3 steps done");
        assert!(band.marks.iter().all(|mark| *mark == StepState::Done));
    }

    #[test]
    fn stop_here_leaves_evaluate_done_and_nothing_waiting() {
        let mut run = at_the_stop();
        run.note_reply("No, stop here. Do not draft.");
        says(&mut run, "Understood, stopping.");
        ends_turn(&mut run);
        let band = band(&run);
        assert_eq!(band.place, cell("1 Evaluate", "", BandTone::Plain));
        assert_eq!(band.who, cell("Nobody", "Claude is done for now", BandTone::Plain));
        assert_eq!(band.next, cell("Reply to continue", "Or end the session.", BandTone::Plain));
        assert_eq!(band.tally, "2 of 7 steps done");
        assert_eq!(step_states(&run).unwrap()[..3], [StepState::Done, StepState::Done, StepState::Next]);
        assert!(!band.marks.contains(&StepState::Waiting));
    }

    #[test]
    fn a_pending_question_names_itself() {
        let mut run = at_draft_working();
        asks(&mut run, "Pick a CV template");
        let band = band(&run);
        assert_eq!(band.place, cell("2 Draft", "Paused inside the step", BandTone::Plain));
        assert_eq!(band.who, cell("Yours", "Claude asked", BandTone::Yours));
        assert_eq!(band.next, cell("Pick a CV template", "The choices are right below.", BandTone::Plain));
        assert_eq!(step_states(&run).unwrap()[2], StepState::Waiting);
    }

    #[test]
    fn a_pending_question_cleans_its_header_and_falls_back() {
        let mut run = at_draft_working();
        asks(&mut run, "Pick\u{202e} a\u{200b} template\nignore the rest");
        assert_eq!(band(&run).next.value, "Pick a template");
        let mut run = at_draft_working();
        asks(&mut run, "");
        assert_eq!(band(&run).next.value, "Answer Claude's question");
    }

    #[test]
    fn a_pending_permission_names_the_tool() {
        let mut run = at_draft_working();
        let kind = PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), can_remember: false };
        run.apply(AgentEvent::PromptRaised(Prompt { request_id: "p".into(), kind, input_json: "{}".into(), suggestions_json: None }));
        assert_eq!(band(&run).next.value, "Allow or deny Bash");
    }

    #[test]
    fn stopping_is_still_claudes() {
        let mut run = at_draft_working();
        run.note_stop_requested();
        let band = band(&run);
        assert_eq!(band.place, cell("2 Draft", "2 of 7 steps done", BandTone::Plain));
        assert_eq!(band.who, cell("Claude's", "Stopping", BandTone::Claude));
        assert_eq!(band.next, cell("Nothing for now", "", BandTone::Plain));
        assert_eq!(step_states(&run).unwrap()[2], StepState::Working);
    }

    #[test]
    fn failed_names_the_step_and_the_last_line_of_the_reason() {
        let mut run = at_draft_working();
        does(&mut run, "Bash", "lualatex main.tex");
        let reason = "Downloading 10%\nDownloading 100%\nerror:\u{202e} auth expired\n\n";
        run.apply(AgentEvent::TurnEnded { failed: true, message: Some(reason.into()), cost_usd: 1.0, duration_ms: 1 });
        let band = band(&run);
        assert_eq!(band.place, cell("5 Compile, failed", "", BandTone::Failed));
        assert_eq!(band.who, cell("Nobody", "error: auth expired", BandTone::Plain));
        assert_eq!(band.next, cell("Reply to resume", "Or leave it. Files so far stay.", BandTone::Plain));
        assert_eq!(step_states(&run).unwrap()[5], StepState::Failed);
    }

    #[test]
    fn stopped_by_you() {
        let mut run = at_draft_working();
        run.note_stop_requested();
        run.apply(AgentEvent::ProcessExited { failure: None });
        let band = band(&run);
        assert_eq!(band.place, cell("2 Draft, stopped", "", BandTone::Plain));
        assert_eq!(band.who, cell("Nobody", "You stopped this run", BandTone::Plain));
        assert_eq!(band.next.value, "Reply to resume");
        assert_eq!(step_states(&run).unwrap()[2], StepState::Failed);
    }

    #[test]
    fn finished_apply_points_at_the_documents() {
        let mut run = at_draft_working();
        does(&mut run, "Edit", "/w/job_search_tracker.csv");
        run.apply(AgentEvent::ProcessExited { failure: None });
        let band = band(&run);
        assert_eq!(band.place, cell("All 7 steps", "", BandTone::Plain));
        assert_eq!(band.who, cell("Nobody", "The session is closed", BandTone::Plain));
        assert_eq!(band.next, cell("Read the documents", "Open the dossier, then send it yourself.", BandTone::Plain));
        assert_eq!(band.tally, "7 of 7 steps done");
        assert!(band.marks.iter().all(|mark| *mark == StepState::Done));
    }

    #[test]
    fn finished_without_documents_has_nothing_to_read() {
        let mut run = started("/rank");
        does(&mut run, "Bash", "python3 rank_state.py apply");
        run.apply(AgentEvent::ProcessExited { failure: None });
        let band = band(&run);
        assert_eq!(band.place.value, "All 3 steps");
        assert_eq!(band.next, cell("Nothing for now", "Reply to resume this session.", BandTone::Plain));
    }

    #[test]
    fn ending_early_is_not_all_steps() {
        let mut run = at_draft_working();
        run.apply(AgentEvent::ProcessExited { failure: None });
        let band = band(&run);
        assert_eq!(band.place.value, "2 Draft");
        assert_eq!(band.who, cell("Nobody", "The session is closed", BandTone::Plain));
        assert_eq!(band.next.value, "Reply to resume");
        assert_eq!(step_states(&run).unwrap()[..4], [StepState::Done, StepState::Done, StepState::Done, StepState::Next]);
    }

    #[test]
    fn without_a_rail_the_command_word_stands_in() {
        let mut run = started("/outcome acme");
        let working = band(&run);
        assert_eq!(working.place, cell("/outcome", "", BandTone::Plain));
        assert_eq!(working.tally, "");
        assert!(working.marks.is_empty() && step_states(&run).is_none() && step_lines(&run).is_empty());
        ends_turn(&mut run);
        assert_eq!(band(&run).who, cell("Nobody", "Claude is done for now", BandTone::Plain));
        run.note_reply("Log the offer.");
        says(&mut run, "Shall I log it?");
        ends_turn(&mut run);
        assert_eq!(band(&run).who.tone, BandTone::Yours);
        run.apply(AgentEvent::ProcessExited { failure: None });
        let closed = band(&run);
        assert_eq!((closed.place.value.as_str(), closed.next.value.as_str()), ("/outcome", "Nothing for now"));
    }

    fn fetched() -> Run {
        let mut run = started("/apply x");
        does(&mut run, "Agent", "posting-fetcher: get the posting");
        does(&mut run, "Bash", "python3 tools/posting_guard.py --in posting.txt");
        run
    }

    #[test]
    fn sentence_matches_the_canvas() {
        assert_eq!(sentence(&fetched(), 0), "Fetched the posting and checked it for hidden instructions.");
        let mut run = fetched();
        does(&mut run, "Bash", "bun run tools/cli.ts search");
        assert_eq!(sentence(&run, 0), "Fetched the posting, checked it for hidden instructions and read it from the job board.");
    }

    #[test]
    fn repeated_clauses_are_said_once() {
        let mut run = fetched();
        does(&mut run, "Agent", "posting-fetcher: again");
        assert_eq!(sentence(&run, 0), "Fetched the posting and checked it for hidden instructions.");
    }

    #[test]
    fn unmatched_rows_are_counted() {
        let mut run = fetched();
        does(&mut run, "Bash", "ls");
        does(&mut run, "Read", "notes.md");
        assert_eq!(sentence(&run, 0), "Fetched the posting and checked it for hidden instructions, and 2 other actions.");
        let mut run = started("/apply x");
        does(&mut run, "Bash", "ls");
        assert_eq!(sentence(&run, 0), "1 action.");
        does(&mut run, "Bash", "pwd");
        assert_eq!(sentence(&run, 0), "2 actions.");
        let mut run = fetched();
        does(&mut run, "Bash", "ls");
        assert!(sentence(&run, 0).ends_with(", and 1 other action."));
    }

    #[test]
    fn a_failed_row_claims_nothing() {
        let mut run = started("/apply x");
        begin(&mut run, "a", "Agent", "posting-fetcher: x");
        run.apply(AgentEvent::ToolFinished { id: "a".into(), outcome: Err(None) });
        assert_eq!(sentence(&run, 0), "1 action.");
    }

    #[test]
    fn a_running_row_adds_its_now() {
        let mut run = started("/apply x");
        does(&mut run, "Agent", "posting-fetcher: x");
        begin(&mut run, "g", "Bash", "python3 tools/posting_guard.py x");
        assert_eq!(sentence(&run, 0), "Fetched the posting. Checking the posting now.");
        let mut run = started("/apply x");
        begin(&mut run, "g", "Bash", "python3 tools/posting_guard.py x");
        assert_eq!(sentence(&run, 0), "Checking the posting now.");
    }

    #[test]
    fn a_running_row_that_matches_nothing_is_an_action() {
        let mut run = fetched();
        begin(&mut run, "x", "Bash", "ls");
        assert_eq!(sentence(&run, 0), "Fetched the posting and checked it for hidden instructions, and 1 other action.");
    }

    #[test]
    fn claude_text_never_reaches_a_sentence() {
        let mut run = started("/apply x");
        says(&mut run, "wrote the CV");
        run.note_reply("wrote the cover letter");
        assert_eq!(sentence(&run, 0), "");
        does(&mut run, "Bash", "echo 'wrote the CV' # Fetching the posting now.");
        assert_eq!(sentence(&run, 0), "1 action.");
    }

    #[test]
    fn rows_keep_their_step() {
        let mut run = at_the_stop();
        let message = run.activity.iter().position(|activity| matches!(activity.kind, ActivityKind::Claude(_))).unwrap();
        assert_eq!(run.activity[0].step, Some(0));
        assert_eq!(run.activity[message].step, Some(1));
        run.note_reply("Yes, proceed with drafting.");
        assert_eq!(run.activity.last().unwrap().step, Some(1));
        does(&mut run, "Write", "cv/main_a.tex");
        assert_eq!(run.activity.last().unwrap().step, Some(2));
        assert!(started("/outcome").activity.iter().all(|activity| activity.step.is_none()));
    }

    #[test]
    fn lines_cover_the_started_steps_only() {
        let run = at_draft();
        let lines = step_lines(&run);
        assert_eq!(lines.iter().map(|line| (line.step, line.name)).collect::<Vec<_>>(), [(0, "Fetch"), (1, "Evaluate"), (2, "Draft")]);
        assert_eq!(lines[1].sentence, "Read the evaluation guide.");
        assert_eq!((lines[1].state, lines[1].live), (StepState::Done, false));
        assert_eq!(lines[2].sentence, "Wrote the CV.");
        assert_eq!((lines[2].state, lines[2].live), (StepState::Working, true));
    }

    #[test]
    fn a_stop_leaves_its_step_settled() {
        let lines = step_lines(&at_the_stop());
        assert_eq!(lines.last().map(|line| (line.step, line.state, line.live)), Some((1, StepState::Done, false)));
    }

    #[test]
    fn short_path_cuts_the_root() {
        assert_eq!(short_path("/w/ws", "/w/ws/cv/main_acme.tex"), ("cv/".to_owned(), "main_acme.tex".to_owned()));
        assert_eq!(short_path("/w/ws/", "/w/ws/job_search_tracker.csv"), (String::new(), "job_search_tracker.csv".to_owned()));
        assert_eq!(short_path("C:\\w", "C:\\w\\cv\\main_a.tex"), ("cv/".to_owned(), "main_a.tex".to_owned()));
    }

    #[test]
    fn short_path_keeps_outside_paths_whole() {
        assert_eq!(short_path("/w/ws", "/w/ws/../etc/passwd"), ("/w/ws/../etc/".to_owned(), "passwd".to_owned()));
        assert_eq!(short_path("/w/ws", "/w/ws2/x.md"), ("/w/ws2/".to_owned(), "x.md".to_owned()));
        assert_eq!(short_path("/w/ws", "/home/me/x.md"), ("/home/me/".to_owned(), "x.md".to_owned()));
        assert_eq!(short_path("", "/a/b.md"), ("/a/".to_owned(), "b.md".to_owned()));
    }

    #[test]
    fn elapsed_text_reads_minutes_and_seconds() {
        assert_eq!(elapsed_text(45_000), "45s");
        assert_eq!(elapsed_text(860_000), "14m 20s");
    }
}
