//! Renders every screen in both themes from a sample workspace.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, NaiveDateTime};
use egui::{Key, Theme};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, SnapshotOptions};
use jobdesk::runs::application::{
    AgentEvent, AgentGateway, AgentSession, Decision, EventSink, Prompt, PromptKind, Question, QuestionOption, RunDesk, RunRequest, StartOptions, Usage,
};
use jobdesk::shell::{Screen, Shell, Snapshot, install_look};
use jobdesk::workspace::application::{EnvironmentChecks, ToolProber, ToolSpec};

fn sample_workspace(root: &Path) {
    let source = Path::new("tests/sample_workspace");
    copy_tree(source, root);
    fs::create_dir_all(root.join(".claude/commands")).unwrap();
    fs::write(root.join(".claude/commands/apply.md"), "").unwrap();
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A stable folder keeps the name in the sidebar identical between runs.
fn shell(screen: Screen, name: &str) -> Shell {
    shell_on(screen, name, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap())
}

fn shell_on(screen: Screen, name: &str, today: NaiveDate) -> Shell {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    let snapshot = Snapshot::load(&root, today);
    assert!(snapshot.issues.is_empty(), "{:?}", snapshot.issues);
    Shell::with_snapshot(screen, jobdesk::workspace::open(&root).unwrap(), snapshot)
}

type SharedSink = Arc<Mutex<Option<EventSink>>>;

/// Plays back events the way the real gateway would.
struct ScriptedGateway(SharedSink);
struct ScriptedSession;

impl AgentGateway for ScriptedGateway {
    fn start(&self, _: &RunRequest, sink: EventSink) -> Box<dyn AgentSession> {
        *self.0.lock().unwrap() = Some(sink);
        Box::new(ScriptedSession)
    }
}

impl AgentSession for ScriptedSession {
    fn send_message(&mut self, _: &str) {}
    fn respond(&mut self, _: &Prompt, _: &Decision) {}
    fn interrupt(&mut self) {}
}

/// What a recording gateway saw: each process it started, and each message sent to a live one.
#[derive(Default)]
struct Calls {
    starts: usize,
    sent: Vec<String>,
}

type SharedCalls = Arc<Mutex<Calls>>;

/// A scripted gateway that also records its calls.
struct RecordingGateway(SharedSink, SharedCalls);
struct RecordingSession(SharedCalls);

impl AgentGateway for RecordingGateway {
    fn start(&self, _: &RunRequest, sink: EventSink) -> Box<dyn AgentSession> {
        self.1.lock().unwrap().starts += 1;
        *self.0.lock().unwrap() = Some(sink);
        Box::new(RecordingSession(Arc::clone(&self.1)))
    }
}

impl AgentSession for RecordingSession {
    fn send_message(&mut self, text: &str) {
        self.0.lock().unwrap().sent.push(text.to_owned());
    }
    fn respond(&mut self, _: &Prompt, _: &Decision) {}
    fn interrupt(&mut self) {}
}

fn tool(id: &str, name: &str, summary: &str, nested: bool) -> AgentEvent {
    AgentEvent::ToolStarted { id: id.into(), name: name.into(), summary: summary.into(), nested }
}

fn done(id: &str) -> AgentEvent {
    AgentEvent::ToolFinished { id: id.into(), outcome: Ok(()) }
}

fn failed(id: &str, reason: &str) -> AgentEvent {
    AgentEvent::ToolFinished { id: id.into(), outcome: Err(Some(reason.into())) }
}

/// Start times show on screen, so scripted runs start at a fixed moment.
fn run_started() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(9, 30, 0).unwrap()
}

fn apply_run(final_event: AgentEvent) -> RunDesk {
    apply_run_ending([final_event])
}

fn apply_run_ending(final_events: impl IntoIterator<Item = AgentEvent>) -> RunDesk {
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    // Permission prompts, as the coming setting will show them.
    desk.ask_permission();
    desk.start(APPLY_LINE, &StartOptions { model: None, budget_usd: 5.0 }, run_started()).unwrap();
    let events = [
        AgentEvent::SessionStarted { session_id: "a5ce0bcc-22f9".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() },
        AgentEvent::UsageChanged(Usage { five_hour: Some(0.08), seven_day: Some(0.25) }),
        tool("1", "Agent", "posting-fetcher: Fetch the posting", false),
        tool("2", "Bash", "python3 tools/posting_guard.py --text --json", true),
        done("2"),
        done("1"),
        tool("3", "Read", ".claude/skills/job-application-assistant/04-job-evaluation.md", false),
        done("3"),
        AgentEvent::AssistantText { text: "**Fit 78, Strong Fit.** Event sourcing and Kafka match. Card schemes are a gap.\n\n| Area | Match |\n|---|---|\n| Skills | Strong |\n| Experience | Strong |".into(), nested: false },
        tool("4", "Write", "cv/main_kestrel_solution_architect.tex", false),
        done("4"),
        tool("5", "Agent", "application-reviewer: Research and critique", false),
        done("5"),
        tool("6", "Edit", "cv/main_kestrel_solution_architect.tex", false),
        done("6"),
        tool("8", "Bash", "ls -Z \"$SCRATCHPAD\"", false),
        failed("8", "ls: invalid option -- 'Z'\nTry 'ls --help' for more information."),
        tool("7", "Bash", "cd cv && lualatex -interaction=nonstopmode main_kestrel_solution_architect.tex", false),
    ];
    let guard = sink.lock().unwrap();
    let emit = guard.as_ref().unwrap();
    events.into_iter().chain(final_events).for_each(emit);
    drop(guard);
    desk
}

/// An `/apply` run that evaluated the posting, asked whether to draft, and ended its turn.
fn tray_waiting_apply_run() -> RunDesk {
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    desk.start("/apply https://kestrel.example/jobs/2", &StartOptions { model: None, budget_usd: 5.0 }, run_started()).unwrap();
    let events = [
        AgentEvent::SessionStarted { session_id: "a5ce0bcc-22f9".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() },
        tool("1", "Read", ".claude/skills/job-application-assistant/04-job-evaluation.md", false),
        done("1"),
        AgentEvent::AssistantText { text: "**Fit 78, Strong Fit.** Event sourcing and Kafka match. Card schemes are a gap.\n\nShall I proceed with drafting?".into(), nested: false },
        AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.94, duration_ms: 401_000 },
    ];
    let guard = sink.lock().unwrap();
    let emit = guard.as_ref().unwrap();
    events.into_iter().for_each(emit);
    drop(guard);
    desk
}

fn permission_prompt() -> AgentEvent {
    AgentEvent::PromptRaised(Prompt {
        request_id: "p".into(),
        kind: PromptKind::Permission {
            tool: "Bash".into(),
            detail: "cd cv && lualatex -interaction=nonstopmode main_kestrel_solution_architect.tex".into(),
            can_remember: true,
        },
        input_json: "{}".into(),
        suggestions_json: Some("[]".into()),
    })
}

fn question_prompt() -> AgentEvent {
    let option = |label: &str, description: &str| QuestionOption { label: label.into(), description: description.into() };
    AgentEvent::PromptRaised(Prompt {
        request_id: "q".into(),
        kind: PromptKind::Questions(vec![Question {
            text: "Layout check found a hole on page 1. How should the CV be fixed?".into(),
            header: "Layout".into(),
            options: vec![option("Shorten the next entry", "Keeps content order."), option("Cut one older bullet", "Lowest relevance to this posting.")],
            multi_select: false,
        }]),
        input_json: "{}".into(),
        suggestions_json: None,
    })
}

fn stop_point() -> AgentEvent {
    AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.94, duration_ms: 401_000 }
}

const APPLY_LINE: &str = "/apply https://kestrel.example/jobs/2";
const STOP_EVALUATION: &str = "# Job fit evaluation: Solution Architect at Kestrel Payments\n\nVerdict: Strong Fit, 78 out of 100. Every hard gate passes, and the two gaps are small.\n\n| Gate | In the posting | Result |\n|---|---|---|\n| Contract | Permanent, full time | Pass |\n| Work mode | Hybrid, 2 days a week in Leuven | Pass |\n| Language | English, Dutch is a plus | Pass |\n| Salary | Not stated | Check |\n\nShall I go ahead and draft the CV and cover letter?";

/// /apply stopped after Evaluate, with Claude's question closing its evaluation.
fn apply_stop_after_evaluate() -> RunDesk {
    stop_after_evaluate(SharedCalls::default())
}

/// The same stop, on a gateway that records into `calls`.
fn stop_after_evaluate(calls: SharedCalls) -> RunDesk {
    stop_with(calls, STOP_EVALUATION).0
}

/// /apply stopped after Evaluate on `message`, with the sink to play more events into.
fn stop_with(calls: SharedCalls, message: &str) -> (RunDesk, SharedSink) {
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(RecordingGateway(Arc::clone(&sink), calls)), Arc::new(|| {}));
    desk.start(APPLY_LINE, &StartOptions { model: None, budget_usd: 3.0 }, run_started()).unwrap();
    let events = [
        AgentEvent::SessionStarted { session_id: "3f9a21c0-5d7e".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() },
        AgentEvent::UsageChanged(Usage { five_hour: Some(0.08), seven_day: Some(0.25) }),
        tool("1", "Agent", "posting-fetcher: Fetch the posting", false),
        tool("2", "Bash", "python3 tools/posting_guard.py --text --json", true),
        done("2"),
        done("1"),
        tool("3", "Read", ".claude/skills/job-application-assistant/04-job-evaluation.md", false),
        done("3"),
        tool("4", "Bash", "ls salary_data.json salary_lookup.py job_search_tracker.csv", false),
        done("4"),
        tool("5", "Bash", "python3 salary_lookup.py \"Kestrel\" --json", false),
        done("5"),
        AgentEvent::AssistantText { text: message.into(), nested: false },
        AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 1.12, duration_ms: 303_000 },
    ];
    let guard = sink.lock().unwrap();
    let emit = guard.as_ref().unwrap();
    events.into_iter().for_each(emit);
    drop(guard);
    (desk, sink)
}

fn emit_all(sink: &SharedSink, events: impl IntoIterator<Item = AgentEvent>) {
    let guard = sink.lock().unwrap();
    let emit = guard.as_ref().unwrap();
    events.into_iter().for_each(emit);
}

const GO_AHEAD: &str = "Yes, proceed with drafting.";

/// /apply past its stop: the CV is written, and the cover letter is being written.
fn apply_drafting() -> (RunDesk, SharedSink) {
    let (mut desk, sink) = stop_with(SharedCalls::default(), STOP_EVALUATION);
    desk.poll();
    let id = desk.runs()[0].id.clone();
    desk.reply(&id, GO_AHEAD).unwrap();
    emit_all(&sink, [
        tool("6", "Read", ".claude/skills/job-application-assistant/05-cv-tailoring.md", false),
        done("6"),
        tool("7", "Write", "cv/main_kestrel_solution_architect.tex", false),
        done("7"),
        tool("8", "Write", "cover_letters/cover_kestrel_solution_architect.tex", false),
    ]);
    (desk, sink)
}

const SUMMARY: &str = "# Application ready: Kestrel Payments, Solution Architect\n\nThe CV (2 pages) and the cover letter (1 page) are built, and both checks passed.\n\n## What changed after review\n\n- 6 reviewer edits applied. 1 was skipped because your profile does not back it.\n- The letter now opens with the payments migration.\n\n## Next\n\n- Read both PDFs in the dossier.\n- Send it yourself. Jobdesk never sends applications.";

/// /apply through Finish, its closing summary, and the process gone.
fn apply_finished() -> RunDesk {
    let (desk, sink) = apply_drafting();
    emit_all(&sink, [
        done("8"),
        tool("9", "Agent", "application-reviewer: Research and critique", false),
        done("9"),
        tool("10", "Edit", "cv/main_kestrel_solution_architect.tex", false),
        done("10"),
        tool("11", "Edit", "cover_letters/cover_kestrel_solution_architect.tex", false),
        done("11"),
        tool("12", "Bash", "cd cv && lualatex -interaction=nonstopmode main_kestrel_solution_architect.tex", false),
        done("12"),
        tool("13", "Bash", "python3 tools/verify_pdf.py cv/main_kestrel_solution_architect.pdf", false),
        done("13"),
        tool("14", "Edit", "job_search_tracker.csv", false),
        done("14"),
        AgentEvent::AssistantText { text: SUMMARY.into(), nested: false },
        AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 2.41, duration_ms: 557_000 },
        AgentEvent::ProcessExited { failure: None },
    ]);
    desk
}

fn render(screen: Screen, theme: Theme, name: &str) {
    render_with(shell(screen, name), theme, name);
}

fn render_run(final_event: AgentEvent, theme: Theme, name: &str) {
    render_desk(apply_run(final_event), theme, name);
}

fn render_desk(desk: RunDesk, theme: Theme, name: &str) {
    let mut shell = shell(Screen::Runs, name);
    *shell.desk_slot() = Some(desk);
    render_with(shell, theme, name);
}

fn render_launcher(theme: Theme, name: &str) {
    let mut shell = shell(Screen::Launcher, name);
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(SharedSink::default())), Arc::new(|| {})));
    render_with(shell, theme, name);
}

fn render_with(mut shell: Shell, theme: Theme, name: &str) {
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_pixels_per_point(1.5)
        .with_theme(theme)
        .with_context_setup(install_look)
        .wgpu()
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    harness.snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
}

macro_rules! screens {
    ($($name:ident: $screen:expr, $theme:expr;)*) => {
        $(#[test]
        fn $name() {
            render($screen, $theme, stringify!($name));
        })*
    };
}

/// Insights button, launcher form, run, then the Runs screen.
#[test]
fn a_command_button_leads_to_a_started_run() {
    let mut shell = shell(Screen::Insights, "click_through");
    let gateway = ScriptedGateway(SharedSink::default());
    *shell.desk_slot() = Some(RunDesk::new(Box::new(gateway), Arc::new(|| {})));
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_context_setup(install_look)
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(2);

    harness.get_by_label("Tune scoring").click();
    harness.run_steps(3);
    harness.get_by_label("All commands");

    harness.get_by_label("Run").click();
    harness.run_steps(3);

    harness.get_by_label("Recent runs");
    harness.get_by_label("Where the run is");
    assert_eq!(harness.get_all_by_label("Starting").count(), 1, "recent runs, as the head has no tag");
}

/// However long the activity, the prompt's buttons stay in the window.
#[test]
fn a_long_activity_keeps_the_prompt_visible() {
    let mut shell = shell(Screen::Runs, "long_activity");
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    // Permission prompts, as the coming setting will show them.
    desk.ask_permission();
    desk.start("/rank --limit 10", &StartOptions { model: None, budget_usd: 2.0 }, run_started()).unwrap();
    let guard = sink.lock().unwrap();
    let emit = guard.as_ref().unwrap();
    for index in 0..200 {
        let id = index.to_string();
        emit(tool(&id, "Read", &format!("jobs/posting_{index}.md"), false));
        emit(done(&id));
    }
    emit(permission_prompt());
    drop(guard);
    *shell.desk_slot() = Some(desk);
    let harness = tray_harness(shell);
    assert_in_window(&harness, [1280.0, 820.0], &["Allow once", "Deny"]);
}

fn assert_in_window(harness: &Harness, size: [f32; 2], labels: &[&str]) {
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, size.into());
    for label in labels {
        let rect = harness.get_by_label(label).rect();
        assert!(window.contains_rect(rect), "{label} at {rect:?}");
    }
}

/// The smallest window the app allows, with a run's step rail above the prompt.
fn smallest_window(desk: RunDesk, name: &str) -> Harness<'static> {
    let mut shell = shell(Screen::Runs, name);
    *shell.desk_slot() = Some(desk);
    let mut harness = Harness::builder()
        .with_size([1040.0, 640.0])
        .with_context_setup(install_look)
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    harness
}

#[test]
fn a_permission_prompt_fits_the_smallest_window() {
    let harness = smallest_window(apply_run(permission_prompt()), "smallest_permission");
    assert_in_window(&harness, [1040.0, 640.0], &["Allow once", "Allow for this run", "Deny"]);
}

#[test]
fn a_question_with_many_choices_fits_the_smallest_window() {
    let harness = smallest_window(apply_run(many_choices("q")), "smallest_question");
    assert_in_window(&harness, [1040.0, 640.0], &["Send answer", "Decline"]);
}

#[test]
fn a_stop_point_fits_the_smallest_window() {
    let harness = smallest_window(apply_stop_after_evaluate(), "smallest_stop_point");
    assert_in_window(&harness, [1040.0, 640.0], &["Go ahead with the draft", "Stop here", "Send", "End session…"]);
}

/// A quick reply is an ordinary reply on the same run: it starts no other run.
#[test]
fn a_quick_reply_is_sent_as_a_reply() {
    let mut shell = shell(Screen::Runs, "quick_reply");
    let calls = SharedCalls::default();
    let (desk, sink) = stop_with(Arc::clone(&calls), STOP_EVALUATION);
    *shell.desk_slot() = Some(desk);
    let mut harness = tray_harness(shell);
    let lines = harness.query_all_by_label(APPLY_LINE).count();

    harness.get_by_label("Go ahead with the draft").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, "\u{201c}Yes, proceed with drafting.\u{201d}"), "the story quotes the reply");
    assert!(!is_shown(&harness, "Go ahead with the draft"));
    assert!(!is_shown(&harness, "Stop here"));
    assert_eq!(harness.query_all_by_label(APPLY_LINE).count(), lines, "Recent runs still lists one /apply run");
    let calls = calls.lock().unwrap();
    assert_eq!(calls.starts, 1, "no other process started");
    assert_eq!(calls.sent, ["Yes, proceed with drafting."], "the reply reached the live session");
    drop(calls);

    // Claude ends its next turn with a question, and the field comes back empty.
    emit_all(&sink, [
        AgentEvent::AssistantText { text: "Before I draft: shall I lead with the Kafka migration?".into(), nested: false },
        AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 1.3, duration_ms: 20_000 },
    ]);
    harness.run_steps(3);
    assert!(is_shown(&harness, "Send"));
    let field = harness.get_by_role(egui::accesskit::Role::TextInput);
    assert_eq!(field.value().as_deref().unwrap_or_default(), "", "the field is empty");
}

/// A quick reply never wipes a draft: while the field holds text, the quick replies wait.
#[test]
fn a_quick_reply_waits_while_a_reply_is_drafted() {
    let mut shell = shell(Screen::Runs, "quick_reply_draft");
    let calls = SharedCalls::default();
    *shell.desk_slot() = Some(stop_after_evaluate(Arc::clone(&calls)));
    let mut harness = tray_harness(shell);
    let field = harness.get_by_role(egui::accesskit::Role::TextInput);
    field.focus();
    field.type_text("Lead with the Kafka work");
    harness.run_steps(2);
    for label in ["Go ahead with the draft", "Stop here"] {
        assert!(harness.get_by_label(label).accesskit_node().is_disabled(), "{label} waits for the draft");
    }
    assert!(!harness.get_by_label("Send").accesskit_node().is_disabled(), "the draft can be sent");

    harness.get_by_label("Go ahead with the draft").click();
    harness.run_steps(3);

    assert_eq!(harness.get_by_role(egui::accesskit::Role::TextInput).value().as_deref(), Some("Lead with the Kafka work"), "the draft stays");
    assert!(calls.lock().unwrap().sent.is_empty(), "nothing was sent");
}

/// After Stop here and Claude's plain goodbye, the run is done for now: a reply is welcome, not awaited.
#[test]
fn stop_here_ends_your_turn() {
    let mut shell = shell(Screen::Runs, "stop_here");
    let (desk, sink) = stop_with(SharedCalls::default(), STOP_EVALUATION);
    *shell.desk_slot() = Some(desk);
    let mut harness = tray_harness(shell);
    harness.get_by_label("Stop here").click();
    harness.run_steps(3);
    emit_all(&sink, [
        AgentEvent::AssistantText { text: "Understood, stopping. The evaluation stays in the tracker.".into(), nested: false },
        AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 1.2, duration_ms: 4_000 },
    ]);
    harness.run_steps(3);

    assert!(harness.query_all_by_label("Your turn").count() == 0, "no run is tagged Your turn");
    assert!(harness.query_all_by_label("Ready for reply").count() > 0, "the tag reads Ready for reply");
    assert!(is_shown(&harness, "Claude is done for now"));
    assert!(harness.query_by_label_contains("Tasks, ").is_none(), "the Tasks tab has no mark");
    assert!(harness.query_by_label_contains("needs you").is_none());
    assert!(is_shown(&harness, "Send"), "the reply field stays under the story");
    assert!(!is_shown(&harness, "Go ahead with the draft"));
}

/// Allowances fold behind their step's toggle, even in a step with no actions.
#[test]
fn allowances_stay_folded_in_a_step_without_actions() {
    let mut shell = shell(Screen::Runs, "allowance_folded");
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    desk.start(APPLY_LINE, &StartOptions { model: None, budget_usd: 3.0 }, run_started()).unwrap();
    emit_all(&sink, [
        AgentEvent::SessionStarted { session_id: "3f9a21c0-5d7e".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() },
        permission_prompt(),
        tool("1", "Read", ".claude/skills/job-application-assistant/04-job-evaluation.md", false),
        done("1"),
    ]);
    *shell.desk_slot() = Some(desk);
    let mut harness = tray_harness(shell);
    let allowed = "Allowed Bash without asking.";
    assert!(!is_shown(&harness, allowed), "folded while the toggle is closed");

    harness.get_by_label("Show details").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, allowed));
    assert!(is_shown(&harness, "Hide details"));
}

/// A command without steps names its one row Details when it holds no actions.
#[test]
fn a_command_without_steps_folds_allowances_under_details() {
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    desk.start("/outcome kestrel", &StartOptions { model: None, budget_usd: 1.0 }, run_started()).unwrap();
    emit_all(&sink, [permission_prompt()]);
    let mut harness = tray_harness_with(desk, "no_rail_details");
    assert!(is_shown(&harness, "Details"));
    assert!(!is_shown(&harness, "Allowed Bash without asking."));
    harness.get_by_label("Show details").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Allowed Bash without asking."));
}

/// A failed tool's reason is cleaned and capped as a notice is: its last lines, with no blank ones.
#[test]
fn a_failed_tool_shows_the_last_lines_of_its_reason() {
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    desk.start("/outcome kestrel", &StartOptions { model: None, budget_usd: 1.0 }, run_started()).unwrap();
    let reason = (1..=10).map(|at| format!("Downloading {at}0%\n")).collect::<String>() + "\nerror:\u{202e} auth expired";
    emit_all(&sink, [tool("1", "Bash", "bun run tools/cli.ts fetch", false), failed("1", &reason)]);
    let mut harness = tray_harness_with(desk, "tool_reason_lines");
    harness.get_by_label("Show 1 action so far").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "error: auth expired"));
    assert!(is_shown(&harness, "Downloading 40%"));
    for cut in ["Downloading 10%", "Downloading 30%"] {
        assert!(!is_shown(&harness, cut), "{cut} is past the cap");
    }
}

/// While another run works, the stop's replies wait, and the bar says why.
#[test]
fn replies_wait_while_another_run_works() {
    let mut shell = shell(Screen::Runs, "replies_blocked");
    let calls = SharedCalls::default();
    let mut desk = stop_after_evaluate(Arc::clone(&calls));
    desk.poll();
    desk.start("/rank --limit 10", &StartOptions { model: None, budget_usd: 2.0 }, run_started()).unwrap();
    *shell.desk_slot() = Some(desk);
    let mut harness = tray_harness(shell);
    // The stopped run is the one row tagged Your turn: the newest run, shown first, is starting.
    harness.get_by_label("Your turn").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, "Another run is working. You can reply when it finishes."));
    let field = harness.get_by_role(egui::accesskit::Role::TextInput);
    field.focus();
    field.type_text("Draft it, please.");
    harness.run_steps(2);
    for label in ["Go ahead with the draft", "Stop here", "Send"] {
        assert!(harness.get_by_label(label).accesskit_node().is_disabled(), "{label} is disabled");
    }

    press(&mut harness, Key::Enter);
    let calls = calls.lock().unwrap();
    assert!(calls.sent.is_empty(), "Enter sends nothing");
    assert_eq!(calls.starts, 2, "the stopped run and the rank run, and no process for the reply");
    drop(calls);
    assert!(!is_shown(&harness, "\u{201c}Draft it, please.\u{201d}"));
}

/// The story folds each step's actions behind its toggle, closed until asked.
#[test]
fn actions_open_only_when_asked() {
    let mut shell = shell(Screen::Runs, "actions_toggle");
    *shell.desk_slot() = Some(apply_run(stop_point()));
    let mut harness = tray_harness(shell);
    let file = "main_kestrel_solution_architect.tex";
    let named = |harness: &Harness| harness.query_all_by_label_contains(file).count();
    assert_eq!(named(&harness), 0, "nowhere while every toggle is closed");

    // Draft's toggle is the one on Draft's line.
    let draft = harness.get_by_label("Draft").rect().center().y;
    let toggle = harness.get_all_by_label("Show 1 action").find(|toggle| (toggle.rect().center().y - draft).abs() < 4.0).expect("Draft's toggle");
    toggle.click();
    harness.run_steps(3);
    assert_eq!(named(&harness), 1, "Draft's Write row");
    assert!(is_shown(&harness, "Hide actions"));

    harness.get_by_label("Hide actions").click();
    harness.run_steps(3);
    assert_eq!(named(&harness), 0);
}

/// At a stop, the story opens on the top of Claude's newest message, not on its last line.
#[test]
fn a_stop_opens_on_the_newest_message() {
    let mut shell = shell(Screen::Runs, "stop_opens_on_message");
    *shell.desk_slot() = Some(apply_stop_after_evaluate());
    let harness = tray_harness(shell);
    let story = harness.get_by_label("The run so far").rect();
    let heading = harness.get_by_label_contains("Job fit evaluation").rect();
    assert!(heading.top() > story.bottom(), "{heading:?} is under the story's heading at {story:?}");
    assert!(heading.top() < story.bottom() + 40.0, "{heading:?} opens at the top of the story under {story:?}");
}

#[test]
fn a_finished_run_has_no_head_action() {
    let mut shell = shell(Screen::Runs, "finished_head");
    *shell.desk_slot() = Some(apply_finished());
    let harness = tray_harness(shell);
    assert!(is_shown(&harness, "All 7 steps"));
    assert!(is_shown(&harness, "The run"));
    for label in ["Stop run", "Force stop", "End session…", "Run details"] {
        assert!(!is_shown(&harness, label), "{label} is in the head");
    }
}

/// An AskUserQuestion stays answerable in the smallest window, with word of a background agent under the story.
#[test]
fn a_question_stays_answerable_in_the_smallest_window() {
    let desk = apply_run_ending([many_choices("q"), AgentEvent::BackgroundTasksChanged { count: 1 }]);
    let mut harness = smallest_window(desk, "smallest_question_footer");
    assert!(is_shown(&harness, "1 background agent is still working. Claude continues when it reports back."));
    assert_in_window(&harness, [1040.0, 640.0], &["Choice 1", "Send answer", "Decline"]);
    harness.get_by_label("Choice 1").click();
    harness.run_steps(3);
    assert!(!harness.get_by_label("Send answer").accesskit_node().is_disabled(), "Send answer is enabled");
}

const LONG_QUESTION: &str = "Every hard gate passes.\n\nShall I go ahead and draft the CV and the cover letter for the Solution Architect role at Kestrel Payments, leading with the event-sourcing work and the Kafka migration, keeping the CV to two pages and the letter to one, naming the hybrid days in Leuven as fine, and noting in the letter that the posting states no salary, so the range can be asked for on the first call with the hiring manager, or would you rather stop here and look at the evaluation first?";

/// The turn bar keeps Send in the smallest window, with a long question, the blocked line and the quick replies.
#[test]
fn the_turn_bar_fits_the_smallest_window() {
    let (mut desk, _sink) = stop_with(SharedCalls::default(), LONG_QUESTION);
    desk.poll();
    desk.start("/rank --limit 10", &StartOptions { model: None, budget_usd: 2.0 }, run_started()).unwrap();
    let mut harness = smallest_window(desk, "smallest_turn_bar");
    // The newest run shows first. The stopped one is in Run details.
    harness.get_by_label("Run details").click();
    harness.run_steps(3);
    harness.get_by_label("Your turn").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Another run is working. You can reply when it finishes."));
    assert!(!is_shown(&harness, "Recent runs"), "picking a run closes Run details");
    assert_in_window(&harness, [1040.0, 640.0], &["Go ahead with the draft", "Stop here", "Send", "End session…", "Run details"]);
}

/// The theme switch, which sits in the status line.
const STATUS_LINE: &str = "System";

/// A reply takes a stop's story back to following the run: its newest row comes into view.
#[test]
fn a_resumed_run_is_followed_again() {
    let (desk, sink) = stop_with(SharedCalls::default(), STOP_EVALUATION);
    let mut harness = smallest_window(desk, "resumed_follow");
    harness.get_by_label("Go ahead with the draft").click();
    harness.run_steps(3);
    emit_all(&sink, [
        tool("6", "Read", ".claude/skills/job-application-assistant/05-cv-tailoring.md", false),
        done("6"),
        tool("7", "Write", "cv/main_kestrel_solution_architect.tex", false),
    ]);
    harness.run_steps(3);
    let draft = harness.get_by_label("Draft").rect();
    let status = harness.get_by_label(STATUS_LINE).rect();
    assert!(draft.bottom() <= status.top(), "Draft's line at {draft:?} is below the story, over the status line at {status:?}");
    assert!(draft.top() >= harness.get_by_label("The run so far").rect().bottom(), "Draft's line at {draft:?} is above the story");
}

const COMPILE_FAILED: &str = "lualatex stopped: main_kestrel_solution_architect.tex has an undefined control sequence on line 88.";

/// Why a run failed shows in the story, outside the step's toggle, even where the compact band drops its notes.
#[test]
fn a_failure_reason_shows_without_opening_the_step() {
    let failed = AgentEvent::TurnEnded { failed: true, message: Some(COMPILE_FAILED.into()), cost_usd: 1.4, duration_ms: 90_000 };
    let harness = smallest_window(apply_run_ending([failed]), "failure_reason_shown");
    assert!(!is_shown(&harness, "Hide actions"), "every toggle is closed");
    assert_in_window(&harness, [1040.0, 640.0], &[COMPILE_FAILED]);
    assert!(harness.get_by_label(COMPILE_FAILED).rect().bottom() <= harness.get_by_label(STATUS_LINE).rect().top());
}

/// A failure reason is untrusted: the story draws it without its hidden characters.
#[test]
fn a_failure_reason_shows_without_hidden_characters() {
    let failed = AgentEvent::TurnEnded { failed: true, message: Some("Compile\u{202e} broke\u{200b} on page 2.".into()), cost_usd: 1.4, duration_ms: 90_000 };
    let harness = smallest_window(apply_run_ending([failed]), "failure_reason_visible");
    assert!(is_shown(&harness, "Compile broke on page 2."));
    for hidden in ["\u{202e}", "\u{200b}"] {
        assert!(harness.query_by_label_contains(hidden).is_none(), "{hidden:?} reaches the screen");
    }
}

/// A stderr tail shows whole in the story, so the error at its end is never hidden behind its first line.
#[test]
fn a_failure_reason_shows_its_last_lines() {
    let reason = "Downloading 10%\nDownloading 55%\n\nDownloading 100%\nerror: auth expired";
    let failed = AgentEvent::ProcessExited { failure: Some(reason.into()) };
    let harness = tray_harness_with(apply_run_ending([failed]), "failure_reason_lines");
    for line in ["Downloading 10%", "Downloading 100%"] {
        assert!(is_shown(&harness, line), "{line} shows");
    }
    // The story's notice, and the band's note, which keeps the last line.
    assert_eq!(harness.query_all_by_label("error: auth expired").count(), 2);
}

/// A turn Claude begins on its own leaves the reader where they were.
#[test]
fn a_turn_claude_begins_alone_does_not_move_the_story() {
    let (desk, sink) = stop_with(SharedCalls::default(), STOP_EVALUATION);
    let mut harness = smallest_window(desk, "self_started_turn");
    let offset = |harness: &Harness| harness.get_by_label_contains("Job fit evaluation").rect().top() - harness.get_by_label("The run so far").rect().bottom();
    let before = offset(&harness);
    emit_all(&sink, [
        AgentEvent::SessionStarted { session_id: "3f9a21c0-5d7e".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() },
        tool("6", "Read", "notes/kestrel.md", false),
        done("6"),
        tool("7", "Bash", "ls cv", false),
    ]);
    harness.run_steps(3);
    assert!((offset(&harness) - before).abs() <= 1.0, "the story moved from {before} to {}", offset(&harness));
}

/// A run that failed with its process still up can end its session, through the same popover.
#[test]
fn an_idle_failed_session_can_be_ended() {
    let mut shell = shell(Screen::Runs, "end_failed_session");
    let failed = AgentEvent::TurnEnded { failed: true, message: Some(COMPILE_FAILED.into()), cost_usd: 1.4, duration_ms: 90_000 };
    *shell.desk_slot() = Some(apply_run_ending([failed]));
    let mut harness = tray_harness(shell);
    harness.get_by_label("End session…").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Keep it open"), "the popover asks first");
    harness.get_by_label("End session").click();
    harness.run_steps(3);
    assert!(!is_shown(&harness, "End session…"), "the session is gone");
    assert!(harness.query_all_by_label(COMPILE_FAILED).count() > 0, "the run keeps why it failed");
}

/// A click on a Recent runs row's command text picks the run, as a click on its tag does.
#[test]
fn a_recent_run_is_picked_by_its_command_text() {
    let mut shell = shell(Screen::Runs, "recent_by_text");
    let mut desk = apply_stop_after_evaluate();
    desk.poll();
    desk.start("/rank --limit 10", &StartOptions { model: None, budget_usd: 2.0 }, run_started()).unwrap();
    *shell.desk_slot() = Some(desk);
    let mut harness = tray_harness(shell);
    assert_eq!(harness.query_all_by_label(APPLY_LINE).count(), 1, "only Recent runs lists the stopped run");

    harness.get_by_label(APPLY_LINE).click();
    harness.run_steps(3);

    assert_eq!(harness.query_all_by_label(APPLY_LINE).count(), 2, "the head and Recent runs");
}

#[test]
fn esc_keeps_the_session_open() {
    let mut shell = shell(Screen::Runs, "end_session_esc");
    *shell.desk_slot() = Some(apply_stop_after_evaluate());
    let mut harness = tray_harness(shell);
    harness.get_by_label("End session…").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Keep it open"));

    press(&mut harness, Key::Escape);

    assert!(!is_shown(&harness, "Keep it open"));
    assert!(harness.query_all_by_label("Your turn").count() > 0);
    assert!(is_shown(&harness, "End session…"));
}

#[test]
fn quick_replies_only_at_a_known_stop() {
    let mut shell = shell(Screen::Runs, "no_quick_replies");
    *shell.desk_slot() = Some(apply_run(stop_point()));
    let harness = tray_harness(shell);
    assert!(is_shown(&harness, "Send"));
    assert!(!is_shown(&harness, "Go ahead with the draft"));
}

#[test]
fn end_session_asks_first() {
    let mut shell = shell(Screen::Runs, "end_session");
    *shell.desk_slot() = Some(apply_stop_after_evaluate());
    let mut harness = tray_harness(shell);
    let your_turn = |harness: &Harness| harness.query_all_by_label("Your turn").count() > 0;

    harness.get_by_label("End session…").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "End session"), "the popover asks first");
    assert!(your_turn(&harness), "the click alone ends nothing");
    harness.get_by_label("Keep it open").click();
    harness.run_steps(3);
    assert!(your_turn(&harness));
    assert!(!is_shown(&harness, "Keep it open"));

    harness.get_by_label("End session…").click();
    harness.run_steps(3);
    harness.get_by_label("End session").click();
    harness.run_steps(3);
    assert!(!your_turn(&harness));
}

fn many_choices(request_id: &str) -> AgentEvent {
    let choices = (1..=8).map(|index| QuestionOption { label: format!("Choice {index}"), description: "Keeps the CV on two pages.".into() }).collect();
    let question = Question { text: "Layout check found a hole on page 1. How should the CV be fixed?".into(), header: "Layout".into(), options: choices, multi_select: false };
    AgentEvent::PromptRaised(Prompt { request_id: request_id.into(), kind: PromptKind::Questions(vec![question]), input_json: "{}".into(), suggestions_json: None })
}

/// Claude's replies are untrusted: an image in one shows as its alt text and loads nothing.
#[test]
fn a_reply_shows_an_image_as_its_alt_text() {
    let mut shell = shell(Screen::Runs, "reply_image");
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    desk.start("/rank --limit 10", &StartOptions { model: None, budget_usd: 2.0 }, run_started()).unwrap();
    let text = "![Tracking pixel](https://tracker.example/p.png)";
    sink.lock().unwrap().as_ref().unwrap()(AgentEvent::AssistantText { text: text.into(), nested: false });
    *shell.desk_slot() = Some(desk);
    let harness = tray_harness(shell);
    assert!(is_shown(&harness, "Tracking pixel"));
}

/// Cmd or Ctrl K opens the launcher while a field has focus, and the field never sees it:
/// on Windows a text field takes Ctrl K to delete to the end of the line.
#[test]
fn the_launcher_shortcut_works_while_typing() {
    let mut shell = shell(Screen::Runs, "shortcut_typing");
    *shell.desk_slot() = Some(apply_stop_after_evaluate());
    let mut harness = tray_harness(shell);
    let field = harness.get_by_role(egui::accesskit::Role::TextInput);
    field.focus();
    field.type_text("keep this");
    harness.run_steps(2);
    harness.key_press(Key::Home);
    harness.key_press_modifiers(egui::Modifiers { ctrl: true, command: true, ..Default::default() }, Key::K);
    harness.run_steps(3);
    harness.get_by_label("All commands");
    harness.get_by_label("Tasks, 1 need you").click();
    harness.run_steps(3);
    assert_eq!(harness.get_by_role(egui::accesskit::Role::TextInput).value().as_deref(), Some("keep this"));
}

#[test]
fn the_tabs_use_plain_names() {
    let harness = tray_harness(shell(Screen::InTray, "plain_tabs"));
    for name in ["Today", "Jobs", "Applications", "Insights", "Tasks", "Setup"] {
        harness.get_by_label_contains(name);
    }
    assert!(harness.query_by_label_contains("In-tray").is_none());
}

/// A square marks each job with a card in Today, and the legend under the table says what it means.
#[test]
fn jobs_mark_what_waits_in_today() {
    let mut harness = tray_harness(shell(Screen::Jobs, "jobs_waiting"));
    let legend = "Waiting in Today. Decide there, or open the drafting task from here.";
    assert!(is_shown(&harness, legend));
    let marks = harness.query_all_by_label("Waiting in Today").count();
    assert_eq!(marks, 4 + 1, "every shortlisted job waits, and the legend has its own square");

    harness.get_by_label_contains("Skipped").click();
    harness.run_steps(3);
    assert!(!is_shown(&harness, legend));
    assert!(!is_shown(&harness, "Waiting in Today"));
}

/// The frame a prompt arrives in shows it, before it is measured and with no second pass to redraw in.
/// A taller prompt after a measured one in the same run starts from no measure too.
#[test]
fn the_first_frame_shows_the_prompt_in_one_pass() {
    let mut shell = shell(Screen::Runs, "first_frame");
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    desk.ask_permission();
    desk.start("/rank --limit 10", &StartOptions { model: None, budget_usd: 2.0 }, run_started()).unwrap();
    let session = AgentEvent::SessionStarted { session_id: "b7d2e410-93c1".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() };
    sink.lock().unwrap().as_ref().unwrap()(session);
    for index in 0..60 {
        let id = index.to_string();
        sink.lock().unwrap().as_ref().unwrap()(tool(&id, "Read", &format!("jobs/posting_{index}.md"), false));
    }
    *shell.desk_slot() = Some(desk);
    let mut harness = Harness::builder()
        .with_size([1040.0, 640.0])
        .with_context_setup(|ctx| {
            install_look(ctx);
            ctx.options_mut(|options| options.max_passes = std::num::NonZeroUsize::MIN);
        })
        .build_ui(move |ui| shell.show(ui));
    assert!(!is_shown(&harness, "Allow once"));
    sink.lock().unwrap().as_ref().unwrap()(permission_prompt());
    harness.run_steps(1);
    assert_in_window(&harness, [1040.0, 640.0], &["Allow once", "Deny"]);

    // The short reply box gets measured, then a tall question takes its place.
    harness.run_steps(3);
    harness.get_by_label("Allow once").click();
    harness.run_steps(3);
    assert!(!is_shown(&harness, "Allow once"));
    sink.lock().unwrap().as_ref().unwrap()(stop_point());
    harness.run_steps(3);
    assert!(is_shown(&harness, "Send"));
    sink.lock().unwrap().as_ref().unwrap()(many_choices("q2"));
    harness.run_steps(1);
    assert_in_window(&harness, [1040.0, 640.0], &["Send answer", "Decline"]);
}

fn tray_harness(mut shell: Shell) -> Harness<'static> {
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_context_setup(install_look)
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    harness
}

fn tray_harness_with(desk: RunDesk, name: &str) -> Harness<'static> {
    let mut shell = shell(Screen::Runs, name);
    *shell.desk_slot() = Some(desk);
    tray_harness(shell)
}

fn press(harness: &mut Harness, key: Key) {
    harness.key_press(key);
    harness.run_steps(3);
}

fn is_shown(harness: &Harness, label: &str) -> bool {
    harness.query_by_label(label).is_some()
}

#[test]
fn applications_mark_what_waits_in_today() {
    let harness = tray_harness(shell(Screen::Tracker, "applications_waits"));
    assert!(is_shown(&harness, "Granite Cloud, Platform Lead, No reply in 15 days, waits for you in Today"));
    assert!(is_shown(&harness, "Sable Analytics, Tech Lead, Sent 3 days ago"), "a card with nothing in Today stays plain");
    assert!(is_shown(&harness, "Mosaic Data, Architect, You withdrew"));
    assert!(harness.query_by_label_contains("of them wait for you in Today.").is_some());
    assert!(is_shown(&harness, "A pink edge means the application waits for you in Today."));
}

const TRAY_CARDS: usize = 10;
const HARBOR: &str = "Lead Java Engineer";
const MERIDIAN: &str = "Tech Lead Java, Real-Time Traffic";

/// Y on a card, launcher form, run, then the Runs screen.
#[test]
fn y_leads_from_a_card_to_a_started_run() {
    let mut shell = shell(Screen::InTray, "tray_click_through");
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(SharedSink::default())), Arc::new(|| {})));
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, HARBOR));

    press(&mut harness, Key::Y);
    harness.get_by_label("All commands");
    harness.get_by_label("/outcome");

    harness.get_by_label("Run").click();
    harness.run_steps(3);
    harness.get_by_label("Recent runs");
}

#[test]
fn later_deals_the_next_card_and_undo_brings_it_back() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_later"));

    press(&mut harness, Key::L);
    assert!(!is_shown(&harness, HARBOR));
    let row = "Last decision on Harbor Bank: Set aside until tomorrow. Press U to undo.";
    assert!(is_shown(&harness, row));
    assert!(is_shown(&harness, "Set aside until tomorrow"));
    assert!(!is_shown(&harness, "Each decision lands here. U undoes the last one."), "the undo row heads the rail");

    press(&mut harness, Key::U);
    assert!(is_shown(&harness, HARBOR));
    assert!(!is_shown(&harness, row));
    assert!(is_shown(&harness, "Each decision lands here. U undoes the last one."), "setting aside clears nothing");
}

#[test]
fn a_timeline_mark_pulls_a_set_aside_card_back() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_mark"));
    press(&mut harness, Key::L);
    assert!(!is_shown(&harness, HARBOR));

    harness.get_by_label("Tue 3, Harbor Bank deadline").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, HARBOR));
}

#[test]
fn bring_them_back_refills_the_tray() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_bring_back"));
    for _ in 0..TRAY_CARDS {
        press(&mut harness, Key::L);
    }
    assert!(is_shown(&harness, "All done for today"));
    assert!(!is_shown(&harness, &format!("{TRAY_CARDS} set aside")));

    assert!(is_shown(&harness, "Want more to look at?"));
    harness.get_by_label("Bring them back now").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, HARBOR));
    assert!(!is_shown(&harness, "Want more to look at?"));
}

/// The buttons open the launcher with the line filled in, and the user starts the run there.
#[test]
fn find_new_jobs_opens_the_launcher_and_starts_nothing() {
    let started = SharedSink::default();
    let mut shell = shell(Screen::InTray, "tray_find_new_jobs");
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&started))), Arc::new(|| {})));
    let mut harness = tray_harness(shell);
    for _ in 0..TRAY_CARDS {
        press(&mut harness, Key::L);
    }
    assert!(is_shown(&harness, "Want more to look at?"));
    assert!(is_shown(&harness, "Score 2 new jobs…"), "the sample board holds two unscored jobs");
    assert!(is_shown(&harness, &format!("{TRAY_CARDS} cards set aside with Later come back tomorrow. ")));
    assert!(!is_shown(&harness, "All commands"));

    harness.get_by_label("Find new jobs…").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, "All commands"), "the launcher opens");
    assert!(harness.query_all_by_label("/scrape").count() > 0, "the line is filled in");
    assert!(started.lock().unwrap().is_none(), "no run starts");
}

/// Later leaves a card out of the count, a skip ticks one off the stamp.
#[test]
fn the_head_counts_what_needs_you() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_head"));
    assert!(is_shown(&harness, &format!("{TRAY_CARDS} things need you")));
    assert!(is_shown(&harness, &format!("Saturday 31 October. 0 of {TRAY_CARDS} decided today.")));
    assert!(is_shown(&harness, &format!("1 of {TRAY_CARDS}")));
    assert!(!is_shown(&harness, &format!("{} behind this one", TRAY_CARDS - 1)));

    press(&mut harness, Key::L);
    let left = TRAY_CARDS - 1;
    assert!(is_shown(&harness, &format!("{left} things need you")));
    assert!(is_shown(&harness, &format!("Saturday 31 October. 0 of {left} decided today.")));
    assert!(is_shown(&harness, &format!("1 of {left}")));

    press(&mut harness, Key::L);
    press(&mut harness, Key::N);
    harness.get_by_label("Domain").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, &format!("{} things need you", left - 2)));
    assert!(is_shown(&harness, &format!("Saturday 31 October. 1 of {} decided today.", left - 1)));
}

/// The decided card slides off from a copy while the next one already takes keys, and nothing moves after 150 ms.
#[test]
fn a_decision_settles_within_the_animation_time() {
    let mut shell = shell(Screen::InTray, "tray_motion");
    let frame = 1.0 / 60.0;
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_step_dt(frame)
        .with_context_setup(install_look)
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);

    harness.key_press(Key::L);
    harness.run_steps(2);
    assert!(is_shown(&harness, HARBOR), "the decided card's copy is still sliding off");
    harness.key_press(Key::L);
    harness.run_steps(1);
    assert!(is_shown(&harness, &format!("1 of {}", TRAY_CARDS - 2)), "a key during the slide decides the new top card");

    harness.run_steps((0.15_f32 / frame).ceil() as usize);
    assert!(!is_shown(&harness, HARBOR));
    assert!(is_shown(&harness, MERIDIAN));
    harness.run();
}

#[test]
fn skip_takes_a_reason_and_only_some_cards_allow_it() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_skip"));

    press(&mut harness, Key::N);
    assert!(is_shown(&harness, HARBOR), "a send card has no skip");

    for _ in 0..2 {
        press(&mut harness, Key::L);
    }
    assert!(is_shown(&harness, MERIDIAN));
    press(&mut harness, Key::N);
    assert!(!is_shown(&harness, MERIDIAN));

    harness.get_by_label("Domain").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Skipped for domain"));
    assert!(is_shown(&harness, "Last decision on Meridian Rail Systems: Skipped for domain. Press U to undo."));
    assert_eq!(harness.query_all_by_label("Meridian Rail Systems").count(), 1, "the logged skip is not drawn twice");
    assert_eq!(harness.get_by_label("Domain").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
}

#[test]
fn a_paused_run_comes_first_and_y_does_nothing() {
    let mut shell = shell(Screen::InTray, "tray_permission");
    *shell.desk_slot() = Some(apply_run(permission_prompt()));
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, "/apply: allow Bash?"));

    press(&mut harness, Key::Y);
    press(&mut harness, Key::N);
    assert!(is_shown(&harness, "/apply: allow Bash?"));

    harness.get_by_label("Open task").click();
    harness.run_steps(3);
    harness.get_by_label("Recent runs");
}

#[test]
fn a_waiting_run_deals_a_your_turn_card() {
    let mut shell = shell(Screen::InTray, "tray_turn");
    *shell.desk_slot() = Some(tray_waiting_apply_run());
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, "/apply: proceed with drafting?"));
    assert!(is_shown(&harness, "Your turn"));

    press(&mut harness, Key::Y);
    press(&mut harness, Key::N);
    assert!(is_shown(&harness, "/apply: proceed with drafting?"));

    harness.get_by_label("Open task").click();
    harness.run_steps(3);
    harness.get_by_label("Recent runs");
}

#[test]
fn a_guard_card_has_no_default_and_a_kept_posting_reaches_triage() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_guard"));
    for _ in 0..4 {
        press(&mut harness, Key::L);
    }
    let guarded = "Lead Developer Kafka";
    assert!(is_shown(&harness, guarded));
    press(&mut harness, Key::Y);
    assert!(is_shown(&harness, guarded));

    harness.get_by_label("Harmless, keep posting").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Last decision on Halden Logistics: Kept. Press U to undo."));
}

/// The slip's note and its tally both read the count.
fn tallied(harness: &Harness, done: usize, total: usize) -> bool {
    harness.query_all_by_label(&format!("{done} of {total}")).count() == 2
}

/// Later leaves a card out of the total, a skip ticks one off.
#[test]
fn the_tally_counts_what_is_done() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_count"));
    assert!(tallied(&harness, 0, TRAY_CARDS));

    press(&mut harness, Key::L);
    assert!(tallied(&harness, 0, TRAY_CARDS - 1));
    press(&mut harness, Key::L);
    press(&mut harness, Key::N);
    assert!(tallied(&harness, 1, TRAY_CARDS - 2));
}

#[test]
fn bring_back_returns_set_aside_cards_without_a_run() {
    let mut shell = shell(Screen::InTray, "tray_bring_back_row");
    let started = SharedSink::default();
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&started))), Arc::new(|| {})));
    let mut harness = tray_harness(shell);
    press(&mut harness, Key::L);
    assert!(is_shown(&harness, "1 set aside, back tomorrow: Harbor Bank. Bring it back now."));

    harness.get_by_label("Bring back").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, HARBOR), "the card brought back is on top again");
    assert!(tallied(&harness, 0, TRAY_CARDS));
    assert!(harness.query_by_label_contains("set aside, back tomorrow").is_none());
    assert!(!is_shown(&harness, "All commands"), "the row opens no launcher");
    assert!(started.lock().unwrap().is_none(), "the row starts no run");
}

/// With the rail beside the card and nothing above it, a new match reads down to why it fits before any scroll.
#[test]
fn a_triage_card_shows_why_it_fits_without_scrolling() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_why_it_fits"));
    press(&mut harness, Key::L);
    press(&mut harness, Key::L);
    assert!(is_shown(&harness, MERIDIAN));
    let why = harness.get_by_label("Why it fits").rect();
    let footer = harness.get_by_label("Draft application…").rect();
    assert!(why.bottom() < footer.top(), "Why it fits at {why:?} sits above the footer at {footer:?}");
}

/// Today's card and the Jobs detail read one job's checks from one place, so they say the same.
#[test]
fn today_and_jobs_word_a_job_the_same() {
    let mut today = tray_harness(shell(Screen::InTray, "checks_today"));
    press(&mut today, Key::L);
    press(&mut today, Key::L);
    assert!(is_shown(&today, MERIDIAN));
    let line = today.get_by_label_contains("84 of 100.").accesskit_node().value().expect("a verdict line");
    // A value can also be a strength, so each is looked for, not counted.
    let values = ["Freelance mission, 12 months", "Brussels, hybrid", "No conflict", "Nothing hidden in the ad"];
    for value in values {
        assert!(today.query_all_by_label(value).count() > 0, "{value} on Today");
    }

    let mut jobs = tray_harness(shell(Screen::Jobs, "checks_jobs"));
    jobs.query_all_by_label(MERIDIAN).next().expect("a Meridian row").click();
    jobs.run_steps(3);
    assert!(is_shown(&jobs, &line), "{line} on Jobs");
    for value in values {
        assert!(jobs.query_all_by_label(value).count() > 0, "{value} on Jobs");
    }
}

/// The top of the dealt card's tab. Only the card's body scrolls, so a decision never moves it.
fn tab_top(harness: &Harness) -> f32 {
    let kinds = ["Claude asks", "Your turn", "Ready to send", "New match", "Interview prep", "Offer", "Check this ad", "Follow up", "Close it?"];
    kinds.iter().find_map(|kind| harness.query_by_label(kind)).expect("a dealt card").rect().top()
}

/// The top of a company's name on the dealt card, near the top of its body.
fn name_top(harness: &Harness, company: &str) -> f32 {
    harness.get_by_label(company).rect().top()
}

/// Ardent Software's offer comes after Meridian. Its body's top, dealt without scrolling anything first.
fn ardent_resting() -> f32 {
    let mut harness = smallest_tray("tray_body_resting", ScriptedGateway(SharedSink::default()));
    for _ in 0..3 {
        press(&mut harness, Key::L);
    }
    name_top(&harness, "Ardent Software")
}

/// Scrolling one card's body leaves the next card's body at its top, and the card itself in place.
#[test]
fn the_next_card_opens_its_body_at_the_top() {
    let mut harness = smallest_tray("tray_body_scroll", ScriptedGateway(SharedSink::default()));
    press(&mut harness, Key::L);
    press(&mut harness, Key::L);
    assert!(is_shown(&harness, MERIDIAN));
    let (tab, resting) = (tab_top(&harness), name_top(&harness, "Meridian Rail Systems"));

    scroll_body(&mut harness, "Meridian Rail Systems");
    assert!(name_top(&harness, "Meridian Rail Systems") < resting, "the body scrolls");
    assert_eq!(tab_top(&harness), tab, "the card stays put");

    press(&mut harness, Key::L);
    assert_eq!(name_top(&harness, "Ardent Software"), ardent_resting(), "the next card starts at its top");
    assert_eq!(tab_top(&harness), tab);

    scroll_body(&mut harness, "Ardent Software");
    press(&mut harness, Key::U);
    assert!(is_shown(&harness, MERIDIAN));
    assert_eq!(name_top(&harness, "Meridian Rail Systems"), resting, "a card back on top starts at its top");
}

/// Turns the mouse wheel over the dealt card's body, where `company` is.
fn scroll_body(harness: &mut Harness, company: &str) {
    harness.hover_at(harness.get_by_label(company).rect().center());
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -200.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(wheel);
    harness.run_steps(3);
}

/// A footer button decides the card, and the next card's tab sits where this one's did.
#[test]
fn a_footer_button_leaves_the_card_in_place() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_footer_click"));
    for _ in 0..4 {
        press(&mut harness, Key::L);
    }
    let top = tab_top(&harness);
    harness.get_by_label("Harmless, keep posting").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Last decision on Halden Logistics: Kept. Press U to undo."));
    assert_eq!(tab_top(&harness), top);
}

fn smallest_tray(name: &str, gateway: ScriptedGateway) -> Harness<'static> {
    let mut shell = shell(Screen::InTray, name);
    *shell.desk_slot() = Some(RunDesk::new(Box::new(gateway), Arc::new(|| {})));
    let mut harness = Harness::builder()
        .with_size([1040.0, 640.0])
        .with_context_setup(install_look)
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    harness
}

/// The footer sits outside the card's scroll, so a long triage card keeps its actions in view.
#[test]
fn the_card_actions_fit_the_smallest_window() {
    let mut harness = smallest_tray("tray_actions_smallest", ScriptedGateway(SharedSink::default()));
    press(&mut harness, Key::L);
    press(&mut harness, Key::L);
    assert!(is_shown(&harness, MERIDIAN));
    assert_in_window(&harness, [1040.0, 640.0], &["Draft application…", "Skip", "Later", "Open job ad", "Show the command"]);
}

/// The line is plain text to read: it opens no launcher and starts no run.
#[test]
fn show_the_command_reveals_the_line_and_starts_nothing() {
    let started = SharedSink::default();
    let mut harness = smallest_tray("tray_show_command", ScriptedGateway(Arc::clone(&started)));
    press(&mut harness, Key::L);
    press(&mut harness, Key::L);
    let line = "/apply https://meridian.example/jobs/1";
    assert!(!is_shown(&harness, line));

    harness.get_by_label("Show the command").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, line));
    assert!(is_shown(&harness, "Hide the command"));
    assert!(is_shown(&harness, MERIDIAN), "the card stays dealt");
    assert!(!is_shown(&harness, "All commands"), "no launcher opens");
    assert!(started.lock().unwrap().is_none(), "no run starts");

    press(&mut harness, Key::L);
    assert!(!is_shown(&harness, line), "the line leaves with its card");
    press(&mut harness, Key::U);
    assert!(is_shown(&harness, MERIDIAN));
    assert!(!is_shown(&harness, line), "a card back on top starts with its line closed");
}

/// The rail sits outside the card's scroll, so the row and its reasons show without scrolling.
#[test]
fn the_undo_row_stays_in_view_in_the_smallest_window() {
    let mut shell = shell(Screen::InTray, "tray_undo_smallest");
    let mut harness = Harness::builder()
        .with_size([1040.0, 640.0])
        .with_context_setup(install_look)
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);

    press(&mut harness, Key::L);
    assert_in_window(&harness, [1040.0, 640.0], &["Undo"]);

    press(&mut harness, Key::L);
    let top = tab_top(&harness);
    press(&mut harness, Key::N);
    assert_in_window(&harness, [1040.0, 640.0], &["Undo", "Rate", "Scope", "Domain", "Location"]);
    assert_eq!(tab_top(&harness), top, "a skip does not scroll");
}

#[test]
fn a_suggested_skip_comes_with_its_reason_and_leaves_the_scroll_alone() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_suggested_skip"));
    for _ in 0..8 {
        press(&mut harness, Key::L);
    }
    let resting = tab_top(&harness);
    press(&mut harness, Key::Y);
    assert!(is_shown(&harness, "Last decision on Federal Data Agency: Skipped for rate. Press U to undo."));
    assert!(is_shown(&harness, "Skipped for rate"), "Y skips with the suggested reason");
    assert_eq!(tab_top(&harness), resting);
}

/// L, L sets aside two cards and N skips Meridian. L five times sets aside five more, then Y skips Federal Data Agency with its suggested reason.
const SKIP_OVER_LOG: [Key; 9] = [Key::L, Key::L, Key::N, Key::L, Key::L, Key::L, Key::L, Key::L, Key::Y];

#[test]
fn a_logged_offer_shows_once_above_the_log() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_offer_over_log"));
    for key in SKIP_OVER_LOG {
        press(&mut harness, key);
    }
    let row = harness.get_by_label("Last decision on Federal Data Agency: Skipped for rate. Press U to undo.").rect();
    assert_eq!(harness.query_all_by_label("Federal Data Agency").count(), 1, "the offer is not drawn again in the log");
    assert_eq!(harness.query_all_by_label("Meridian Rail Systems").count(), 1, "the older skip stays in the log");
    let logged = harness.get_by_label("Meridian Rail Systems").rect();
    assert!(row.bottom() <= logged.top(), "the undo row at {row:?} heads the log row at {logged:?}");
    assert!(is_shown(&harness, "Skipped"));
    assert!(!is_shown(&harness, "Each decision lands here. U undoes the last one."));
}

#[test]
fn overdue_cards_lead_coming_up() {
    let harness = tray_harness(shell_on(Screen::InTray, "tray_overdue", overdue_day()));
    assert!(is_shown(&harness, "Tue 3, Harbor Bank deadline, 4 days late"));
    assert!(is_shown(&harness, "Nothing else dated before Fri 20 Nov."), "late rows leave the next two weeks empty");
}

#[test]
fn an_empty_tray_says_nothing_is_due() {
    let harness = tray_harness(empty_shell("tray_empty"));
    assert!(is_shown(&harness, "No deadlines in the next two weeks."));
}

fn tray_after(mut harness: Harness, keys: &[Key], name: &str) {
    for key in keys {
        press(&mut harness, *key);
    }
    harness.snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
}

fn wgpu_tray(mut shell: Shell, theme: Theme) -> Harness<'static> {
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_pixels_per_point(1.5)
        .with_theme(theme)
        .with_context_setup(install_look)
        .wgpu()
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    harness
}

#[test]
fn intray_triage_light() {
    tray_after(wgpu_tray(shell(Screen::InTray, "intray_triage_light"), Theme::Light), &[Key::L, Key::L], "intray_triage_light");
}

#[test]
fn intray_skip_suggestion_dark() {
    let skips = [Key::L; 8];
    tray_after(wgpu_tray(shell(Screen::InTray, "intray_skip_suggestion_dark"), Theme::Dark), &skips, "intray_skip_suggestion_dark");
}

/// A skip that asks its reason, with nothing logged below it, so the row rounds the slip's foot.
#[test]
fn intray_undo_reasons_light() {
    tray_after(wgpu_tray(shell(Screen::InTray, "intray_undo_reasons_light"), Theme::Light), &[Key::L, Key::L, Key::N], "intray_undo_reasons_light");
}

/// A skip with its reason above a log row, so the row's foot is square. Canvas scene C.
#[test]
fn intray_undo_over_log_light() {
    tray_after(wgpu_tray(shell(Screen::InTray, "intray_undo_over_log_light"), Theme::Light), &SKIP_OVER_LOG, "intray_undo_over_log_light");
}

#[test]
fn intray_guard_dark() {
    tray_after(wgpu_tray(shell(Screen::InTray, "intray_guard_dark"), Theme::Dark), &[Key::L; 4], "intray_guard_dark");
}

#[test]
fn intray_permission_dark() {
    let mut shell = shell(Screen::InTray, "intray_permission_dark");
    *shell.desk_slot() = Some(apply_run(permission_prompt()));
    tray_after(wgpu_tray(shell, Theme::Dark), &[], "intray_permission_dark");
}

#[test]
fn intray_question_light() {
    let mut shell = shell(Screen::InTray, "intray_question_light");
    *shell.desk_slot() = Some(apply_run(question_prompt()));
    tray_after(wgpu_tray(shell, Theme::Light), &[], "intray_question_light");
}

#[test]
fn intray_turn_light() {
    let mut shell = shell(Screen::InTray, "intray_turn_light");
    *shell.desk_slot() = Some(tray_waiting_apply_run());
    tray_after(wgpu_tray(shell, Theme::Light), &[], "intray_turn_light");
}

#[test]
fn intray_set_aside_dark() {
    tray_after(wgpu_tray(shell(Screen::InTray, "intray_set_aside_dark"), Theme::Dark), &[Key::L; TRAY_CARDS], "intray_set_aside_dark");
}

/// A workspace with one command and no applications.
fn empty_shell(name: &str) -> Shell {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".claude/commands")).unwrap();
    fs::write(root.join(".claude/commands/apply.md"), "").unwrap();
    let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    Shell::with_snapshot(Screen::InTray, jobdesk::workspace::open(&root).unwrap(), snapshot)
}

#[test]
fn intray_clear_dark() {
    tray_after(wgpu_tray(empty_shell("intray_clear_dark"), Theme::Dark), &[], "intray_clear_dark");
}

/// The sample workspace with a tracker saved in Latin-1, as a spreadsheet's plain CSV export does.
/// With `jobs_too`, the saved jobs file is cut off as well.
fn failed_shell(name: &str, jobs_too: bool) -> Shell {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    fs::write(root.join("job_search_tracker.csv"), b"Company,Role,Status\nCaf\xe9 Nordlicht,Platform Lead,Applied\n").unwrap();
    if jobs_too {
        fs::write(root.join("job_scraper/seen_jobs.json"), "{\"seen\": {").unwrap();
    }
    let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    assert_eq!(snapshot.issues.len(), 1 + usize::from(jobs_too), "{:?}", snapshot.issues);
    Shell::with_snapshot(Screen::InTray, jobdesk::workspace::open(&root).unwrap(), snapshot)
}

fn failed_tracker_shell(name: &str) -> Shell {
    failed_shell(name, false)
}

#[test]
fn a_failed_source_marks_its_tab_and_today_deals_the_rest() {
    let mut harness = tray_harness(failed_tracker_shell("failed_tracker"));
    assert!(is_shown(&harness, "Applications, could not be read"));
    assert!(is_shown(&harness, "Your applications could not be read."));
    assert!(is_shown(&harness, "job_search_tracker.csv: not UTF-8 text, re-save it as CSV UTF-8"));
    assert!(is_shown(&harness, "Today shows new matches and tasks until it is fixed. Jobs, Insights and Tasks still work."));
    assert!(harness.query_by_label_contains("Applications are missing from this list.").is_some());
    assert!(is_shown(&harness, "Application dates are missing until the file is fixed."));
    assert!(is_shown(&harness, MERIDIAN), "the job cards still deal");
    assert!(harness.query_by_label_contains("source could not be read").is_none(), "the count left the status bar");

    harness.get_by_label("Open Setup").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "You will see which account Claude uses after the first task. Jobdesk stores no credentials."), "Setup is on show");
    assert!(is_shown(&harness, "Your applications could not be read."), "the alert stays over every screen");
    assert!(!is_shown(&harness, "Open Setup"), "Setup has no button to itself");
}

/// A decisions file that cannot be read joins the same alert, on every screen, and leaves Today's count alone.
#[test]
fn a_decisions_file_that_fails_joins_the_alert() {
    let mut shell = shell(Screen::InTray, "failed_decisions");
    let data = Path::new(env!("CARGO_TARGET_TMPDIR")).join("failed_decisions").join("data");
    let _ = fs::remove_dir_all(&data);
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("decisions.json"), "{\"cut off").unwrap();
    let workspace = data.join("ai-job-search");
    *shell.tray_slot() = Some(jobdesk::intray::open_tray(data, &workspace));
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, "Your decisions could not be saved or read."));
    assert!(harness.query_by_label_contains("decisions.json: ").is_some());
    assert!(is_shown(&harness, "Today still works, but what you decide may be gone after Jobdesk closes."));
    assert!(harness.query_by_label_contains("could not be read").is_none(), "no tab is marked");

    harness.get_by_label("Jobs").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Your decisions could not be saved or read."), "the alert shows on every screen");
}

/// Each sentence on what still works assumes one failure, so two failures show neither, and one button.
#[test]
fn two_failed_sources_share_one_alert_and_promise_nothing() {
    let harness = tray_harness(failed_shell("failed_two", true));
    assert!(is_shown(&harness, "Jobs, could not be read"));
    assert!(is_shown(&harness, "Applications, could not be read"));
    assert!(is_shown(&harness, "Your applications could not be read."));
    assert!(is_shown(&harness, "Your saved jobs could not be read."));
    assert!(harness.query_by_label_contains("still work").is_none());
    assert_eq!(harness.query_all_by_label("Open Setup").count(), 1);
    assert!(harness.query_by_label_contains("Jobs and Applications are missing from this list.").is_some(), "in the alert's order");
    assert!(is_shown(&harness, "Job and application dates are missing until the files are fixed."));
}

#[test]
fn intray_failed_light() {
    tray_after(wgpu_tray(failed_tracker_shell("intray_failed_light"), Theme::Light), &[], "intray_failed_light");
}

/// Every sample deadline, Nov 3 to 6, has passed by then.
fn overdue_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()
}

#[test]
fn intray_overdue_light() {
    tray_after(wgpu_tray(shell_on(Screen::InTray, "intray_overdue_light", overdue_day()), Theme::Light), &[Key::L], "intray_overdue_light");
}

#[test]
fn intray_overdue_dark() {
    tray_after(wgpu_tray(shell_on(Screen::InTray, "intray_overdue_dark", overdue_day()), Theme::Dark), &[Key::L], "intray_overdue_dark");
}

const FINDINGS: &str = "## Verification Checklist\n- [x] All claims match the profile\n- [x] CV is exactly 2 pages\n- [ ] Addressed to a named person\n\n| Keyword | Priority | Status | Note |\n|---|---|---|---|\n| event sourcing | required | covered | Skills |\n| Azure | preferred | missing (gap) | Named in letter |\n";
const EVALUATION: &str = "**Overall fit: 78, Strong Fit.**\n\n**Skills match:** Java, Kafka and event sourcing match.\n\n**Experience match:** Public sector work maps well.";

fn dossier_shell(name: &str) -> Shell {
    let mut shell = shell(Screen::Tracker, name);
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    desk.start("/apply https://tessel.example/1", &StartOptions { model: None, budget_usd: 5.0 }, run_started()).unwrap();
    let guard = sink.lock().unwrap();
    let emit = guard.as_ref().unwrap();
    for text in [EVALUATION, FINDINGS] {
        emit(AgentEvent::AssistantText { text: text.into(), nested: false });
    }
    emit(AgentEvent::ProcessExited { failure: None });
    drop(guard);
    *shell.desk_slot() = Some(desk);
    shell.open_dossier("tessel_group_solution_architect", jobdesk::dossier::page_store(Arc::new(|| {})));
    shell
}

fn render_dossier(section: Option<&str>, theme: Theme, name: &str) {
    let mut shell = dossier_shell(name);
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_pixels_per_point(1.5)
        .with_theme(theme)
        .with_context_setup(install_look)
        .wgpu()
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    if let Some(section) = section {
        harness.get_by_label_contains(section).click();
    }
    for _ in 0..40 {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    harness.snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
}

struct FixedTools;

impl ToolProber for FixedTools {
    fn probe(&self, tool: &ToolSpec) -> Option<String> {
        (tool.name != "pdftotext" && tool.name != "bun").then(|| "1.0".to_owned())
    }
}

fn render_workspace(theme: Theme, name: &str) {
    render_probed(shell(Screen::Workspace, name), theme, name);
}

/// Renders once the tool check is back, so the status bar says whether Claude is installed. It starts as the app's does, at launch.
fn render_probed(mut shell: Shell, theme: Theme, name: &str) {
    let mut checks = EnvironmentChecks::new(Arc::new(FixedTools), Arc::new(|| {}));
    checks.ensure_started();
    *shell.environment_slot() = Some(checks);
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_pixels_per_point(1.5)
        .with_theme(theme)
        .with_context_setup(install_look)
        .wgpu()
        .build_ui(move |ui| shell.show(ui));
    for _ in 0..20 {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    harness.snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
}

/// The sample folder, open but not read yet.
fn loading_shell(name: &str) -> Shell {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    Shell::loading(Screen::InTray, jobdesk::workspace::open(&root).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 31).unwrap())
}

#[test]
fn loading_deals_no_card_and_ignores_keys() {
    let mut shell = loading_shell("tray_loading");
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(SharedSink::default())), Arc::new(|| {})));
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, "Reading your job search…"));
    assert!(is_shown(&harness, "Your first card appears in a moment."));
    assert!(is_shown(&harness, "Saturday 31 October"), "the stamp is empty and reads as the date");

    press(&mut harness, Key::Y);
    press(&mut harness, Key::L);

    assert!(!is_shown(&harness, "All commands"), "Y opens no launcher");
    assert!(!is_shown(&harness, "Set aside until tomorrow"), "L sets nothing aside");
    assert!(is_shown(&harness, "Reading your job search…"));
}

#[test]
fn intray_loading_light() {
    render_probed(loading_shell("intray_loading_light"), Theme::Light, "intray_loading_light");
}

#[test]
fn intray_loading_dark() {
    render_probed(loading_shell("intray_loading_dark"), Theme::Dark, "intray_loading_dark");
}

#[test]
fn first_run_light() {
    render_probed(Shell::without_workspace(NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()), Theme::Light, "first_run_light");
}

#[test]
fn workspace_dark() {
    render_workspace(Theme::Dark, "workspace_dark");
}

#[test]
fn workspace_light() {
    render_workspace(Theme::Light, "workspace_light");
}

#[test]
fn dossier_documents_dark() {
    render_dossier(None, Theme::Dark, "dossier_documents_dark");
}

#[test]
fn dossier_documents_light() {
    render_dossier(None, Theme::Light, "dossier_documents_light");
}

#[test]
fn dossier_evaluation_dark() {
    render_dossier(Some("Evaluation"), Theme::Dark, "dossier_evaluation_dark");
}

#[test]
fn dossier_prep_light() {
    render_dossier(Some("Interview prep"), Theme::Light, "dossier_prep_light");
}

#[test]
fn dossier_outcome_dark() {
    render_dossier(Some("Results and follow-ups"), Theme::Dark, "dossier_outcome_dark");
}

#[test]
fn run_permission_dark() {
    render_run(permission_prompt(), Theme::Dark, "run_permission_dark");
}

#[test]
fn run_question_light() {
    render_run(question_prompt(), Theme::Light, "run_question_light");
}

#[test]
fn run_stop_point_dark() {
    render_run(stop_point(), Theme::Dark, "run_stop_point_dark");
}

#[test]
fn run_turn_light() {
    render_desk(apply_stop_after_evaluate(), Theme::Light, "run_turn_light");
}

#[test]
fn run_turn_dark() {
    render_desk(apply_stop_after_evaluate(), Theme::Dark, "run_turn_dark");
}

#[test]
fn run_working_light() {
    render_desk(apply_drafting().0, Theme::Light, "run_working_light");
}

#[test]
fn run_finished_dark() {
    render_desk(apply_finished(), Theme::Dark, "run_finished_dark");
}

/// The user's own window size: Run details stands in for the side column, and the band keeps to one line.
#[test]
fn run_turn_1040_light() {
    let mut shell = shell(Screen::Runs, "run_turn_1040_light");
    *shell.desk_slot() = Some(apply_stop_after_evaluate());
    let mut harness = Harness::builder()
        .with_size([1040.0, 650.0])
        .with_pixels_per_point(1.5)
        .with_theme(Theme::Light)
        .with_context_setup(install_look)
        .wgpu()
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    harness.snapshot_options("run_turn_1040_light", &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
}

#[test]
fn launcher_light() {
    render_launcher(Theme::Light, "launcher_light");
}

#[test]
fn launcher_dark() {
    render_launcher(Theme::Dark, "launcher_dark");
}

screens! {
    intray_dark: Screen::InTray, Theme::Dark;
    intray_light: Screen::InTray, Theme::Light;
    jobs_dark: Screen::Jobs, Theme::Dark;
    jobs_light: Screen::Jobs, Theme::Light;
    tracker_dark: Screen::Tracker, Theme::Dark;
    tracker_light: Screen::Tracker, Theme::Light;
    insights_dark: Screen::Insights, Theme::Dark;
    insights_light: Screen::Insights, Theme::Light;
}
