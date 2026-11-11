//! Renders every screen in both themes from a sample workspace.

use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use egui::{CursorIcon, Key, Theme};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder, SnapshotOptions};
use jobdesk::folder_update::application::{Failure as FolderFailure, Outcome as FolderOutcome, Phase, Report, Reverted, SideCopy, Stage, UndoOutcome};
use jobdesk::folder_update::idle_folder_update;
use jobdesk::runs::application::{
    AgentEvent, AgentGateway, AgentSession, CATALOG, CommandSpec, Decision, EventSink, ModelCost, Prompt, PromptKind, Question, QuestionOption, RunDesk,
    RunRequest, StartOptions, TaskCost, TaskOutcome, Tokens, Usage, UsageReport,
};
use jobdesk::settings::application::{
    ApiKey, ConnectionTester, DataSizes, Dollars, Drawn, KeyStore, KeyStoreError, KeyTest, Outcome, Page, PermissionMode, Providers, Settings, SignIn,
    TestDone, TestReport, drawn_index, index,
};
use jobdesk::settings::{idle_tester, memory_keychain, open_providers};
use jobdesk::shared_kernel::ModelChoice;
use jobdesk::shell::{Screen, Shell, Snapshot, Source, install_look};
use jobdesk::ui_kit::tokens::{BODY_PADDING, HAIRLINE, HEADER_PADDING, HEAT_LABELS, SCREEN_PADDING, STATUS_DOT, STATUS_PADDING, TALLY_GAP_Y};
use jobdesk::updates::application::{ApplyFailed, ApplyFailure, DownloadEnd, DownloadId, Failure, Platform, Release, ServiceEvent, Version};
use jobdesk::updates::scripted_updates;
use jobdesk::workspace::application::{EnvironmentChecks, ToolProber, ToolSpec};

/// Snapshot baselines come from one renderer, so only macOS draws and compares them. Every other assertion runs everywhere.
const SNAPSHOTS: bool = cfg!(target_os = "macos");

/// Gives the harness the renderer that draws snapshots, where they are compared.
fn with_renderer<State>(builder: HarnessBuilder<State>) -> HarnessBuilder<State> {
    if SNAPSHOTS { builder.wgpu() } else { builder }
}

/// Compares the harness to the baseline `name`, where snapshots are compared.
fn compare(harness: &mut Harness, name: &str) {
    if SNAPSHOTS {
        harness.snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
    }
}

#[cfg(target_os = "macos")]
#[test]
#[allow(clippy::assertions_on_constants)]
fn snapshots_compare_on_macos() {
    assert!(SNAPSHOTS);
}

fn sample_workspace(root: &Path) {
    let source = Path::new("tests/sample_workspace");
    copy_tree(source, root);
    // A real `CLAUDE.md` in this repository would steer the agents that work on it.
    fs::rename(root.join("CLAUDE.profile.md"), root.join("CLAUDE.md")).unwrap();
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
    let snapshot = Snapshot { usage_read: true, ..Snapshot::load(&root, today) };
    assert!(snapshot.issues.is_empty(), "{:?}", snapshot.issues);
    Shell::with_snapshot(screen, jobdesk::workspace::open(&root).unwrap(), snapshot)
}

type SharedSink = Arc<Mutex<Option<EventSink>>>;

/// The moment every scripted clock counts from.
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// Stops the desk's clock `seconds` after the epoch. Elapsed, the last update and the sweep read it,
/// so a snapshot holds still however long the test takes.
fn clock_at(desk: &mut RunDesk, seconds: f64) {
    let now = epoch() + Duration::from_secs_f64(seconds);
    desk.set_clock(Arc::new(move || now));
}

/// A desk on the scripted clock, standing at the epoch until the test moves it.
fn desk_on(gateway: impl AgentGateway + 'static) -> RunDesk {
    let mut desk = RunDesk::new(Box::new(gateway), Arc::new(|| {}));
    clock_at(&mut desk, 0.0);
    desk
}

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

/// What a recording gateway saw: each process it started, with the model and budget it asked for, and each message sent to a live one.
#[derive(Default)]
struct Calls {
    starts: usize,
    asked: Vec<(Option<String>, f64)>,
    sent: Vec<String>,
    /// Each request a process started from, whose line and figures a run log's meta line holds.
    requests: Vec<RunRequest>,
}

type SharedCalls = Arc<Mutex<Calls>>;

/// A scripted gateway that also records its calls.
struct RecordingGateway(SharedSink, SharedCalls);
struct RecordingSession(SharedCalls);

impl AgentGateway for RecordingGateway {
    fn start(&self, request: &RunRequest, sink: EventSink) -> Box<dyn AgentSession> {
        let mut calls = self.1.lock().unwrap();
        calls.starts += 1;
        calls.asked.push((request.model.clone(), request.budget_usd));
        calls.requests.push(request.clone());
        drop(calls);
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
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    // Permission prompts, as Ask me before each tool shows them.
    desk.set_asks_permission(true);
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
        used("msg_1", 30_000),
        tool("4", "Write", "cv/main_kestrel_solution_architect.tex", false),
        done("4"),
        tool("5", "Agent", "application-reviewer: Research and critique", false),
        done("5"),
        tool("6", "Edit", "cv/main_kestrel_solution_architect.tex", false),
        done("6"),
        tool("8", "Bash", "ls -Z \"$SCRATCHPAD\"", false),
        failed("8", "ls: invalid option -- 'Z'\nTry 'ls --help' for more information."),
        tool("7", "Bash", "cd cv && lualatex -interaction=nonstopmode main_kestrel_solution_architect.tex", false),
        used("msg_2", 18_000),
    ];
    let guard = sink.lock().unwrap();
    let emit = guard.as_ref().unwrap();
    events.into_iter().chain(final_events).for_each(emit);
    drop(guard);
    // The last word comes at 8:05. A prompt keeps the clock running while the user reads it, and an end holds it.
    clock_at(&mut desk, 485.0);
    desk.poll();
    clock_at(&mut desk, 571.0);
    desk
}

/// What one of Claude's messages used, priced as Opus while the turn is under way.
fn used(id: &str, output: u64) -> AgentEvent {
    AgentEvent::MessageUsage { id: id.into(), model: "claude-opus-5-5".into(), tokens: Tokens { input: 3_000, output, cache_write: 8_000, cache_read: 90_000 } }
}

/// An `/apply` run that evaluated the posting, asked whether to draft, and ended its turn.
fn tray_waiting_apply_run() -> RunDesk {
    let sink = SharedSink::default();
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    desk.start("/apply https://kestrel.example/jobs/2", &StartOptions { model: None, budget_usd: 5.0 }, run_started()).unwrap();
    let events = [
        AgentEvent::SessionStarted { session_id: "a5ce0bcc-22f9".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() },
        tool("1", "Read", ".claude/skills/job-application-assistant/04-job-evaluation.md", false),
        done("1"),
        AgentEvent::AssistantText {
            text: "**Fit 78, Strong Fit.** Event sourcing and Kafka match. Card schemes are a gap.\n\nShall I proceed with drafting?".into(),
            nested: false,
        },
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

/// The launcher's subtitle, which only the launcher shows. Its title, "New task", is also the menu button's label.
const LAUNCHER: &str = "Pick a command. Nothing runs until you press Run.";
const APPLY_LINE: &str = "/apply https://kestrel.example/jobs/2";
const STOP_EVALUATION: &str = "# Job fit evaluation: Solution Architect at Kestrel Payments\n\nVerdict: Strong Fit, 78 out of 100. Every hard gate passes, and the two gaps are small.\n\n| Gate | In the posting | Result | Notes |\n|---|---|---|---|\n| Contract | Permanent, full time | Pass | Matches the permanent role you asked for in the profile. |\n| Work mode | Hybrid, 2 days a week in Leuven | Pass | Leuven is inside the commute you set, and two office days are fine. |\n| Language | English, Dutch is a plus | Pass | Your Dutch is conversational, which covers the plus. |\n| Salary | Not stated | Check | Ask for the range on the first call with the hiring manager. |\n\nShall I go ahead and draft the CV and cover letter?";

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
    let mut desk = desk_on(RecordingGateway(Arc::clone(&sink), calls));
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
    // Claude stops at 2:30, and the user has read for a minute since: the clock runs on through their turn.
    clock_at(&mut desk, 150.0);
    desk.poll();
    clock_at(&mut desk, 212.0);
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
    emit_all(
        &sink,
        [
            tool("6", "Read", ".claude/skills/job-application-assistant/05-cv-tailoring.md", false),
            done("6"),
            tool("7", "Write", "cv/main_kestrel_solution_architect.tex", false),
            done("7"),
            tool("8", "Write", "cover_letters/cover_kestrel_solution_architect.tex", false),
            used("msg_3", 500),
        ],
    );
    // The last update came 3 s ago, and the sweep is 1.1 s into a pass.
    clock_at(&mut desk, 250.1);
    desk.poll();
    clock_at(&mut desk, 253.1);
    (desk, sink)
}

/// The same draft, with no update for 3 min.
fn apply_drafting_quiet() -> RunDesk {
    let (mut desk, _) = apply_drafting();
    clock_at(&mut desk, 430.1);
    desk
}

const SUMMARY: &str = "# Application ready: Kestrel Payments, Solution Architect\n\nThe CV (2 pages) and the cover letter (1 page) are built, and both checks passed.\n\n## What changed after review\n\n- 6 reviewer edits applied. 1 was skipped because your profile does not back it.\n- The letter now opens with the payments migration.\n\n## Next\n\n- Read both PDFs in the dossier.\n- Send it yourself. Jobdesk never sends applications.";

/// /apply through Finish, its closing summary, and the process gone.
fn apply_finished() -> RunDesk {
    let (mut desk, sink) = apply_drafting();
    emit_all(
        &sink,
        [
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
        ],
    );
    // It ends at 14:20, and the figure holds after.
    clock_at(&mut desk, 860.0);
    desk.poll();
    clock_at(&mut desk, 900.0);
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
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(SharedSink::default())));
    render_with(shell, theme, name);
}

fn render_with(mut shell: Shell, theme: Theme, name: &str) {
    let mut harness =
        with_renderer(Harness::builder().with_size([1280.0, 820.0]).with_pixels_per_point(1.5).with_theme(theme).with_context_setup(install_look))
            .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    compare(&mut harness, name);
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
    *shell.desk_slot() = Some(desk_on(gateway));
    let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
    harness.run_steps(2);

    harness.get_by_label("Tune scoring…").click();
    harness.run_steps(3);
    harness.get_by_label(LAUNCHER);

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
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    // Permission prompts, as Ask me before each tool shows them.
    desk.set_asks_permission(true);
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
    let mut harness = Harness::builder().with_size([1040.0, 640.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
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
    emit_all(
        &sink,
        [
            AgentEvent::AssistantText { text: "Before I draft: shall I lead with the Kafka migration?".into(), nested: false },
            AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 1.3, duration_ms: 20_000 },
        ],
    );
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

/// A disabled control keeps the arrow, so the way to learn why it waits is to rest the pointer on it.
#[test]
fn a_disabled_send_says_why() {
    let mut shell = shell(Screen::Runs, "disabled_send");
    *shell.desk_slot() = Some(stop_after_evaluate(SharedCalls::default()));
    let mut harness = tray_harness(shell);
    let send = harness.get_by_label("Send");
    assert!(send.accesskit_node().is_disabled(), "nothing is typed yet");
    assert!(!is_shown(&harness, "Type a reply first."));
    send.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "Type a reply first."), "the pointer on a disabled Send says why");
}

/// A question's Send answer waits for a pick in every question.
#[test]
fn a_disabled_send_answer_says_why() {
    let mut shell = shell(Screen::Runs, "disabled_send_answer");
    *shell.desk_slot() = Some(apply_run(question_prompt()));
    let mut harness = tray_harness(shell);
    let send = harness.get_by_label("Send answer");
    assert!(send.accesskit_node().is_disabled(), "no answer is picked yet");
    send.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "Pick an answer for each question."));
}

/// Until a folder is open, the tabs and New task wait and say what for.
#[test]
fn the_tabs_say_what_they_wait_for_before_a_folder_is_open() {
    let mut harness = tray_harness(Shell::without_workspace(NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()));
    let new_task = harness.get_by_label("New task");
    assert!(new_task.accesskit_node().is_disabled());
    new_task.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "Choose your job search folder first."));
}

/// Answers a probe once `release` is set, so a check stays running as long as a test wants.
struct GatedTools(Arc<std::sync::atomic::AtomicBool>);

impl ToolProber for GatedTools {
    fn probe(&self, _: &ToolSpec) -> Option<String> {
        while !self.0.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Some("1.0".to_owned())
    }
}

#[test]
fn check_again_says_checking_while_it_runs() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let release = Arc::new(AtomicBool::new(false));
    let mut shell = shell(Screen::Workspace, "check_again_running");
    let mut checks = EnvironmentChecks::new(Arc::new(GatedTools(Arc::clone(&release))), Arc::new(|| {}));
    checks.ensure_started();
    *shell.environment_slot() = Some(checks);
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, "Checking…"), "the button names what runs");
    assert!(!is_shown(&harness, "Check again"));
    assert!(harness.get_by_label("Checking…").accesskit_node().is_disabled());

    release.store(true, Ordering::SeqCst);
    for _ in 0..40 {
        harness.run_steps(1);
        if is_shown(&harness, "Check again") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(is_shown(&harness, "Check again"), "the label comes back with the results");
    assert!(!harness.get_by_label("Check again").accesskit_node().is_disabled());
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
    emit_all(
        &sink,
        [
            AgentEvent::AssistantText { text: "Understood, stopping. The evaluation stays in the tracker.".into(), nested: false },
            AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 1.2, duration_ms: 4_000 },
        ],
    );
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
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    desk.start(APPLY_LINE, &StartOptions { model: None, budget_usd: 3.0 }, run_started()).unwrap();
    emit_all(
        &sink,
        [
            AgentEvent::SessionStarted { session_id: "3f9a21c0-5d7e".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() },
            permission_prompt(),
            tool("1", "Read", ".claude/skills/job-application-assistant/04-job-evaluation.md", false),
            done("1"),
        ],
    );
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
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
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
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
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

/// A step's whole line is its toggle's hit area: a click on its name opens its actions, and a second one closes them.
#[test]
fn a_click_on_a_step_name_opens_its_actions() {
    let mut shell = shell(Screen::Runs, "step_name_click");
    *shell.desk_slot() = Some(apply_run(stop_point()));
    let mut harness = tray_harness(shell);
    let file = "main_kestrel_solution_architect.tex";
    let named = |harness: &Harness| harness.query_all_by_label_contains(file).count();
    assert_eq!(named(&harness), 0, "nowhere while every toggle is closed");

    harness.get_by_label("Draft").click();
    harness.run_steps(3);
    assert_eq!(named(&harness), 1, "Draft's Write row");
    assert!(is_shown(&harness, "Hide actions"));

    harness.get_by_label("Draft").click();
    harness.run_steps(3);
    assert_eq!(named(&harness), 0);
}

/// At a stop, the story opens on the top of Claude's newest message, not on its last line.
/// Both stories are taller than their room, so they have to scroll: the stop in the smallest window, and a long stop in the default one.
#[test]
fn a_stop_opens_on_the_newest_message() {
    let long = format!("{STOP_EVALUATION}\n\n{}", "The gaps are small and either can be closed in the letter.\n\n".repeat(24));
    let harnesses = [
        smallest_window(apply_stop_after_evaluate(), "stop_opens_on_message"),
        tray_harness_with(stop_with(SharedCalls::default(), &long).0, "long_stop_opens_on_message"),
    ];
    for harness in harnesses {
        let story = harness.get_by_label("The run so far").rect();
        // Where the story's rows start: under its heading's padding, its rule and the body's padding.
        let content_top = story.bottom() + f32::from(HEADER_PADDING) + HAIRLINE + f32::from(BODY_PADDING);
        let heading = harness.get_by_label_contains("Job fit evaluation").rect();
        assert!(heading.top() > content_top, "{heading:?} is under the story's rows at {content_top}");
        assert!(heading.top() < content_top + 40.0, "{heading:?} opens at the top of the story's rows at {content_top}");
    }
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

/// In the smallest window, the footer under the story ends inside the story's column, above the screen's bottom padding.
/// A run that waits on a question with word of a background agent, and a finished run with its reply field.
#[test]
fn the_story_footer_stays_inside_its_column() {
    const BACKGROUND: &str = "1 background agent is still working. Claude continues when it reports back.";
    let (long_stop, sink) = stop_with(SharedCalls::default(), LONG_QUESTION);
    emit_all(&sink, [AgentEvent::BackgroundTasksChanged { count: 1 }]);
    let cases = [
        (apply_run_ending([many_choices("q"), AgentEvent::BackgroundTasksChanged { count: 1 }]), BACKGROUND),
        (long_stop, BACKGROUND),
        (apply_finished(), "Send"),
    ];
    for (desk, footer) in cases {
        let harness = smallest_window(desk, "footer_in_column");
        let column_end = harness.get_by_label(STATUS_LINE).rect().top() - STATUS_PADDING.1 - SCREEN_PADDING.2;
        let rect = harness.get_by_label(footer).rect();
        assert!(rect.bottom() <= column_end + 0.5, "{footer} ends at {} under the column's end at {column_end}", rect.bottom());
    }
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

/// Only whole text is copied: a question cut to fit takes no selection and shows in full on hover. A whole one can be copied.
#[test]
fn only_a_whole_question_can_be_copied() {
    let asks = |question: &str| format!("Claude asks: {}", question.rsplit("\n\n").next().unwrap());
    let (desk, _sink) = stop_with(SharedCalls::default(), LONG_QUESTION);
    let mut harness = smallest_window(desk, "cut_question_copy");
    let cut = asks(LONG_QUESTION);
    harness.get_by_label(&cut).hover();
    harness.run_steps(4);
    assert_ne!(harness.output().platform_output.cursor_icon, CursorIcon::Text, "a cut question takes no selection");
    assert!(harness.query_all_by_label_contains("or would you rather stop here").count() > 1, "the tooltip shows the whole question");

    let mut harness = smallest_window(apply_stop_after_evaluate(), "whole_question_copy");
    harness.get_by_label(&asks(STOP_EVALUATION)).hover();
    harness.run_steps(2);
    assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::Text, "a whole question can be selected");
}

/// The Settings button, which sits in the status line.
const STATUS_LINE: &str = "Settings";

/// A reply takes a stop's story back to following the run: its newest row comes into view.
#[test]
fn a_resumed_run_is_followed_again() {
    let (desk, sink) = stop_with(SharedCalls::default(), STOP_EVALUATION);
    let mut harness = smallest_window(desk, "resumed_follow");
    harness.get_by_label("Go ahead with the draft").click();
    harness.run_steps(3);
    emit_all(
        &sink,
        [
            tool("6", "Read", ".claude/skills/job-application-assistant/05-cv-tailoring.md", false),
            done("6"),
            tool("7", "Write", "cv/main_kestrel_solution_architect.tex", false),
        ],
    );
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
    emit_all(
        &sink,
        [
            AgentEvent::SessionStarted { session_id: "3f9a21c0-5d7e".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() },
            tool("6", "Read", "notes/kestrel.md", false),
            done("6"),
            tool("7", "Bash", "ls cv", false),
        ],
    );
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

/// The window's close button asks first while a session is open. Keep working takes the focus, and Esc keeps working.
#[test]
fn esc_in_the_quit_dialog_keeps_working() {
    let mut shell = shell(Screen::Runs, "quit_esc");
    *shell.desk_slot() = Some(apply_stop_after_evaluate());
    let mut harness = tray_harness(shell);
    harness.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
    harness.run_steps(3);
    assert!(harness.get_by_label("Keep working").is_focused());

    press(&mut harness, Key::Escape);

    assert!(!is_shown(&harness, "Keep working"));
    assert!(harness.query_all_by_label("Your turn").count() > 0, "the session is still open");
}

/// The command key as the platform sends it: Cmd on macOS and Ctrl elsewhere, each with `command` set.
const COMMAND: egui::Modifiers =
    egui::Modifiers { alt: false, ctrl: !cfg!(target_os = "macos"), shift: false, mac_cmd: cfg!(target_os = "macos"), command: true };

fn cmd_enter(harness: &mut Harness) {
    harness.key_press_modifiers(COMMAND, Key::Enter);
    harness.run_steps(3);
}

fn recording_shell(screen: Screen, name: &str, calls: &SharedCalls) -> Shell {
    let mut shell = shell(screen, name);
    *shell.desk_slot() = Some(desk_on(RecordingGateway(SharedSink::default(), Arc::clone(calls))));
    shell
}

/// On /apply the posting is the multiline field, and the budget the only single line.
fn posting<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
    harness.get_by_role(egui::accesskit::Role::MultilineTextInput)
}

fn budget<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
    harness.get_by_role(egui::accesskit::Role::TextInput)
}

/// Cmd or Ctrl Enter starts the run only while Run could, and a multiline field never takes it as a new line.
#[test]
fn cmd_enter_runs_only_when_run_is_enabled() {
    let calls = SharedCalls::default();
    let mut harness = tray_harness(recording_shell(Screen::Launcher, "launcher_cmd_enter", &calls));
    harness.get_by_label_contains("/apply").click();
    harness.run_steps(3);
    // An empty budget leaves Run disabled.
    budget(&harness).focus();
    harness.run_steps(2);
    harness.key_press_modifiers(COMMAND, Key::A);
    press(&mut harness, Key::Backspace);
    posting(&harness).focus();
    posting(&harness).type_text("Kestrel Payments");
    harness.run_steps(2);
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 0, "no budget, so Run is disabled");
    assert_eq!(posting(&harness).value().as_deref(), Some("Kestrel Payments"), "and the posting took no new line");

    budget(&harness).focus();
    budget(&harness).type_text("2");
    harness.run_steps(2);
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 1, "Run was enabled, so the keys started the run");
    harness.get_by_label("Recent runs");

    let calls = SharedCalls::default();
    let mut harness = tray_harness(recording_shell(Screen::Launcher, "launcher_cmd_enter_reset", &calls));
    harness.get_by_label_contains("/reset").scroll_to_me();
    harness.run_steps(3);
    harness.get_by_label_contains("/reset").click();
    harness.run_steps(3);
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 0, "I understand is unticked");
    harness.get_by_label(LAUNCHER);

    harness.get_by_label("I understand").scroll_to_me();
    harness.run_steps(3);
    harness.get_by_label("I understand").click();
    harness.run_steps(3);
    cmd_enter(&mut harness);
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 1, "exactly one run");
}

#[test]
fn cmd_enter_starts_nothing_outside_the_launcher() {
    let calls = SharedCalls::default();
    for screen in [Screen::InTray, Screen::Jobs, Screen::Tracker, Screen::Insights, Screen::Runs, Screen::Portals, Screen::Workspace] {
        let mut harness = tray_harness(recording_shell(screen, "cmd_enter_elsewhere", &calls));
        cmd_enter(&mut harness);
        assert!(!is_shown(&harness, LAUNCHER), "{screen:?} stays");
    }
    assert_eq!(calls.lock().unwrap().starts, 0);
}

/// Esc goes back to the screen that opened the launcher, but an open list takes it first.
#[test]
fn esc_leaves_the_launcher_for_where_it_came_from() {
    let mut harness = tray_harness(recording_shell(Screen::Insights, "launcher_esc", &SharedCalls::default()));
    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);
    harness.get_by_label(LAUNCHER);

    harness.get_by_role(egui::accesskit::Role::ComboBox).click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Haiku"), "the model list is open");
    press(&mut harness, Key::Escape);
    assert!(!is_shown(&harness, "Haiku"), "Esc closed the list");
    assert!(is_shown(&harness, LAUNCHER), "and left the launcher open");

    press(&mut harness, Key::Escape);
    assert!(!is_shown(&harness, LAUNCHER));
    harness.get_by_label("Tune scoring…");
}

/// In a text field Esc only leaves the field, and keeps what was typed. A second Esc leaves the launcher.
#[test]
fn esc_in_a_field_leaves_the_field_first() {
    let mut harness = tray_harness(recording_shell(Screen::Insights, "launcher_esc_field", &SharedCalls::default()));
    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);
    harness.get_by_label_contains("/apply").click();
    harness.run_steps(3);
    posting(&harness).focus();
    posting(&harness).type_text("Kestrel Payments");
    harness.run_steps(2);

    press(&mut harness, Key::Escape);
    assert!(is_shown(&harness, LAUNCHER), "the launcher stays");
    assert!(!posting(&harness).is_focused(), "the field let go");
    assert_eq!(posting(&harness).value().as_deref(), Some("Kestrel Payments"));

    press(&mut harness, Key::Escape);
    assert!(!is_shown(&harness, LAUNCHER));
    harness.get_by_label("Tune scoring…");
}

const GRANITE_CARD: &str = "Granite Cloud, Platform Lead, No reply in 15 days, waits for you in Today";

#[test]
fn a_second_click_closes_the_application() {
    let mut harness = tray_harness(shell(Screen::Tracker, "application_close"));
    let open = |harness: &Harness| harness.query_all_by_label("Granite Cloud").count() == 2;
    harness.get_by_label(GRANITE_CARD).click();
    harness.run_steps(3);
    assert!(open(&harness), "the first click opens the details");
    harness.get_by_label(GRANITE_CARD).click();
    harness.run_steps(3);
    assert!(!open(&harness), "the second closes them");

    harness.get_by_label(GRANITE_CARD).click();
    harness.run_steps(3);
    press(&mut harness, Key::Escape);
    assert!(!open(&harness), "Esc closes them too");
}

/// The snapshot a command leaves when it drops `company` from the sample tracker. The file is put back after.
fn tracker_without(name: &str, company: &str) -> Snapshot {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let tracker = root.join("job_search_tracker.csv");
    let full = fs::read_to_string(&tracker).unwrap();
    let kept: Vec<&str> = full.lines().filter(|row| !row.contains(company)).collect();
    fs::write(&tracker, kept.join("\n") + "\n").unwrap();
    let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    fs::write(&tracker, full).unwrap();
    snapshot
}

fn tracker_again(name: &str) -> Snapshot {
    Snapshot::load(&Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search"), NaiveDate::from_ymd_opt(2026, 10, 31).unwrap())
}

/// A snapshot that misses the picked card hides its details, and they come back with the card. Esc stays free meanwhile.
#[test]
fn a_picked_application_comes_back_with_its_card() {
    let name = "application_vanishes";
    let mut shell = shell(Screen::Tracker, name);
    let feed = shell.snapshot_feed();
    let mut harness = tray_harness(shell);
    let open = |harness: &Harness| harness.query_all_by_label("Granite Cloud").count() == 2;
    harness.get_by_label(GRANITE_CARD).click();
    harness.run_steps(3);
    assert!(open(&harness));

    feed.send(tracker_without(name, "Granite Cloud")).unwrap();
    harness.run_steps(3);
    assert!(!is_shown(&harness, "Granite Cloud"), "the card and its details are gone");
    press(&mut harness, Key::Escape);

    feed.send(tracker_again(name)).unwrap();
    harness.run_steps(3);
    assert!(open(&harness), "the details came back with the card");
}

/// However the user comes back to a dossier whose application has gone, Applications shows instead.
#[test]
fn a_gone_dossier_falls_back_to_applications() {
    let name = "dossier_gone";
    let mut shell = shell(Screen::Tracker, name);
    let feed = shell.snapshot_feed();
    shell.open_dossier("tessel_group_solution_architect", jobdesk::dossier::page_store(Arc::new(|| {})));
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(SharedSink::default())));
    let mut harness = tray_harness(shell);
    let applications = "Pick an application for its details and dossier.";
    assert!(!is_shown(&harness, applications), "the dossier shows");
    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);
    harness.get_by_label(LAUNCHER);

    feed.send(tracker_without(name, "Tessel Group")).unwrap();
    harness.run_steps(3);
    press(&mut harness, Key::Escape);
    assert!(is_shown(&harness, applications), "Esc led to the dossier, which fell back");
    assert!(!is_shown(&harness, "Tessel Group"));
}

fn request_close(harness: &mut Harness) {
    harness.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
    harness.run_steps(3);
    assert!(is_shown(harness, "Keep working"));
}

/// The quit dialog takes the first Esc, even from a field that had the focus when it opened.
#[test]
fn esc_keeps_working_from_a_focused_field() {
    let mut harness = tray_harness_with(apply_stop_after_evaluate(), "quit_from_field");
    let reply = harness.get_by_role(egui::accesskit::Role::TextInput);
    reply.focus();
    reply.type_text("one more thing");
    harness.run_steps(2);
    request_close(&mut harness);

    press(&mut harness, Key::Escape);

    assert!(!is_shown(&harness, "Keep working"));
}

/// And over an open popup: one Esc keeps working. The model list is a popup egui tracks, as Run details is not.
#[test]
fn esc_keeps_working_over_an_open_popup() {
    let mut shell = shell(Screen::Launcher, "quit_over_popup");
    *shell.desk_slot() = Some(apply_stop_after_evaluate());
    let mut harness = tray_harness(shell);
    harness.get_by_role(egui::accesskit::Role::ComboBox).click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Haiku"), "the model list is open");
    request_close(&mut harness);

    press(&mut harness, Key::Escape);

    assert!(!is_shown(&harness, "Keep working"));
}

/// While the quit dialog asks, Cmd K opens nothing behind it.
#[test]
fn cmd_k_waits_for_the_quit_dialog() {
    let mut harness = tray_harness_with(apply_stop_after_evaluate(), "quit_cmd_k");
    request_close(&mut harness);

    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);

    assert!(!is_shown(&harness, LAUNCHER));
    assert!(is_shown(&harness, "Keep working"));
    press(&mut harness, Key::Escape);
    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);
    assert!(is_shown(&harness, LAUNCHER), "once it closes, Cmd K opens the launcher");
}

/// Cmd K from a focused reply leaves the field behind, so the next Esc already leaves the launcher.
#[test]
fn esc_leaves_a_launcher_opened_from_a_field() {
    let mut harness = tray_harness_with(apply_stop_after_evaluate(), "launcher_from_field");
    let reply = harness.get_by_role(egui::accesskit::Role::TextInput);
    reply.focus();
    reply.type_text("one more thing");
    harness.run_steps(2);
    harness.key_press_modifiers(COMMAND, Key::K);
    harness.step();
    assert!(is_shown(&harness, LAUNCHER));

    harness.key_press(Key::Escape);
    harness.step();
    harness.run_steps(2);

    assert!(!is_shown(&harness, LAUNCHER));
    assert!(is_shown(&harness, "Send"));
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
    let question = Question {
        text: "Layout check found a hole on page 1. How should the CV be fixed?".into(),
        header: "Layout".into(),
        options: choices,
        multi_select: false,
    };
    AgentEvent::PromptRaised(Prompt {
        request_id: request_id.into(),
        kind: PromptKind::Questions(vec![question]),
        input_json: "{}".into(),
        suggestions_json: None,
    })
}

/// Claude's replies are untrusted: an image in one shows as its alt text and loads nothing.
#[test]
fn a_reply_shows_an_image_as_its_alt_text() {
    let mut shell = shell(Screen::Runs, "reply_image");
    let sink = SharedSink::default();
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
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
    harness.get_by_label(LAUNCHER);
    harness.get_by_label("Tasks, 1 need you").click();
    harness.run_steps(3);
    assert_eq!(harness.get_by_role(egui::accesskit::Role::TextInput).value().as_deref(), Some("keep this"));
}

#[test]
fn the_tabs_use_plain_names() {
    let mut harness = tray_harness(probed(shell(Screen::InTray, "plain_tabs"), FixedTools));
    settle(&mut harness);
    for name in ["Today", "Jobs", "Applications", "Insights", "Tasks", "Profile", "Portals", "Setup"] {
        harness.get_by_label_contains(name);
    }
    assert!(harness.query_by_label_contains("In-tray").is_none());
}

/// Every tool answers, so nothing required is missing.
struct AllTools;

impl ToolProber for AllTools {
    fn probe(&self, _: &ToolSpec) -> Option<String> {
        Some("1.0".to_owned())
    }
}

/// `shell` with its tool check started on `prober`, as the app starts it at launch.
fn probed(mut shell: Shell, prober: impl ToolProber + 'static) -> Shell {
    let mut checks = EnvironmentChecks::new(Arc::new(prober), Arc::new(|| {}));
    checks.ensure_started();
    *shell.environment_slot() = Some(checks);
    shell
}

/// Steps until the tool check is back, as `render_probed` does.
fn settle(harness: &mut Harness) {
    for _ in 0..20 {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// A harness that keeps the shell within reach, so a test can ask it what is open.
fn shared_harness(shell: Shell) -> (Harness<'static>, Rc<RefCell<Shell>>) {
    let shared = Rc::new(RefCell::new(shell));
    let inner = Rc::clone(&shared);
    let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(install_look).build_ui(move |ui| inner.borrow_mut().show(ui));
    harness.run_steps(3);
    (harness, shared)
}

fn cmd_comma(harness: &mut Harness) {
    harness.key_press_modifiers(COMMAND, Key::Comma);
    harness.run_steps(3);
}

fn search<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
    harness.get_by_role(egui::accesskit::Role::TextInput)
}

/// Cmd , opens Settings while a field has focus, and the field keeps what it holds.
#[test]
fn cmd_comma_opens_settings_even_from_a_field() {
    let mut shell = shell(Screen::Runs, "settings_from_field");
    *shell.desk_slot() = Some(apply_stop_after_evaluate());
    let (mut harness, shell) = shared_harness(shell);
    let field = harness.get_by_role(egui::accesskit::Role::TextInput);
    field.focus();
    field.type_text("keep this");
    harness.run_steps(2);

    cmd_comma(&mut harness);
    assert!(shell.borrow().settings_open());
    assert!(harness.query_by_label_contains("Back to app").is_some());
    assert!(!is_shown(&harness, "Tasks, 1 need you"), "the menu is gone");

    press(&mut harness, Key::Escape);
    assert!(!shell.borrow().settings_open());
    assert_eq!(harness.get_by_role(egui::accesskit::Role::TextInput).value().as_deref(), Some("keep this"));
}

/// Esc first clears the search, then leaves the field, then closes Settings on the screen that was open.
#[test]
fn esc_clears_the_search_then_leaves_the_field_then_closes_settings() {
    let (mut harness, shell) = shared_harness(shell(Screen::Insights, "settings_esc"));
    cmd_comma(&mut harness);
    search(&harness).focus();
    search(&harness).type_text("folder");
    harness.run_steps(3);
    assert!(is_shown(&harness, "11 settings match. Esc clears the search."));
    assert!(search(&harness).is_focused(), "the field keeps the focus once Clear shows beside it");

    press(&mut harness, Key::Escape);
    assert_eq!(search(&harness).value().as_deref(), Some(""), "the search is cleared");
    assert!(shell.borrow().settings_open(), "and Settings stays");

    search(&harness).focus();
    harness.run_steps(2);
    press(&mut harness, Key::Escape);
    assert!(!search(&harness).is_focused(), "the field let go");
    assert!(shell.borrow().settings_open());

    press(&mut harness, Key::Escape);
    assert!(!shell.borrow().settings_open());
    harness.get_by_label("Tune scoring…");
}

#[test]
fn settings_reopens_on_the_last_page() {
    let (mut harness, _) = shared_harness(shell(Screen::InTray, "settings_last_page"));
    cmd_comma(&mut harness);
    assert!(is_shown(&harness, "Your job search folder, and the screen Jobdesk opens on."));
    harness.get_by_label("Appearance").click();
    harness.run_steps(3);
    press(&mut harness, Key::Escape);
    assert!(!is_shown(&harness, "How Jobdesk looks on this computer."));

    cmd_comma(&mut harness);
    assert!(is_shown(&harness, "How Jobdesk looks on this computer."));
}

fn settings_button<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Settings")
}

#[test]
fn the_status_button_shows_settings_as_on() {
    let (mut harness, shell) = shared_harness(shell(Screen::InTray, "settings_button"));
    assert_eq!(settings_button(&harness).accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
    settings_button(&harness).click();
    harness.run_steps(3);
    assert!(shell.borrow().settings_open());
    assert_eq!(settings_button(&harness).accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
}

/// A running task keeps its folder: Change folder waits on Settings › General and on Setup, and the task stays live.
#[test]
fn change_folder_waits_while_a_task_runs() {
    let mut settings = shell(Screen::InTray, "change_folder_settings");
    *settings.desk_slot() = Some(apply_run(permission_prompt()));
    settings.open_settings(Page::General);
    let (harness, settings) = shared_harness(settings);
    assert!(harness.get_by_label("Change folder").accesskit_node().is_disabled(), "General waits");
    let show = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in Explorer" };
    assert!(!harness.get_by_label(show).accesskit_node().is_disabled(), "the folder can still be shown");
    assert!(settings.borrow_mut().desk_slot().as_ref().unwrap().has_live_process());

    let mut setup = shell(Screen::Workspace, "change_folder_setup");
    *setup.desk_slot() = Some(apply_run(permission_prompt()));
    let mut checks = EnvironmentChecks::new(Arc::new(FixedTools), Arc::new(|| {}));
    checks.ensure_started();
    *setup.environment_slot() = Some(checks);
    let harness = tray_harness(setup);
    assert!(harness.get_by_label("Change folder").accesskit_node().is_disabled(), "Setup waits");
}

/// With no task running, Change folder answers on both.
#[test]
fn change_folder_is_open_with_no_task() {
    let mut settings = shell(Screen::InTray, "change_folder_idle");
    settings.open_settings(Page::General);
    let harness = tray_harness(settings);
    assert!(!harness.get_by_label("Change folder").accesskit_node().is_disabled());

    let mut setup = shell(Screen::Workspace, "change_folder_setup_idle");
    let mut checks = EnvironmentChecks::new(Arc::new(FixedTools), Arc::new(|| {}));
    checks.ensure_started();
    *setup.environment_slot() = Some(checks);
    let harness = tray_harness(setup);
    assert!(!harness.get_by_label("Change folder").accesskit_node().is_disabled());
}

#[test]
fn every_section_opens() {
    let mut shell = shell(Screen::InTray, "settings_every_section");
    shell.open_settings(Page::General);
    let mut harness = tray_harness(shell);
    assert_eq!(harness.query_all_by_label("Later").count(), 0, "no page waits");
    for page in Page::ALL {
        let row = harness.query_all_by_label(page.title()).next().unwrap_or_else(|| panic!("{} is listed", page.title()));
        assert!(!row.accesskit_node().is_disabled(), "{}", page.title());
        row.click();
        harness.run_steps(3);
        // Runtime status is the workspace's page, which this shell, with no tool check, does not draw.
        if page != Page::Runtime {
            assert!(is_shown(&harness, page.lead()), "{} opens", page.title());
        }
    }
}

/// Your terms read from the sample profile, and the rail says where they come from.
#[test]
fn your_terms_show_the_sample_profile() {
    let mut shell = shell(Screen::InTray, "settings_terms_values");
    shell.open_settings(Page::Terms);
    let harness = tray_harness(shell);
    for value in ["Freelance only", "EUR 700 a day", "From Namur (by car).", "Ideal", "Namur, Charleroi", "Too far", "From profile"] {
        assert!(is_shown(&harness, value), "{value}");
    }
}

/// Edit in Profile leaves Settings for the Profile tab.
#[test]
fn edit_in_profile_shows_profile() {
    let mut shell = shell(Screen::Jobs, "settings_terms_edit");
    shell.open_settings(Page::Terms);
    let (mut harness, shell) = shared_harness(shell);
    harness.get_by_label("Edit in Profile").click();
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open());
    assert!(is_shown(&harness, PROFILE_SUBTITLE));
}

/// Update with Claude… opens the launcher on the goals section, and the user starts the run there. Show the command shows
/// the line first, and starts nothing.
#[test]
fn update_your_terms_with_claude_opens_the_launcher_and_starts_nothing() {
    let calls = SharedCalls::default();
    let mut shell = recording_shell(Screen::InTray, "settings_terms_claude", &calls);
    shell.open_settings(Page::Terms);
    let (mut harness, shell) = shared_harness(shell);
    assert!(!is_shown(&harness, "/setup --section goals"));
    harness.get_by_label("Show the command").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "/setup --section goals"), "the line shows");
    assert!(shell.borrow().settings_open() && !is_shown(&harness, LAUNCHER), "and nothing opens");

    harness.get_by_label("Update with Claude…").click();
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open());
    assert!(is_shown(&harness, LAUNCHER), "the launcher opens");
    assert!(harness.query_all_by_label("/setup --section goals").count() > 0, "on the goals section");
    assert_eq!(calls.lock().unwrap().starts, 0, "no run starts");
}

/// Picks `choice` on Settings › Today queue, then goes back to Today.
fn pick_in_queue(harness: &mut Harness, choice: &str) {
    cmd_comma(harness);
    harness.get_by_label("Today queue").click();
    harness.run_steps(3);
    harness.get_by_label(choice).click();
    harness.run_steps(3);
    press(harness, Key::Escape);
}

/// Coming up's range reaches Today: its panel says the new span.
#[test]
fn coming_up_in_settings_reaches_today() {
    let mut harness = tray_harness(shell(Screen::InTray, "settings_queue_coming_up"));
    assert!(is_shown(&harness, "2 weeks"));
    pick_in_queue(&mut harness, "7 days");
    assert!(is_shown(&harness, "1 week"), "Today's Coming up looks a week ahead");
    assert!(!is_shown(&harness, "2 weeks"));
}

/// How long Later lasts reaches Today: on Saturday, Until Monday sets the card aside until Monday.
#[test]
fn later_in_settings_reaches_today() {
    let mut harness = tray_harness(shell(Screen::InTray, "settings_queue_later"));
    pick_in_queue(&mut harness, "Until Monday");
    assert!(is_shown(&harness, HARBOR));
    press(&mut harness, Key::L);
    assert!(!is_shown(&harness, HARBOR));
    assert!(is_shown(&harness, "Last decision on Harbor Bank: Set aside until Monday. Press U to undo."));
}

/// A skip reason added in Settings is offered on Today's next skip.
#[test]
fn a_skip_reason_added_in_settings_is_offered_on_today() {
    let mut harness = tray_harness(shell(Screen::InTray, "settings_queue_reason"));
    cmd_comma(&mut harness);
    harness.get_by_label("Today queue").click();
    harness.run_steps(3);
    let field = harness.get_by(|node| node.role() == egui::accesskit::Role::TextInput && node.label().as_deref() == Some("New skip reason"));
    field.focus();
    field.type_text("Culture");
    harness.run_steps(2);
    harness.get_by_label("Add reason").click();
    harness.run_steps(3);
    press(&mut harness, Key::Escape);
    assert!(is_shown(&harness, HARBOR));
    // Harbor's card is a send, which has no skip. Meridian's comes after two set aside.
    press(&mut harness, Key::L);
    press(&mut harness, Key::L);
    assert!(is_shown(&harness, MERIDIAN));
    press(&mut harness, Key::N);
    harness.get_by_label("Culture").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Skipped for culture"), "the new reason is offered and taken");
}

/// The Jobs view hides a flagged ad from Jobs, and Today still deals it as Check this ad, so the user decides on it.
#[test]
fn a_flagged_ad_hidden_from_jobs_still_deals_on_today() {
    let mut today = shell(Screen::InTray, "settings_flagged_today");
    today.settings_mut().set_hide_flagged(true);
    let mut harness = tray_harness(today);
    for _ in 0..4 {
        press(&mut harness, Key::L);
    }
    assert!(is_shown(&harness, "Check this ad"), "Today deals the flagged ad");
    assert!(is_shown(&harness, "Harmless, keep posting"));

    let mut jobs = shell(Screen::Jobs, "settings_flagged_jobs");
    jobs.settings_mut().set_hide_flagged(true);
    let mut harness = tray_harness(jobs);
    // Halden's flagged ad is in Low score.
    harness.get_by_label_contains("Low score").click();
    harness.run_steps(3);
    assert!(harness.query_by_label_contains("1 job hidden by Settings › Jobs view").is_some(), "Jobs hides it and says so");
}

/// The filters hide rows on Jobs, and Jobs view counts what they hide, in its note and in the rail.
#[test]
fn the_jobs_view_counts_what_jobs_hides() {
    let mut shell = shell(Screen::Jobs, "settings_jobs_count");
    shell.settings_mut().set_hide_under_score(true);
    shell.open_settings(Page::JobsView);
    let harness = tray_harness(shell);
    let note = harness.get_by_label_contains("Jobs hides ").accesskit_node().value().unwrap_or_default();
    let hidden: usize = note.trim_start_matches("Jobs hides ").split(' ').next().unwrap().parse().unwrap();
    assert!(hidden > 0, "{note}");
    assert!(is_shown(&harness, &format!("{hidden} hidden")), "the rail says the same");
}

/// With Reduce motion ticked, a decided card is gone on the next frame, with no slide.
#[test]
fn reduce_motion_drops_the_slide() {
    let mut shell = shell(Screen::InTray, "settings_reduce_motion");
    shell.open_settings(Page::Appearance);
    let mut harness =
        Harness::builder().with_size([1280.0, 820.0]).with_step_dt(1.0 / 60.0).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    harness.get_by_role(egui::accesskit::Role::CheckBox).click();
    harness.run_steps(3);
    press(&mut harness, Key::Escape);
    assert!(is_shown(&harness, HARBOR));

    harness.key_press(Key::L);
    harness.run_steps(1);
    assert!(!is_shown(&harness, HARBOR));
    assert!(is_shown(&harness, &format!("1 of {}", TRAY_CARDS - 1)));
}

/// Cmd + zooms the window, and Appearance shows the new size as the one chosen.
#[test]
fn cmd_plus_follows_in_text_size() {
    let mut harness = tray_harness(shell(Screen::InTray, "settings_zoom"));
    harness.key_press_modifiers(COMMAND, Key::Plus);
    harness.run_steps(3);
    cmd_comma(&mut harness);
    harness.get_by_label("Appearance").click();
    harness.run_steps(3);
    assert_eq!(harness.get_by_label("110%").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
    assert_eq!(harness.get_by_label("100%").accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
}

/// Only a missing folder or a missing required tool keeps the Setup tab. Profile placeholders never do.
#[test]
fn the_setup_tab_shows_only_while_something_required_is_not_ready() {
    let setup_tab = |harness: &Harness| harness.query_by_role_and_label(egui::accesskit::Role::Tab, "Setup").is_some();
    let mut harness = tray_harness(probed(shell(Screen::InTray, "setup_tab_ready"), AllTools));
    settle(&mut harness);
    assert!(!setup_tab(&harness), "every tool answers");
    cmd_comma(&mut harness);
    assert!(harness.query_by_label_contains(" to fill").is_some(), "though the sample profile still has placeholders");

    let mut harness = tray_harness(probed(shell(Screen::Workspace, "setup_tab_open"), AllTools));
    settle(&mut harness);
    assert!(setup_tab(&harness), "Setup on show keeps its tab");

    let mut harness = tray_harness(probed(shell(Screen::InTray, "setup_tab_bun"), FixedTools));
    settle(&mut harness);
    assert!(setup_tab(&harness), "bun is missing");

    let harness = tray_harness(Shell::without_workspace(NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()));
    assert!(setup_tab(&harness), "no folder is chosen yet");
}

/// Settings hides the tabs, so a task that waits says so in the status line.
#[test]
fn a_waiting_task_shows_in_the_status_line_while_settings_is_open() {
    let mut harness = tray_harness_with(apply_stop_after_evaluate(), "settings_waits");
    assert!(!is_shown(&harness, "A task waits for you"), "the Tasks tab counts it");
    cmd_comma(&mut harness);
    assert!(is_shown(&harness, "A task waits for you"));
    assert!(!is_shown(&harness, "Claude is ready"));
    press(&mut harness, Key::Escape);
    assert!(!is_shown(&harness, "A task waits for you"));
}

/// Search looks through the index, so each of its lines must be on Runtime status.
#[test]
fn every_runtime_index_line_is_on_its_page() {
    let mut shell = probed(shell(Screen::InTray, "settings_runtime_index"), FixedTools);
    shell.open_settings(Page::Runtime);
    let mut harness = tray_harness(shell);
    settle(&mut harness);
    for line in drawn_index(Page::Runtime, Drawn::folder(true)) {
        assert!(harness.query_all_by_label(line).next().is_some(), "{line}");
    }
    assert_eq!(drawn_index(Page::Runtime, Drawn::folder(true)), index(Page::Runtime), "with the folder read, every line is drawn");

    // With no folder, the lines a search can match are the ones drawn, and nothing more.
    let mut shell = probed(Shell::without_workspace(NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()), FixedTools);
    shell.open_settings(Page::Runtime);
    let mut harness = tray_harness(shell);
    settle(&mut harness);
    for &line in index(Page::Runtime) {
        let drawn = harness.query_all_by_label(line).next().is_some();
        assert_eq!(drawn, drawn_index(Page::Runtime, Drawn::folder(false)).contains(&line), "{line}");
    }
}

/// With no folder, Sync is not drawn on Runtime status, so a search for it finds nothing there.
#[test]
fn with_no_folder_a_search_for_sync_has_no_runtime_hit() {
    let mut shell = probed(Shell::without_workspace(NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()), FixedTools);
    shell.open_settings(Page::Runtime);
    let mut harness = tray_harness(shell);
    settle(&mut harness);
    search(&harness).focus();
    search(&harness).type_text("sync");
    harness.run_steps(3);
    assert!(is_shown(&harness, "No setting matches"));
    assert!(harness.query_by_label("1 match").is_none());
}

/// Cmd K in Settings closes it and opens the launcher, which keeps Settings' page for next time.
#[test]
fn cmd_k_in_settings_opens_the_launcher() {
    let calls = SharedCalls::default();
    let (mut harness, shell) = shared_harness(recording_shell(Screen::Insights, "settings_cmd_k", &calls));
    cmd_comma(&mut harness);
    assert!(shell.borrow().settings_open());
    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open());
    assert!(is_shown(&harness, LAUNCHER));
    assert_eq!(calls.lock().unwrap().starts, 0, "the launcher starts nothing by itself");
}

/// The welcome's second step names Profile, whose tab stays once the folder is chosen and every tool answers, when the
/// Setup tab has left the menu. Its promise says when Jobdesk changes the folder.
#[test]
fn the_welcome_points_at_a_place_that_stays() {
    let harness = tray_harness(probed(Shell::without_workspace(NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()), AllTools));
    assert!(is_shown(&harness, "Profile shows each topic, what is filled in and what is missing."));
    assert!(is_shown(
        &harness,
        "Jobdesk reads this folder and changes it only when you save a Profile edit, switch a portal on or off, or remove one, and when a Jobdesk update brings newer versions of its files, which you can undo right after. Tasks you start change it through Claude Code on this computer."
    ));

    let mut harness = tray_harness(probed(shell(Screen::InTray, "welcome_step_reachable"), AllTools));
    settle(&mut harness);
    assert!(harness.query_by_role_and_label(egui::accesskit::Role::Tab, "Setup").is_none(), "the Setup tab has gone");
    harness.get_by_role_and_label(egui::accesskit::Role::Tab, "Profile").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, PROFILE_SUBTITLE), "and Profile shows the topics");
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
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    desk.set_asks_permission(true);
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
    let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
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
    assert!(is_shown(&harness, "Pick an application for its details and dossier."));
}

/// The hint that a card opens details shows even when nothing waits in Today, and then the pink edge goes unexplained.
#[test]
fn the_applications_legend_hints_at_details_when_nothing_waits() {
    let mut harness = tray_harness(shell(Screen::InTray, "applications_legend_plain"));
    for _ in 0..TRAY_CARDS {
        press(&mut harness, Key::L);
    }
    harness.get_by_label_contains("Applications").click();
    harness.run_steps(3);
    assert!(harness.query_by_label_contains("wait for you in Today.").is_none(), "nothing waits in Today");
    assert!(is_shown(&harness, "Pick an application for its details and dossier."));
    assert!(!is_shown(&harness, "A pink edge means the application waits for you in Today."));
}

/// One action in the Setup header opens `/expand` in the launcher, and the rows carry none. No run starts.
#[test]
fn setup_offers_one_look_for_more_in_your_documents() {
    let started = SharedSink::default();
    let mut shell = shell(Screen::Workspace, "setup_expand");
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(Arc::clone(&started))));
    let mut checks = EnvironmentChecks::new(Arc::new(FixedTools), Arc::new(|| {}));
    checks.ensure_started();
    *shell.environment_slot() = Some(checks);
    let mut harness = tray_harness(shell);
    assert_eq!(harness.query_all_by_label("Look for more in your documents…").count(), 1, "one action, in the header, not one per row");
    assert!(!is_shown(&harness, LAUNCHER));

    harness.get_by_label("Look for more in your documents…").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, LAUNCHER), "the launcher opens");
    assert!(harness.query_all_by_label("/expand").count() > 0, "the line is /expand, with no file");
    assert!(started.lock().unwrap().is_none(), "no run starts");
}

/// Text in a row takes no click of its own, so a click on a job's title picks the job, and the pointer shows the hand.
#[test]
fn a_click_on_a_job_title_picks_the_job() {
    let mut harness = tray_harness(shell(Screen::Jobs, "job_title_click"));
    let second = "Solution Architect, Event Sourcing";
    assert_eq!(harness.query_all_by_label(second).count(), 1, "the table lists it, the detail shows another job");
    harness.get_by_label(second).hover();
    harness.step();
    assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::PointingHand);

    harness.get_by_label(second).click();
    harness.run_steps(3);

    assert_eq!(harness.query_all_by_label(second).count(), 2, "the detail shows the job now");
    assert_eq!(harness.query_all_by_label(MERIDIAN).count(), 1, "and no longer the first");
}

const MERIDIAN_ROW: &str = "Tech Lead Java, Real-Time Traffic, Meridian Rail Systems, 84";
const KESTREL_ROW: &str = "Solution Architect, Event Sourcing, Kestrel Payments, 78";

/// A job's row is one Tab stop, named for screen readers, and the table comes before the detail's actions.
/// Every segment is its own stop, so Tab starts from the last one.
#[test]
fn tab_reaches_the_first_job_in_one_press() {
    let mut harness = tray_harness(shell(Screen::Jobs, "jobs_tab"));
    harness.get_by_label_contains("Skipped").focus();
    harness.run_steps(2);
    press(&mut harness, Key::Tab);
    assert!(harness.get_by_label(MERIDIAN_ROW).is_focused(), "the first job takes the next stop");
    press(&mut harness, Key::Tab);
    assert!(harness.get_by_label(KESTREL_ROW).is_focused(), "the second job one stop further");
    press(&mut harness, Key::Enter);
    assert_eq!(harness.query_all_by_label("Solution Architect, Event Sourcing").count(), 2, "Enter picks the job");
}

/// Up and Down move the pick, and the focus goes with it.
#[test]
fn arrow_keys_move_the_pick() {
    let mut harness = tray_harness(shell(Screen::Jobs, "jobs_arrows"));
    let second = "Solution Architect, Event Sourcing";
    press(&mut harness, Key::ArrowDown);
    assert_eq!(harness.query_all_by_label(second).count(), 2, "the detail shows the second job");
    press(&mut harness, Key::ArrowUp);
    assert_eq!(harness.query_all_by_label(MERIDIAN).count(), 2, "and the first again");

    harness.get_by_label(MERIDIAN_ROW).focus();
    harness.run_steps(2);
    press(&mut harness, Key::ArrowDown);
    assert!(harness.get_by_label(KESTREL_ROW).is_focused(), "the focus follows the pick");
    assert_eq!(harness.query_all_by_label(second).count(), 2);
}

/// A focused button keeps its arrows, and so does a dialog: only a row or no focus at all lets them move the pick.
#[test]
fn arrow_keys_leave_the_pick_alone_away_from_the_table() {
    let mut shell = shell(Screen::Jobs, "jobs_arrows_held");
    *shell.desk_slot() = Some(apply_stop_after_evaluate());
    let mut harness = tray_harness(shell);
    let second = "Solution Architect, Event Sourcing";
    harness.get_by_label("Draft application…").focus();
    harness.run_steps(2);
    press(&mut harness, Key::ArrowDown);
    assert_eq!(harness.query_all_by_label(MERIDIAN).count(), 2, "the detail still shows the first job");
    assert_eq!(harness.query_all_by_label(second).count(), 1);

    harness.get_by_label(MERIDIAN_ROW).focus();
    harness.run_steps(2);
    request_close(&mut harness);
    harness.get_by_label(MERIDIAN_ROW).focus();
    harness.run_steps(2);
    press(&mut harness, Key::ArrowDown);
    assert_eq!(harness.query_all_by_label(MERIDIAN).count(), 2, "the quit dialog holds the pick");
}

/// A click leaves no keyboard focus behind on a segment, so the arrows still move the pick after one.
#[test]
fn arrow_keys_work_after_clicking_a_segment() {
    let mut harness = tray_harness(shell(Screen::Jobs, "jobs_arrows_clicked"));
    let second = "Solution Architect, Event Sourcing";
    harness.get_by_label_contains("Shortlist").click();
    harness.run_steps(2);
    assert!(!harness.get_by_label_contains("Shortlist").is_focused(), "the click leaves no focus on the segment");
    press(&mut harness, Key::ArrowDown);
    assert_eq!(harness.query_all_by_label(second).count(), 2, "the detail shows the second job");
}

/// The table cuts a long title, and the pointer on it shows the whole title. A row's text sits over the row's click,
/// so it still takes the hover.
#[test]
fn a_cut_job_title_shows_in_full_on_hover() {
    let mut harness = tray_harness(shell(Screen::Jobs, "jobs_cut_title"));
    let second = "Solution Architect, Event Sourcing";
    assert_eq!(harness.query_all_by_label(second).count(), 1, "only the table lists it");
    harness.get_by_label(second).hover();
    harness.run_steps(4);
    assert_eq!(harness.query_all_by_label(second).count(), 2, "the tooltip shows the title");

    harness.get_by_label("+1").hover();
    harness.run_steps(4);
    assert!(harness.query_by_label_contains("Also: ").is_some(), "+1 names the other signal");
}

/// Coming up cuts a long line to the room it has, and the pointer on the row shows the whole line.
#[test]
fn a_cut_coming_up_row_shows_in_full_on_hover() {
    let mut harness = tray_harness(shell_on(Screen::InTray, "tray_cut_row", overdue_day()));
    assert_eq!(harness.query_all_by_label("Harbor Bank deadline").count(), 0, "only the row's name holds the line");
    harness.get_by_label("Tue 3, Harbor Bank deadline, 4 days late, now").hover();
    harness.run_steps(4);
    assert_eq!(harness.query_all_by_label("Harbor Bank deadline").count(), 1, "the tooltip shows the whole line");
}

/// The detail names the site, so a narrow window drops the column and keeps the rest in view.
#[test]
fn the_found_on_column_drops_below_1100() {
    let mut narrow = Harness::builder().with_size([1040.0, 640.0]).with_context_setup(install_look).build_ui({
        let mut shell = shell(Screen::Jobs, "jobs_narrow");
        move |ui| shell.show(ui)
    });
    narrow.run_steps(3);
    assert!(!is_shown(&narrow, "Found on"));
    let harness = tray_harness(shell(Screen::Jobs, "jobs_wide"));
    assert!(is_shown(&harness, "Found on"));
}

/// The same holds for an application card: a click on its role opens the details.
#[test]
fn a_click_on_a_card_role_opens_its_details() {
    let mut harness = tray_harness(shell(Screen::Tracker, "card_role_click"));
    assert_eq!(harness.query_all_by_label("Granite Cloud").count(), 1, "only the card names the company");
    harness.get_by_label("Platform Lead").click();
    harness.run_steps(3);
    assert_eq!(harness.query_all_by_label("Granite Cloud").count(), 2, "the detail slip names it too");
}

const TRAY_CARDS: usize = 10;
const HARBOR: &str = "Lead Java Engineer";
const MERIDIAN: &str = "Tech Lead Java, Real-Time Traffic";

/// Y on a card, launcher form, run, then the Runs screen.
#[test]
fn y_leads_from_a_card_to_a_started_run() {
    let mut shell = shell(Screen::InTray, "tray_click_through");
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(SharedSink::default())));
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, HARBOR));

    press(&mut harness, Key::Y);
    harness.get_by_label(LAUNCHER);
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
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(Arc::clone(&started))));
    let mut harness = tray_harness(shell);
    for _ in 0..TRAY_CARDS {
        press(&mut harness, Key::L);
    }
    assert!(is_shown(&harness, "Want more to look at?"));
    assert!(is_shown(&harness, "Score 2 new jobs…"), "the sample board holds two unscored jobs");
    assert!(is_shown(&harness, &format!("{TRAY_CARDS} cards set aside with Later come back tomorrow. ")));
    assert!(!is_shown(&harness, LAUNCHER));

    harness.get_by_label("Find new jobs…").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, LAUNCHER), "the launcher opens");
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
    let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_step_dt(frame).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
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
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(Arc::clone(&started))));
    let mut harness = tray_harness(shell);
    press(&mut harness, Key::L);
    assert!(is_shown(&harness, "1 set aside, back tomorrow: Harbor Bank. Bring it back now."));

    harness.get_by_label("Bring back").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, HARBOR), "the card brought back is on top again");
    assert!(tallied(&harness, 0, TRAY_CARDS));
    assert!(harness.query_by_label_contains("set aside, back tomorrow").is_none());
    assert!(!is_shown(&harness, LAUNCHER), "the row opens no launcher");
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
    // Jobs says in its subtitle that scores rest on the ad alone, so its line leaves that out.
    let on_jobs = line.strip_suffix(" Scored from the job ad alone.").expect("Today says where the score comes from");
    assert!(is_shown(&jobs, on_jobs), "{on_jobs} on Jobs");
    for value in values {
        assert!(jobs.query_all_by_label(value).count() > 0, "{value} on Jobs");
    }

    // Halden's ad was flagged. Kept in Today, Jobs words it as Today's card does.
    let halden = |kept: &str| format!("Moderate fit, 58 of 100. Five checks pass. The safety check flagged this ad{kept}");
    jobs.get_by_label_contains("Low score").click();
    jobs.run_steps(3);
    assert!(is_shown(&jobs, &halden(", so read what it found before you apply.")), "not kept yet");

    let mut harness = tray_harness(shell(Screen::InTray, "checks_kept"));
    for _ in 0..4 {
        press(&mut harness, Key::L);
    }
    harness.get_by_label("Harmless, keep posting").click();
    harness.run_steps(3);
    harness.get_by_label_contains("Jobs").click();
    harness.run_steps(3);
    harness.get_by_label_contains("Low score").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, &halden(" and you kept it.")), "kept in Today");
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
    *shell.desk_slot() = Some(desk_on(gateway));
    let mut harness = Harness::builder().with_size([1040.0, 640.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
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

/// The job's actions sit under the detail's scroll, so a long detail keeps them in view.
#[test]
fn the_job_actions_stay_in_view() {
    for size in [[1280.0, 820.0], [1040.0, 640.0]] {
        let mut shell = shell(Screen::Jobs, "job_actions_in_view");
        let mut harness = Harness::builder().with_size(size).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
        harness.run_steps(3);
        assert_in_window(&harness, size, &["Draft application…", "Find skill gaps…", "Open job ad"]);
    }
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
    assert!(!is_shown(&harness, LAUNCHER), "no launcher opens");
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
    let mut harness = Harness::builder().with_size([1040.0, 640.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
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
    assert!(is_shown(&harness, "Tue 3, Harbor Bank deadline, 4 days late, now"));
    assert!(is_shown(&harness, "Nothing else dated before Fri 20 Nov."), "late rows leave the next two weeks empty");
}

/// The card on top is the one being read, late or not, so its row takes no click. The late card under it can be pulled up,
/// until it comes on top in turn.
#[test]
fn the_late_card_on_top_has_an_inert_row() {
    let mut harness = tray_harness(shell_on(Screen::InTray, "tray_overdue_top", overdue_day()));
    let late_row = |harness: &Harness, role| harness.query_all_by_label_contains("1 day late").any(|node| node.accesskit_node().role() == role);
    let reading = "Tue 3, Harbor Bank deadline, 4 days late, now";
    assert!(harness.query_by_role_and_label(egui::accesskit::Role::Label, reading).is_some(), "the row is no button");
    assert!(late_row(&harness, egui::accesskit::Role::Button), "the next late card can be pulled up");
    harness.get_by_label(reading).hover();
    harness.step();
    assert_ne!(harness.output().platform_output.cursor_icon, CursorIcon::PointingHand);

    press(&mut harness, Key::L);

    assert!(late_row(&harness, egui::accesskit::Role::Label), "on top, it is read, not pulled up");
    assert!(!late_row(&harness, egui::accesskit::Role::Button));
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
    compare(&mut harness, name);
}

fn wgpu_tray(mut shell: Shell, theme: Theme) -> Harness<'static> {
    let mut harness =
        with_renderer(Harness::builder().with_size([1280.0, 820.0]).with_pixels_per_point(1.5).with_theme(theme).with_context_setup(install_look))
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
    empty_shell_on(Screen::InTray, name)
}

fn empty_shell_on(screen: Screen, name: &str) -> Shell {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".claude/commands")).unwrap();
    fs::write(root.join(".claude/commands/apply.md"), "").unwrap();
    let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    Shell::with_snapshot(screen, jobdesk::workspace::open(&root).unwrap(), snapshot)
}

/// An empty screen offers a verb, and the raw command stays in the hover text. Each button opens the launcher and starts nothing.
#[test]
fn empty_screens_offer_verbs_not_commands() {
    for (screen, verb, command) in [(Screen::Tracker, "Draft an application…", "/apply"), (Screen::Insights, "Find skill gaps…", "/upskill")] {
        let started = SharedSink::default();
        let mut shell = empty_shell_on(screen, "empty_verbs");
        *shell.desk_slot() = Some(desk_on(ScriptedGateway(Arc::clone(&started))));
        // Tall, so Insights shows its skill gap section without scrolling.
        let mut harness = Harness::builder().with_size([1280.0, 1400.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
        harness.run_steps(3);
        assert!(is_shown(&harness, verb), "{verb} is shown, once");
        assert!(harness.query_by_label_contains(command).is_none(), "{command} is not");

        harness.get_by_label(verb).click();
        harness.run_steps(3);
        assert!(is_shown(&harness, LAUNCHER), "{verb} opens the launcher");
        assert!(harness.query_all_by_label(command).count() > 0, "the line is filled in");
        assert!(started.lock().unwrap().is_none(), "no run starts");
    }
}

/// With no tasks, Find new jobs opens `/scrape` and New task opens the launcher empty. Neither starts a run.
#[test]
fn the_empty_tasks_screen_opens_the_launcher_and_starts_nothing() {
    for (button, line) in [("Find new jobs…", Some("/scrape")), ("New task…", None)] {
        let started = SharedSink::default();
        let mut shell = empty_shell_on(Screen::Runs, "empty_tasks");
        *shell.desk_slot() = Some(desk_on(ScriptedGateway(Arc::clone(&started))));
        let mut harness = tray_harness(shell);
        assert!(is_shown(&harness, "No tasks yet"));
        assert!(is_shown(
            &harness,
            "A task runs one command through Claude Code in your job search folder. You start each one, and it shows here while it works and after it ends."
        ));
        assert!(!is_shown(&harness, LAUNCHER));

        harness.get_by_label(button).click();
        harness.run_steps(3);
        assert!(is_shown(&harness, LAUNCHER), "{button} opens the launcher");
        if line.is_some() {
            assert!(harness.query_all_by_label("/scrape").count() > 0, "{button}: the line is filled in");
        }
        assert!(started.lock().unwrap().is_none(), "no run starts");
    }
}

#[test]
fn runs_empty_light() {
    let mut shell = shell(Screen::Runs, "runs_empty_light");
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(SharedSink::default())));
    render_with(shell, Theme::Light, "runs_empty_light");
}

#[test]
fn intray_clear_dark() {
    tray_after(wgpu_tray(empty_shell("intray_clear_dark"), Theme::Dark), &[], "intray_clear_dark");
}

/// The sample workspace with a tracker saved in Latin-1, as a spreadsheet's plain CSV export does.
/// With `jobs_too`, the saved jobs file is cut off as well.
fn failed_shell(name: &str, jobs_too: bool) -> Shell {
    let broken: &[Source] = if jobs_too { &[Source::Applications, Source::Jobs] } else { &[Source::Applications] };
    broken_shell(Screen::InTray, name, broken)
}

fn failed_tracker_shell(name: &str) -> Shell {
    failed_shell(name, false)
}

/// The sample workspace with each of `broken`'s files made unreadable, opened on `screen`.
fn broken_shell(screen: Screen, name: &str, broken: &[Source]) -> Shell {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    for source in broken {
        match source {
            Source::Applications => fs::write(root.join("job_search_tracker.csv"), b"Company,Role,Status\nCaf\xe9 Nordlicht,Platform Lead,Applied\n").unwrap(),
            Source::Jobs => fs::write(root.join("job_scraper/seen_jobs.json"), "{\"seen\": {").unwrap(),
            Source::SkillGaps => fs::write(root.join("upskill/report-2026-10-28.md"), b"# Skill gaps\n\nCaf\xe9\n").unwrap(),
            Source::Profile => {
                fs::write(root.join(".claude/skills/job-application-assistant/02-behavioral-profile.md"), b"# Behavioral Profile\n\nCaf\xe9\n").unwrap()
            }
            Source::CvDocuments => {
                fs::remove_dir_all(root.join("cv")).unwrap();
                fs::write(root.join("cv"), b"").unwrap();
            }
            Source::Portals => {
                fs::remove_dir_all(root.join(".agents/skills")).unwrap();
                fs::write(root.join(".agents/skills"), b"").unwrap();
            }
            Source::Decisions => unreachable!("the tray reports its own file"),
        }
    }
    let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    assert_eq!(snapshot.issues.len(), broken.len(), "{:?}", snapshot.issues);
    Shell::with_snapshot(screen, jobdesk::workspace::open(&root).unwrap(), snapshot)
}

const TRACKER_HIDDEN: &str = "Hidden while the tracker cannot be read.";
const JOBS_HIDDEN: &str = "Hidden while your saved jobs cannot be read.";
const PORTALS_HIDDEN: &str = "Your portals are hidden while their folder cannot be read.";
const SETUP_JOBS_HIDDEN: &str = "Jobs found are hidden while your saved jobs cannot be read.";

/// Each screen a failed source reaches draws its head, then the alert, then hides what the source feeds instead of calling it empty.
/// A screen that forgot its alert would still pass every snapshot, so this walks them all.
#[test]
fn every_failed_screen_shows_its_alert_under_its_head() {
    struct Case {
        screen: Screen,
        broken: &'static [Source],
        subtitle: &'static str,
        alert: &'static str,
        shown: &'static [&'static str],
        not_shown: &'static [&'static str],
    }
    let cases = [
        Case {
            screen: Screen::Tracker,
            broken: &[Source::Applications],
            subtitle: "Not shown while the tracker cannot be read.",
            alert: "Your applications could not be read.",
            shown: &["Your applications are hidden, not gone", "Record a result…", "Check mail for replies…"],
            not_shown: &["No applications yet", "Draft an application…", "Not sent yet"],
        },
        Case {
            screen: Screen::Jobs,
            broken: &[Source::Jobs],
            subtitle: "Not shown while your saved jobs cannot be read.",
            alert: "Your saved jobs could not be read.",
            shown: &["Your saved jobs are hidden, not gone", "Find new jobs…"],
            not_shown: &["No jobs wait here.", "Shortlist  0", "New  0"],
        },
        Case {
            screen: Screen::Insights,
            broken: &[Source::Applications],
            subtitle: "The whole workflow, search health and skill gaps, read live from your files",
            alert: "Your applications could not be read.",
            shown: &[],
            not_shown: &["No applications sent yet.", "0 sent", "drafted, not sent", "0 applied, 0 interview, 0 offer", "0 hired, 0 closed", "Tune scoring"],
        },
        Case {
            screen: Screen::Insights,
            broken: &[Source::Jobs],
            subtitle: "The whole workflow, search health and skill gaps, read live from your files",
            alert: "Your saved jobs could not be read.",
            shown: &[],
            not_shown: &["0 new of 0 found", "on shortlist"],
        },
        Case {
            screen: Screen::Insights,
            broken: &[Source::SkillGaps],
            subtitle: "The whole workflow, search health and skill gaps, read live from your files",
            alert: "Your skill gap reports could not be read.",
            shown: &["Skill gap report", "Your skill gap reports are hidden, not gone. They come back by themselves once the report file is fixed."],
            not_shown: &["No skill gap report yet"],
        },
        Case {
            screen: Screen::Workspace,
            broken: &[Source::Applications],
            subtitle: "ai-job-search",
            alert: "Your applications could not be read.",
            shown: &["10 jobs found"],
            not_shown: &[SETUP_JOBS_HIDDEN],
        },
        Case {
            screen: Screen::Workspace,
            broken: &[Source::Jobs],
            subtitle: "ai-job-search",
            alert: "Your saved jobs could not be read.",
            shown: &[SETUP_JOBS_HIDDEN, "4 of 6 portals on"],
            not_shown: &["10 jobs found", "0 jobs found"],
        },
        Case {
            screen: Screen::Portals,
            broken: &[Source::Jobs],
            subtitle: PORTALS_SUBTITLE,
            alert: "Your saved jobs could not be read.",
            shown: &["Jobs found are hidden while your saved jobs cannot be read."],
            not_shown: &["No jobs yet", "4 jobs · last new Fri 30 Oct"],
        },
        Case {
            screen: Screen::Workspace,
            broken: &[Source::Profile],
            subtitle: "ai-job-search",
            alert: "Part of your profile could not be read.",
            shown: &["Everything else still works.", "Profile, could not be read"],
            not_shown: &[],
        },
        Case {
            screen: Screen::Workspace,
            broken: &[Source::Portals],
            subtitle: "ai-job-search",
            alert: "Your portals could not be read.",
            shown: &["Everything else still works.", "Portals, could not be read", PORTALS_HIDDEN, "Open Portals"],
            not_shown: &["No portals installed.", "0 of 0 portals on"],
        },
        Case {
            screen: Screen::Portals,
            broken: &[Source::Portals],
            subtitle: "The job boards Claude searches when you find new jobs.",
            alert: "Your portals could not be read.",
            shown: &["Everything else still works.", "Your portals are hidden, not gone", "Check all portals…"],
            not_shown: &["No portals yet", "The job boards Claude searches when you find new jobs. None installed.", "Claude searches these"],
        },
        Case {
            screen: Screen::Insights,
            broken: &[Source::Profile],
            subtitle: "The whole workflow, search health and skill gaps, read live from your files",
            alert: "Part of your profile could not be read.",
            shown: &["6 of 11 profile topics filled, 1 hidden"],
            not_shown: &["7 of 11 profile topics filled", "6 of 10 profile topics filled", "Insights, could not be read"],
        },
        Case {
            screen: Screen::Profile,
            broken: &[Source::CvDocuments],
            subtitle: "Claude reads this before it scores a job or writes a CV or a letter.",
            alert: "Your CV folder could not be read.",
            shown: &["Your profile topics still show, and every task still runs."],
            not_shown: &[],
        },
    ];
    for (index, case) in cases.iter().enumerate() {
        let mut shell = broken_shell(case.screen, &format!("failed_screen_{index}"), case.broken);
        let mut checks = EnvironmentChecks::new(Arc::new(FixedTools), Arc::new(|| {}));
        checks.ensure_started();
        *shell.environment_slot() = Some(checks);
        let harness = tray_harness(shell);
        let subtitle = harness.get_by_label(case.subtitle).rect();
        let alert = harness.get_by_label(case.alert).rect();
        assert!(alert.top() > subtitle.bottom(), "{:?}: the alert {alert:?} sits under the subtitle {subtitle:?}", case.screen);
        for text in case.shown {
            assert!(is_shown(&harness, text), "{:?} shows {text}", case.screen);
        }
        for text in case.not_shown {
            assert!(!is_shown(&harness, text), "{:?} hides {text}", case.screen);
        }
    }
}

/// An open dossier whose tracker fails stays open and says its application is hidden, not that it left the tracker.
#[test]
fn a_dossier_whose_tracker_fails_is_hidden_not_gone() {
    let mut shell = broken_shell(Screen::Tracker, "dossier_tracker_failed", &[Source::Applications]);
    shell.open_dossier("tessel_group_solution_architect", jobdesk::dossier::page_store(Arc::new(|| {})));
    let harness = tray_harness(shell);
    assert!(is_shown(&harness, "Your applications could not be read."));
    assert!(is_shown(&harness, "Hidden while the tracker cannot be read."));
    assert!(!is_shown(&harness, "This application is no longer in the tracker."));
}

/// Insights hides only what the failed sources feed: each panel and band cell they feed says so, and the rest still counts.
#[test]
fn insights_hides_only_what_a_failed_source_feeds() {
    let harness = tray_harness(broken_shell(Screen::Insights, "insights_hidden_parts", &[Source::Applications, Source::Jobs]));
    assert_eq!(harness.query_all_by_label(TRACKER_HIDDEN).count(), 7, "Status, Funnel, By channel, By sector, Apply, In progress and Result");
    assert_eq!(harness.query_all_by_label(JOBS_HIDDEN).count(), 2, "Find and Score");
    assert!(harness.query_by_label_contains("profile topics filled").is_some(), "the profile still counts");
    assert!(is_shown(&harness, "Skill gaps"), "the skill gap report still shows");

    let harness = tray_harness(shell(Screen::Insights, "insights_nothing_hidden"));
    assert!(harness.query_by_label(TRACKER_HIDDEN).is_none());
    assert!(harness.query_by_label(JOBS_HIDDEN).is_none());
}

#[test]
fn tracker_failed_light() {
    tray_after(wgpu_tray(broken_shell(Screen::Tracker, "tracker_failed_light", &[Source::Applications]), Theme::Light), &[], "tracker_failed_light");
}

#[test]
fn jobs_failed_dark() {
    tray_after(wgpu_tray(broken_shell(Screen::Jobs, "jobs_failed_dark", &[Source::Jobs]), Theme::Dark), &[], "jobs_failed_dark");
}

#[test]
fn insights_failed_light() {
    tray_after(wgpu_tray(broken_shell(Screen::Insights, "insights_failed_light", &[Source::Applications]), Theme::Light), &[], "insights_failed_light");
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

    // Nothing required is missing, so the Setup tab is gone and the alert leads to Runtime status.
    assert!(!is_shown(&harness, "Open Setup"));
    harness.get_by_label("Open Runtime status").click();
    harness.run_steps(3);
    // This shell has no tool check, so Runtime status draws only the alert, in Settings.
    assert!(harness.query_by_label_contains("Back to app").is_some(), "Settings is open on Runtime status");
    assert!(harness.query_by_label_contains("Jobdesk stores no credentials").is_none(), "the status bar no longer says how tasks sign in");
    assert!(is_shown(&harness, "Your applications could not be read."), "the alert shows there too");
    assert!(!is_shown(&harness, "Open Runtime status"), "Runtime status has no button to itself");

    // While a required tool is missing, the alert leads to Setup, which has no button to itself.
    let mut harness = tray_harness(probed(failed_tracker_shell("failed_tracker_setup"), FixedTools));
    settle(&mut harness);
    harness.get_by_label("Open Setup").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Your applications could not be read."), "the alert stays over every screen");
    assert!(!is_shown(&harness, "Open Setup"), "Setup has no button to itself");
}

/// The mark on a tab is a bare square, so the pointer on the tab says what failed, in the alert's words.
#[test]
fn a_failed_tab_says_what_failed() {
    let name = "failed_gaps_tab";
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    fs::write(root.join("upskill/report-2026-10-29.md"), b"Caf\xe9 \xff\xfe").unwrap();
    let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    assert_eq!(snapshot.issues.len(), 1, "{:?}", snapshot.issues);
    let shell = Shell::with_snapshot(Screen::InTray, jobdesk::workspace::open(&root).unwrap(), snapshot);
    let mut harness = tray_harness(shell);
    assert_eq!(harness.query_all_by_label("Your skill gap reports could not be read.").count(), 1, "the alert says it once");
    harness.get_by_label("Insights, could not be read").hover();
    harness.run_steps(4);
    assert_eq!(harness.query_all_by_label("Your skill gap reports could not be read.").count(), 2, "the tab says it too");
}

/// Labels take no click, except where the user may copy: the alert's reason can be selected, a heading cannot.
#[test]
fn a_failed_sources_reason_can_be_selected() {
    let harness = tray_harness(failed_tracker_shell("failed_selectable"));
    let senses_click = |label: &str| harness.get_by_label(label).accesskit_node().data().supports_action(egui::accesskit::Action::Click);
    assert!(senses_click("job_search_tracker.csv: not UTF-8 text, re-save it as CSV UTF-8"));
    assert!(!senses_click("Your applications could not be read."));
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
    assert_eq!(harness.query_all_by_label("Open Runtime status").count(), 1);
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
const EVALUATION: &str =
    "**Overall fit: 78, Strong Fit.**\n\n**Skills match:** Java, Kafka and event sourcing match.\n\n**Experience match:** Public sector work maps well.";

fn dossier_shell(name: &str) -> Shell {
    let mut shell = shell(Screen::Tracker, name);
    let sink = SharedSink::default();
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
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
    let mut harness =
        with_renderer(Harness::builder().with_size([1280.0, 820.0]).with_pixels_per_point(1.5).with_theme(theme).with_context_setup(install_look))
            .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    if let Some(section) = section {
        harness.get_by_label_contains(section).click();
    }
    for _ in 0..40 {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    compare(&mut harness, name);
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
fn render_probed(shell: Shell, theme: Theme, name: &str) {
    compare(&mut probed_wgpu(probed(shell, FixedTools), theme), name);
}

/// A rendering harness on `shell`, stepped until its tool check is back.
fn probed_wgpu(mut shell: Shell, theme: Theme) -> Harness<'static> {
    let mut harness =
        with_renderer(Harness::builder().with_size([1280.0, 820.0]).with_pixels_per_point(1.5).with_theme(theme).with_context_setup(install_look))
            .build_ui(move |ui| shell.show(ui));
    settle(&mut harness);
    harness
}

/// The fictional data folder's sizes, as the canvas shows them: 212 MB in all.
const DATA_SIZES: DataSizes = DataSizes { run_logs: 211_800_000, decisions: 18_000 };

/// A shell as Settings' snapshots show it: fictional folders and a fixed version, so no temporary path or release shows.
/// Runtime status draws the portals too, so it hides them while their folder cannot be read and never says none are installed.
#[test]
fn runtime_status_hides_portals_it_cannot_read() {
    let mut shell = probed(broken_shell(Screen::InTray, "runtime_portals_unread", &[Source::Portals]), FixedTools);
    shell.open_settings(Page::Runtime);
    let mut harness = tray_harness(shell);
    settle(&mut harness);
    assert!(harness.query_by_label_contains("Back to app").is_some(), "Settings is open on Runtime status");
    assert!(harness.query_by_label_contains("Open Portals").is_some(), "the portals section is drawn");
    assert!(is_shown(&harness, PORTALS_HIDDEN));
    assert!(harness.query_by_label_contains("No portals installed.").is_none());
    assert!(harness.query_by_label_contains("portals on").is_none());
}

fn labelled(shell: Shell) -> Shell {
    shell.with_folder_label("~/Documents/ai-job-search").with_data_label("~/Library/Application Support/Jobdesk", DATA_SIZES).with_version_label("0.1.0")
}

/// Settings over Today, on `page`, with bun missing so Runtime status says "1 missing". `query` is typed into the search.
fn render_settings(page: Page, query: Option<&str>, theme: Theme, name: &str) {
    render_settings_on(shell(Screen::InTray, name), page, query, theme, name);
}

/// `render_settings` over `shell`, such as one whose settings or folder a test changed.
fn render_settings_on(shell: Shell, page: Page, query: Option<&str>, theme: Theme, name: &str) {
    let mut shell = labelled(shell);
    shell.open_settings(page);
    let mut harness = probed_wgpu(probed(shell, FixedTools), theme);
    if let Some(query) = query {
        search(&harness).focus();
        search(&harness).type_text(query);
        harness.run_steps(3);
    }
    compare(&mut harness, name);
}

#[test]
fn settings_general_light() {
    render_settings(Page::General, None, Theme::Light, "settings_general_light");
}

#[test]
fn settings_general_dark() {
    render_settings(Page::General, None, Theme::Dark, "settings_general_dark");
}

#[test]
fn settings_appearance_light() {
    render_settings(Page::Appearance, None, Theme::Light, "settings_appearance_light");
}

#[test]
fn settings_runtime_light() {
    render_settings(Page::Runtime, None, Theme::Light, "settings_runtime_light");
}

#[test]
fn settings_runtime_dark() {
    render_settings(Page::Runtime, None, Theme::Dark, "settings_runtime_dark");
}

#[test]
fn settings_search_light() {
    render_settings(Page::General, Some("folder"), Theme::Light, "settings_search_light");
}

/// The sample folder over Today, with `change` made to it before it is read. A file it breaks is an issue the shell reports.
fn changed_shell(name: &str, change: impl FnOnce(&Path)) -> Shell {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    change(&root);
    let snapshot = Snapshot { usage_read: true, ..Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()) };
    Shell::with_snapshot(Screen::InTray, jobdesk::workspace::open(&root).unwrap(), snapshot)
}

#[test]
fn settings_terms_light() {
    render_settings(Page::Terms, None, Theme::Light, "settings_terms_light");
}

/// The sample profile with its floor taken out: the floor is not stated.
#[test]
fn settings_terms_missing_light() {
    let name = "settings_terms_missing_light";
    let shell = changed_shell(name, |root| {
        let profile = fs::read_to_string(root.join(CANDIDATE)).unwrap();
        assert!(profile.contains("Freelance only, day-rate floor EUR 700."));
        fs::write(root.join(CANDIDATE), profile.replace("Freelance only, day-rate floor EUR 700.", "Freelance only.")).unwrap();
    });
    render_settings_on(shell, Page::Terms, None, Theme::Light, name);
}

/// The profile file is not UTF-8: the alert, hidden not gone, the commute faded, and Update with Claude… waiting, with
/// its reason on hover.
#[test]
fn settings_terms_failed_light() {
    let name = "settings_terms_failed_light";
    let shell = changed_shell(name, |root| fs::write(root.join(CANDIDATE), b"# Candidate Profile\n\nCaf\xe9\n").unwrap());
    let mut shell = labelled(shell);
    shell.open_settings(Page::Terms);
    let mut harness = probed_wgpu(probed(shell, FixedTools), Theme::Light);
    harness.get_by_label("Update with Claude…").hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "Fix the file first, so Claude does not write over it."));
    compare(&mut harness, name);
}

#[test]
fn settings_queue_light() {
    render_settings(Page::Queue, None, Theme::Light, "settings_queue_light");
}

#[test]
fn settings_queue_dark() {
    render_settings(Page::Queue, None, Theme::Dark, "settings_queue_dark");
}

/// Day rate under your floor and the score filter on, with the count of what they hide.
#[test]
fn settings_jobs_view_light() {
    let name = "settings_jobs_view_light";
    let mut shell = shell(Screen::InTray, name);
    shell.settings_mut().set_hide_below_floor(true);
    shell.settings_mut().set_hide_under_score(true);
    render_settings_on(shell, Page::JobsView, None, Theme::Light, name);
}

/// Jobs with the same filters on: the lists leave out what they hide, and say how many.
#[test]
fn jobs_filtered_light() {
    let name = "jobs_filtered_light";
    let mut shell = shell(Screen::Jobs, name);
    shell.settings_mut().set_hide_below_floor(true);
    shell.settings_mut().set_hide_under_score(true);
    render_with(shell, Theme::Light, name);
}

/// The tasks each day of October ran, as the canvas draws them.
const OCTOBER: [usize; 31] = [2, 1, 0, 0, 3, 1, 2, 0, 1, 0, 1, 2, 4, 1, 0, 1, 0, 0, 2, 1, 3, 2, 0, 0, 0, 2, 1, 2, 1, 2, 2];

/// Each model's tasks in October, with the tokens in and out they used between them, as the canvas's By model shows.
const MODEL_TOKENS: [(&str, u64, u64, u64); 3] =
    [("claude-opus-4-5", 14, 2_200_000, 310_000), ("claude-sonnet-4-5", 15, 1_500_000, 240_000), ("claude-haiku-4-5", 8, 400_000, 50_000)];

/// One fictional task: its slash command, model, cost and how it ended. Each model's tokens split evenly, 82% of them cached.
fn usage_task(on: NaiveDate, (command, model, cost_usd, outcome): (&str, &str, f64, TaskOutcome)) -> TaskCost {
    let &(_, tasks, tokens_in, tokens_out) = MODEL_TOKENS.iter().find(|(id, ..)| *id == model).unwrap();
    let tokens_in = tokens_in / tasks;
    let cost = ModelCost { model: model.into(), tokens_in, tokens_out: tokens_out / tasks, cache_read: tokens_in * 82 / 100, cost_usd };
    TaskCost { started_on: on, command: Some(command.into()), outcome, cost_usd, models: vec![cost] }
}

/// Fictional tasks from 13 July to 31 October 2026. October matches the canvas: $41.20 over 37 tasks, the commands and
/// models of its tables, and Tue 13 Oct at 4 tasks for $6.10. The weeks before follow the canvas's pattern of days.
fn usage_tasks() -> Vec<TaskCost> {
    use TaskOutcome::{Failed, Finished, Stopped};
    const OPUS: &str = "claude-opus-4-5";
    const SONNET: &str = "claude-sonnet-4-5";
    const HAIKU: &str = "claude-haiku-4-5";
    let the_13th =
        [("/apply", OPUS, 3.30, Finished), ("/interview", OPUS, 1.45, Finished), ("/rank", OPUS, 1.20, Finished), ("/gmail-sync", HAIKU, 0.15, Finished)];
    let mut rest = vec![
        ("/scrape", SONNET, 0.40, Finished),
        ("/rank", SONNET, 0.75, Finished),
        ("/apply", OPUS, 3.30, Finished),
        ("/outcome", HAIKU, 0.05, Finished),
        ("/setup", OPUS, 1.80, Finished),
        ("/scrape", SONNET, 0.40, Stopped),
        ("/rank", SONNET, 0.75, Finished),
        ("/outcome", HAIKU, 0.10, Finished),
        ("/apply", OPUS, 3.30, Finished),
        ("/scrape", SONNET, 0.40, Finished),
        ("/setup", OPUS, 1.80, Finished),
        ("/rank", OPUS, 1.20, Finished),
        ("/outcome", SONNET, 1.20, Finished),
        ("/scrape", SONNET, 0.40, Finished),
        ("/apply", OPUS, 3.30, Failed),
        ("/rank", SONNET, 0.75, Finished),
        ("/outcome", HAIKU, 0.05, Finished),
        ("/scrape", SONNET, 0.40, Stopped),
        ("/interview", OPUS, 1.45, Finished),
        ("/rank", SONNET, 0.75, Finished),
        ("/gmail-sync", HAIKU, 0.15, Finished),
        ("/apply", OPUS, 3.30, Finished),
        ("/scrape", SONNET, 0.40, Finished),
        ("/outcome", HAIKU, 0.10, Finished),
        ("/setup", OPUS, 1.70, Stopped),
        ("/rank", SONNET, 0.75, Finished),
        ("/scrape", SONNET, 0.40, Finished),
        ("/rank", OPUS, 1.20, Finished),
        ("/outcome", HAIKU, 0.05, Finished),
        ("/apply", OPUS, 3.30, Finished),
        ("/rank", SONNET, 0.75, Finished),
        ("/scrape", SONNET, 0.40, Finished),
        ("/outcome", HAIKU, 0.05, Finished),
    ]
    .into_iter();
    let mut tasks = Vec::new();
    for (at, &count) in OCTOBER.iter().enumerate() {
        let on = NaiveDate::from_ymd_opt(2026, 10, at as u32 + 1).unwrap();
        if at == 12 {
            tasks.extend(the_13th.map(|task| usage_task(on, task)));
        } else {
            tasks.extend(rest.by_ref().take(count).map(|task| usage_task(on, task)));
        }
    }
    assert!(rest.next().is_none(), "every October task has a day");
    let earlier = [("/scrape", SONNET, 0.45), ("/rank", SONNET, 0.70), ("/apply", OPUS, 2.90), ("/outcome", HAIKU, 0.10), ("/setup", OPUS, 1.60)];
    let mut at = 0;
    let mut on = NaiveDate::from_ymd_opt(2026, 7, 13).unwrap();
    while on.month() < 10 {
        // The canvas's own pattern: weekdays run 0 to 4 tasks, weekends one at most.
        let seed = (u64::from(on.day()) * 7 + u64::from(on.month0()) * 13) * 2_654_435_761 % 97;
        let count = match () {
            _ if on.weekday().num_days_from_monday() >= 5 => usize::from(seed > 77),
            _ if seed > 87 => 4,
            _ if seed > 60 => 2,
            _ if seed > 29 => 1,
            _ => 0,
        };
        for _ in 0..count {
            let (command, model, cost) = earlier[at % earlier.len()];
            at += 1;
            tasks.push(usage_task(on, (command, model, cost, Finished)));
        }
        on = on.succ_opt().unwrap();
    }
    tasks
}

/// The sample folder over Today, with `tasks` as the run logs read.
fn shell_with_usage(name: &str, tasks: Vec<TaskCost>) -> Shell {
    usage_shell_on(Screen::InTray, name, tasks)
}

fn usage_shell_on(screen: Screen, name: &str, tasks: Vec<TaskCost>) -> Shell {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    labelled(Shell::with_snapshot(screen, jobdesk::workspace::open(&root).unwrap(), usage_snapshot(&root, tasks)))
}

/// The sample folder at `root` as read on 31 October, with `tasks` as the run logs read.
fn usage_snapshot(root: &Path, tasks: Vec<TaskCost>) -> Snapshot {
    let mut snapshot = Snapshot::load(root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    snapshot.usage = UsageReport::new(tasks);
    snapshot.usage_read = true;
    snapshot
}

/// The checkout can show before the run logs are read. Until they are, Usage statistics says it is reading them.
#[test]
fn usage_reads_until_the_logs_are_read() {
    let name = "settings_usage_unread";
    let mut shell = shell_with_usage(name, usage_tasks());
    let feed = shell.snapshot_feed();
    shell.open_settings(Page::Usage);
    let mut harness = tray_harness(shell);
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let unread = Snapshot { usage_read: false, usage: UsageReport::default(), ..usage_snapshot(&root, Vec::new()) };
    feed.send(unread).unwrap();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Reading your run logs…"));
    assert!(!is_shown(&harness, "No tasks yet"), "an unread log is not an empty one");

    feed.send(usage_snapshot(&root, usage_tasks())).unwrap();
    harness.run_steps(3);
    assert!(!is_shown(&harness, "Reading your run logs…"));
    assert!(is_shown(&harness, "$41.20 in Oct"));
}

/// A command read from a run log keeps its one slash in By command.
#[test]
fn a_logged_command_shows_with_one_slash() {
    let logs = tempfile::tempdir().unwrap();
    let name = "settings_usage_logged";
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    let workspace = jobdesk::workspace::open(&root).unwrap();
    let meta = serde_json::json!({
        "type": "jobdesk_meta", "workspace": workspace.path().display().to_string(), "id": "a", "line": "/rank",
        "model": null, "budget_usd": 2.0, "started_at": "2026-10-30 10:00",
    });
    let result = r#"{"type":"result","subtype":"success","total_cost_usd":0.5,"duration_ms":3}"#;
    fs::write(logs.path().join("a.jsonl"), format!("{meta}\n{result}\n")).unwrap();
    let tasks = jobdesk::runs::usage_logs(logs.path().to_owned(), workspace.path()).tasks();
    assert_eq!(tasks.len(), 1, "the log is read");

    let mut shell = Shell::with_snapshot(Screen::InTray, workspace, usage_snapshot(&root, tasks));
    shell.open_settings(Page::Usage);
    let harness = tray_harness(shell);
    assert!(is_shown(&harness, "/rank"));
    assert!(!is_shown(&harness, "//rank"));
}

/// Usage statistics on `shell`, with bun missing so Runtime status says "1 missing". `hover` rests the pointer on one
/// day of the year, given as its week and weekday.
fn render_usage(mut shell: Shell, hover: Option<(usize, usize)>, theme: Theme, name: &str) {
    shell.open_settings(Page::Usage);
    let mut harness = probed_wgpu(probed(shell, FixedTools), theme);
    if let Some((week, weekday)) = hover {
        let map = harness.get_by_label("Tasks per day over the last 12 months").rect();
        let pitch = STATUS_DOT + TALLY_GAP_Y;
        let first = map.min + egui::vec2(HEAT_LABELS.0 + TALLY_GAP_Y, HEAT_LABELS.1 + TALLY_GAP_Y) + egui::Vec2::splat(STATUS_DOT / 2.0);
        harness.hover_at(first + egui::vec2(week as f32, weekday as f32) * pitch);
        harness.run_steps(4);
    }
    compare(&mut harness, name);
}

/// Tue 13 Oct: three weeks before today's column, the second day.
const THE_13TH: (usize, usize) = (50, 1);

#[test]
fn settings_usage_light() {
    render_usage(shell_with_usage("settings_usage_light", usage_tasks()), None, Theme::Light, "settings_usage_light");
}

#[test]
fn settings_usage_dark() {
    render_usage(shell_with_usage("settings_usage_dark", usage_tasks()), Some(THE_13TH), Theme::Dark, "settings_usage_dark");
}

#[test]
fn settings_usage_empty_light() {
    render_usage(shell_with_usage("settings_usage_empty_light", Vec::new()), None, Theme::Light, "settings_usage_empty_light");
}

#[test]
fn settings_loading_light() {
    let shell = labelled(loading_on(Screen::InTray, "settings_loading_light"));
    render_usage(shell, None, Theme::Light, "settings_loading_light");
}

/// The fixture matches the canvas: October's figures, both tables' rows, and the day the tooltip shows.
#[test]
fn the_usage_fixture_matches_the_canvas() {
    let mut shell = shell_with_usage("settings_usage_fixture", usage_tasks());
    shell.open_settings(Page::Usage);
    let harness = tray_harness(shell);
    // A count such as "2" shows in more than one row, so any match will do.
    let shown = |text: &str| harness.query_all_by_label(text).next().is_some();
    for text in ["$41.20 in Oct", "$41.20", "4.1M", "in, and 600K out", "82%", "37", "33 finished, 3 stopped, 1 failed"] {
        assert!(shown(text), "{text}");
    }
    for (command, tasks, spent) in [("/apply", "6", "$19.80"), ("/rank", "9", "$8.10"), ("/setup", "3", "$5.30"), ("/gmail-sync", "2", "$0.30")] {
        assert!(shown(command) && shown(tasks) && shown(spent), "{command}");
    }
    for (model, spent) in [("Opus", "$31.60"), ("Sonnet", "$8.90"), ("Haiku", "$0.70")] {
        assert!(shown(model) && shown(spent), "{model}");
    }
    let the_13th: Vec<TaskCost> = usage_tasks().into_iter().filter(|task| task.started_on == NaiveDate::from_ymd_opt(2026, 10, 13).unwrap()).collect();
    assert_eq!(the_13th.len(), 4);
    assert!((the_13th.iter().map(|task| task.cost_usd).sum::<f64>() - 6.10).abs() < 1e-9);
}

/// Open Applications closes Settings on Applications, and New task… on the launcher.
#[test]
fn usage_links_leave_settings() {
    let mut applications = shell_with_usage("settings_usage_links", usage_tasks());
    applications.open_settings(Page::Usage);
    let (mut harness, shell) = shared_harness(applications);
    harness.get_by_label("Open Applications").click();
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open());
    assert!(is_shown(&harness, "Record a result…"), "Applications is on show");

    let mut launcher = shell_with_usage("settings_usage_new_task", Vec::new());
    *launcher.desk_slot() = Some(desk_on(ScriptedGateway(SharedSink::default())));
    launcher.open_settings(Page::Usage);
    let (mut harness, shell) = shared_harness(launcher);
    harness.get_by_label("New task…").click();
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open());
    assert!(is_shown(&harness, LAUNCHER), "the launcher is open");
}

const CAP_LINE: &str = "This month's tasks reached your $60.00 cap.";
const CAP_REASON: &str = "This month's tasks cost $60.40, over your $60.00 cap. Run opens again on 1\u{a0}November.";
const CAP_LINK: &str = "Change the cap in Settings";

/// October's fixture with one more fictional /apply on the 30th, so the month comes to $60.40.
fn over_the_cap() -> Vec<TaskCost> {
    let mut tasks = usage_tasks();
    tasks.push(usage_task(NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(), ("/apply", "claude-opus-4-5", 19.20, TaskOutcome::Finished)));
    tasks
}

/// The sample folder on `screen` on 31 October, with `tasks` as the run logs read and a $60 monthly cap.
/// Its desk records what it starts and sends into `calls`.
fn capped_shell(screen: Screen, name: &str, tasks: Vec<TaskCost>, calls: &SharedCalls) -> Shell {
    let mut shell = usage_shell_on(screen, name, tasks);
    *shell.desk_slot() = Some(desk_on(RecordingGateway(SharedSink::default(), Arc::clone(calls))));
    shell.settings_mut().set_cap_on(true);
    shell.settings_mut().set_cap_amount(Dollars::parse("60"));
    shell
}

fn catalog(name: &str) -> &'static CommandSpec {
    CATALOG.iter().find(|spec| spec.name == name).unwrap()
}

/// A command's own row in Run defaults.
fn command_row(settings: &mut Settings, command: &str, model: ModelChoice, budget: &str) {
    settings.add_command(catalog(command));
    settings.set_command_model(command, model);
    settings.set_command_budget(command, Dollars::parse(budget).unwrap());
}

fn budget_field<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
    harness.get_by(|node| node.role() == egui::accesskit::Role::TextInput && node.label().as_deref() == Some("Spend at most, USD"))
}

fn model_box<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
    harness.get_by(|node| node.role() == egui::accesskit::Role::ComboBox && node.label().as_deref() == Some("Model"))
}

/// Clicks the command's row in the list, whose label is its name, a space and its summary.
fn pick_command(harness: &mut Harness, command: &str) {
    let prefix = format!("{command} ");
    harness.get_by(|node| node.role() == egui::accesskit::Role::Button && node.label().is_some_and(|label| label.starts_with(&prefix))).click();
    harness.run_steps(3);
}

/// At the cap, Run is disabled and says why, Cmd Enter starts nothing, and the strong line and the link show.
/// Under it, the same keys start the task.
#[test]
fn cmd_enter_starts_nothing_over_the_cap() {
    let calls = SharedCalls::default();
    let mut harness = tray_harness(capped_shell(Screen::Launcher, "launcher_over_cap", over_the_cap(), &calls));
    assert!(is_shown(&harness, CAP_LINE));
    assert!(is_shown(&harness, CAP_LINK));
    let run = harness.get_by_label("Run");
    assert!(run.accesskit_node().is_disabled());
    run.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, CAP_REASON), "the pointer on Run says why");
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 0, "the cap holds the task back");
    assert!(is_shown(&harness, LAUNCHER));

    let calls = SharedCalls::default();
    let mut harness = tray_harness(capped_shell(Screen::Launcher, "launcher_under_cap", usage_tasks(), &calls));
    assert!(!is_shown(&harness, CAP_LINE) && !is_shown(&harness, CAP_LINK), "$41.20 is under the cap");
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 1);
}

/// While the run logs are read, the month's spend is unknown, so Run waits without claiming the cap is reached.
#[test]
fn run_waits_while_the_months_spend_is_read() {
    let name = "launcher_cap_reading";
    let calls = SharedCalls::default();
    let mut shell = capped_shell(Screen::Launcher, name, over_the_cap(), &calls);
    let feed = shell.snapshot_feed();
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    feed.send(Snapshot { usage_read: false, usage: UsageReport::default(), ..usage_snapshot(&root, Vec::new()) }).unwrap();
    let mut harness = tray_harness(shell);
    assert!(!is_shown(&harness, CAP_LINE) && !is_shown(&harness, CAP_LINK));
    let run = harness.get_by_label("Run");
    assert!(run.accesskit_node().is_disabled());
    run.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "Run waits while this month's spend is read."));
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 0);

    feed.send(usage_snapshot(&root, usage_tasks())).unwrap();
    harness.run_steps(3);
    assert!(!harness.get_by_label("Run").accesskit_node().is_disabled(), "under the cap once read");
}

/// A task that ends leaves its cost in a log the snapshot has not read yet. Until a snapshot read after its end arrives,
/// the month's spend is unknown, so Run waits and Cmd Enter starts nothing, even when the old totals are under the cap.
#[test]
fn run_waits_for_the_spend_of_a_task_that_ended() {
    let name = "launcher_cap_after_a_task";
    let calls = SharedCalls::default();
    let sink = SharedSink::default();
    let mut shell = usage_shell_on(Screen::Launcher, name, usage_tasks());
    let mut desk = desk_on(RecordingGateway(Arc::clone(&sink), Arc::clone(&calls)));
    desk.start(APPLY_LINE, &StartOptions { model: None, budget_usd: 5.0 }, run_started()).unwrap();
    *shell.desk_slot() = Some(desk);
    shell.settings_mut().set_cap_on(true);
    shell.settings_mut().set_cap_amount(Dollars::parse("60"));
    let reloads = shell.reload_feed();
    let feed = shell.snapshot_feed();
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let mut harness = tray_harness(shell);

    emit_all(&sink, [AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 19.20, duration_ms: 9_000 }]);
    harness.run_steps(4);
    assert!(reloads.try_recv().is_ok(), "the end of the task asks for a reload");
    let held = |harness: &mut Harness<'static>| {
        let run = harness.get_by_label("Run");
        assert!(run.accesskit_node().is_disabled(), "the $41.20 read before the task no longer holds");
        run.hover();
        harness.run_steps(8);
        assert!(is_shown(harness, "Run waits while this month's spend is read."));
        assert!(!is_shown(harness, CAP_LINE), "nothing claims the cap is reached yet");
        cmd_enter(harness);
        assert_eq!(calls.lock().unwrap().starts, 1, "only the task that ended ever started");
    };
    held(&mut harness);

    // A load already under way when the task ended still holds the old totals.
    feed.send(usage_snapshot(&root, usage_tasks())).unwrap();
    harness.run_steps(3);
    held(&mut harness);

    feed.send(Snapshot { usage_asked: 1, ..usage_snapshot(&root, over_the_cap()) }).unwrap();
    harness.run_steps(3);
    assert!(is_shown(&harness, CAP_LINE), "the read after the task shows the cap reached");
    assert!(harness.get_by_label("Run").accesskit_node().is_disabled());
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 1);
}

/// The cap holds back only new tasks: a reply to a live session continues a task already paid for.
#[test]
fn a_reached_cap_leaves_a_live_session_answering() {
    let calls = SharedCalls::default();
    let mut shell = capped_shell(Screen::Runs, "quick_reply_over_cap", over_the_cap(), &calls);
    *shell.desk_slot() = Some(stop_with(Arc::clone(&calls), STOP_EVALUATION).0);
    let mut harness = tray_harness(shell);
    harness.get_by_label("Go ahead with the draft").click();
    harness.run_steps(3);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.sent, [GO_AHEAD], "the reply reached the live session");
    assert_eq!(calls.starts, 1, "and started no other process");
}

/// The launcher fills the model and budget from Run defaults: a command's row first, then every task's, then the catalog.
/// What the user changes stays until another command is picked.
#[test]
fn the_launcher_starts_from_run_defaults() {
    let start = |name: &str, command: &str, set: &dyn Fn(&mut Settings)| {
        let calls = SharedCalls::default();
        let mut shell = recording_shell(Screen::Launcher, name, &calls);
        set(shell.settings_mut());
        let mut harness = tray_harness(shell);
        pick_command(&mut harness, command);
        harness.get_by_label("Run").click();
        harness.run_steps(3);
        calls.lock().unwrap().asked.pop().unwrap()
    };
    let defaults = |settings: &mut Settings| {
        settings.set_model(ModelChoice::Opus);
        settings.set_budget(Dollars::parse("4.00"));
        command_row(settings, "/rank", ModelChoice::Haiku, "1.00");
    };
    assert_eq!(start("launcher_defaults_rank", "/rank", &defaults), (Some("haiku".to_owned()), 1.0));
    assert_eq!(start("launcher_defaults_scrape", "/scrape", &defaults), (Some("opus".to_owned()), 4.0));
    let suggested = f64::from(catalog("/rank").budget_usd);
    assert_eq!(start("launcher_no_defaults", "/rank", &|_| {}), (None, suggested));

    let mut shell = recording_shell(Screen::Launcher, "launcher_defaults_edit", &SharedCalls::default());
    defaults(shell.settings_mut());
    let mut harness = tray_harness(shell);
    pick_command(&mut harness, "/rank");
    assert_eq!((budget_field(&harness).value().as_deref(), model_box(&harness).value().as_deref()), (Some("1.00"), Some("Haiku")));
    budget_field(&harness).focus();
    harness.run_steps(2);
    harness.key_press_modifiers(COMMAND, Key::A);
    budget_field(&harness).type_text("7");
    harness.run_steps(2);
    assert_eq!(budget_field(&harness).value().as_deref(), Some("7"), "an edit stays");
    pick_command(&mut harness, "/scrape");
    assert_eq!(budget_field(&harness).value().as_deref(), Some("4.00"), "a new command starts from its defaults");
}

/// The link beside a held Run opens Settings on Run defaults, which keeps the launcher underneath for Back to app.
#[test]
fn the_cap_link_opens_run_defaults() {
    let (mut harness, shell) = shared_harness(capped_shell(Screen::Launcher, "launcher_cap_link", over_the_cap(), &SharedCalls::default()));
    harness.get_by_label(CAP_LINK).click();
    harness.run_steps(3);
    assert!(shell.borrow().settings_open());
    assert!(is_shown(&harness, Page::Defaults.lead()), "on Run defaults");
    assert!(is_shown(&harness, "Cap reached"), "and the rail says why Run stopped");
    press(&mut harness, Key::Escape);
    assert!(!shell.borrow().settings_open());
    assert!(is_shown(&harness, LAUNCHER), "back on the launcher");
}

/// The launcher at the cap, on /rank with its own Sonnet and $3.00, with the pointer on Run.
#[test]
fn launcher_cap_light() {
    let name = "launcher_cap_light";
    let mut shell = capped_shell(Screen::Launcher, name, over_the_cap(), &SharedCalls::default());
    command_row(shell.settings_mut(), "/rank", ModelChoice::Sonnet, "3.00");
    let mut harness = wgpu_tray(shell, Theme::Light);
    pick_command(&mut harness, "/rank");
    harness.get_by_label("Run").hover();
    harness.run_steps(8);
    compare(&mut harness, name);
}

/// A task in plain words, as a screen of Jobdesk writes one. The address is fictional.
const WORDS: &str = "Copy https://github.example/sample/portals/tree/main/sample-board into `.agents/skills/`. Read its code and tell me what it does. Then ask me before you run `python3 tools/install_portal_clis.py`.";
const WORDS_TITLE: &str = "Task in plain words";
/// The launcher's subtitle on a task in plain words, which has no command to pick.
const WORDS_LAUNCHER: &str = "Read the task. Nothing runs until you press Run.";

/// Leaves `line` for the shell as a command action does, under the id `ui_kit` keeps for it, and lets the shell take it.
fn request_line(harness: &mut Harness, line: &str) {
    harness.ctx.data_mut(|data| data.insert_temp(egui::Id::NULL.with("requested_command"), line.to_owned()));
    harness.run_steps(3);
}

/// A line in plain words opens the launcher on it and starts nothing. Run sends it as it stands, on the defaults for
/// every task, and Usage counts it with no command.
#[test]
fn a_task_in_plain_words_opens_the_launcher_and_runs_as_written() {
    let calls = SharedCalls::default();
    let mut harness = tray_harness(recording_shell(Screen::Insights, "launcher_words", &calls));
    request_line(&mut harness, WORDS);
    assert!(is_shown(&harness, WORDS_LAUNCHER), "the launcher opens, asking to read the task");
    assert!(!is_shown(&harness, LAUNCHER));
    assert!(is_shown(&harness, WORDS_TITLE));
    assert!(is_shown(&harness, WORDS), "the task shows as Claude will get it");
    assert!(harness.query_by_role(egui::accesskit::Role::MultilineTextInput).is_none(), "the task is read only");
    assert_eq!(calls.lock().unwrap().starts, 0, "nothing runs until Run");

    harness.get_by_label("Run").click();
    harness.run_steps(3);
    let suggested = f64::from(jobdesk::runs::application::WORDS_BUDGET_USD);
    assert_eq!(calls.lock().unwrap().asked, [(None, suggested)]);
    let request = calls.lock().unwrap().requests[0].clone();
    assert_eq!(request.line, WORDS, "the process gets the task as written");

    // The meta line a run log opens with, as the gateway writes it from the request the process started from.
    // Its line is all Usage reads to name the command.
    let logs = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("launcher_words").join("ai-job-search");
    let workspace = jobdesk::workspace::open(&root).unwrap();
    let meta = serde_json::json!({
        "type": "jobdesk_meta", "workspace": workspace.path().display().to_string(), "id": request.run_id, "line": request.line,
        "model": request.model, "budget_usd": request.budget_usd, "started_at": request.started_at,
    });
    let result = r#"{"type":"result","subtype":"success","total_cost_usd":0.5,"duration_ms":3}"#;
    fs::write(logs.path().join(format!("{}.jsonl", request.run_id)), format!("{meta}\n{result}\n")).unwrap();
    let tasks = jobdesk::runs::usage_logs(logs.path().to_owned(), workspace.path()).tasks();
    assert_eq!(tasks.iter().map(|task| task.command.clone()).collect::<Vec<_>>(), [None]);
}

/// One run at a time: while another run works, Run on a task in plain words waits and says why, and Cmd Enter
/// starts nothing either.
#[test]
fn a_task_in_plain_words_waits_while_another_run_works() {
    let calls = SharedCalls::default();
    let mut shell = recording_shell(Screen::Insights, "launcher_words_busy", &calls);
    shell.desk_slot().as_mut().unwrap().start("/rank", &StartOptions { model: None, budget_usd: 1.0 }, run_started()).unwrap();
    let mut harness = tray_harness(shell);
    request_line(&mut harness, WORDS);
    assert!(is_shown(&harness, WORDS_TITLE));
    assert!(is_shown(&harness, "Another run is active. One run at a time keeps your files safe."));
    let run = harness.get_by_label("Run");
    assert!(run.accesskit_node().is_disabled());
    run.click();
    harness.run_steps(3);
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 1, "only the run already working");
    assert!(is_shown(&harness, WORDS_TITLE), "the task stays to run later");
}

/// Run defaults for every task fill a task in plain words, and a command's own row does not.
/// Picking a command clears the task, and Run then sends the command.
#[test]
fn a_picked_command_replaces_a_task_in_plain_words() {
    let calls = SharedCalls::default();
    let mut shell = recording_shell(Screen::Insights, "launcher_words_pick", &calls);
    shell.settings_mut().set_model(ModelChoice::Opus);
    shell.settings_mut().set_budget(Dollars::parse("4.00"));
    command_row(shell.settings_mut(), "/rank", ModelChoice::Haiku, "1.00");
    let (mut harness, shell) = shared_harness(shell);
    request_line(&mut harness, WORDS);
    assert_eq!((budget_field(&harness).value().as_deref(), model_box(&harness).value().as_deref()), (Some("4.00"), Some("Opus")));

    pick_command(&mut harness, "/rank");
    assert!(!is_shown(&harness, WORDS_TITLE));
    assert!(!is_shown(&harness, WORDS));
    harness.get_by_label("Run").click();
    harness.run_steps(3);
    let line = shell.borrow_mut().desk_slot().as_ref().unwrap().runs()[0].line.clone();
    assert!(line.starts_with("/rank"), "{line}");
    assert_eq!(calls.lock().unwrap().asked, [(Some("haiku".to_owned()), 1.0)]);
}

/// A `/` line the catalog does not hold leaves the form as it was.
#[test]
fn an_unknown_command_leaves_the_task_in_plain_words() {
    let calls = SharedCalls::default();
    let mut harness = tray_harness(recording_shell(Screen::Insights, "launcher_words_unknown", &calls));
    request_line(&mut harness, WORDS);
    request_line(&mut harness, "/unknown now");
    assert!(is_shown(&harness, WORDS));
    assert!(!is_shown(&harness, "/unknown now"));
    assert_eq!(calls.lock().unwrap().starts, 0);
}

/// A task in plain words lasts until it runs, or until the launcher opens with no line, as New task, Cmd K, the Tasks
/// screen's New task and Settings' New task open it. The form is fresh then, so Cmd Enter never repeats the task.
#[test]
fn a_task_in_plain_words_is_dropped_once_it_runs_or_the_launcher_opens_bare() {
    let calls = SharedCalls::default();
    let mut harness = tray_harness(recording_shell(Screen::Insights, "launcher_words_once", &calls));
    request_line(&mut harness, WORDS);
    assert!(is_shown(&harness, WORDS_TITLE));
    press(&mut harness, Key::Escape);
    assert!(!is_shown(&harness, WORDS_TITLE), "Esc leaves the launcher");
    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);
    assert!(is_shown(&harness, LAUNCHER), "Cmd K opens the launcher");
    assert!(!is_shown(&harness, WORDS_TITLE), "on a fresh form");
    assert!(!is_shown(&harness, WORDS));

    request_line(&mut harness, WORDS);
    assert!(is_shown(&harness, WORDS_TITLE));
    harness.get_by(|node| node.role() == egui::accesskit::Role::Button && node.label().as_deref() == Some("New task")).click();
    harness.run_steps(3);
    assert!(is_shown(&harness, LAUNCHER), "the New task button opens the launcher");
    assert!(!is_shown(&harness, WORDS_TITLE), "on a fresh form");
    assert!(!is_shown(&harness, WORDS));

    request_line(&mut harness, WORDS);
    harness.get_by_label("Run").click();
    harness.run_steps(3);
    assert_eq!(calls.lock().unwrap().starts, 1);
    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);
    assert!(is_shown(&harness, LAUNCHER), "Cmd K opens the launcher");
    assert!(!is_shown(&harness, WORDS_TITLE), "the task that ran is gone");
    assert!(!is_shown(&harness, WORDS));
}

/// A task in plain words that holds characters the screen would hide opens no launcher, so what shows is always
/// what Claude reads.
#[test]
fn a_task_with_hidden_characters_opens_no_launcher() {
    let mut harness = tray_harness(recording_shell(Screen::Insights, "launcher_words_hidden", &SharedCalls::default()));
    request_line(&mut harness, "Read the sample portal's code.\u{e0049}\u{e0067}\u{e006e} Then install it.");
    assert!(!is_shown(&harness, LAUNCHER), "no launcher opens");
    assert!(!is_shown(&harness, WORDS_TITLE));
}

/// The launcher on a task in plain words.
#[test]
fn launcher_words_light() {
    let name = "launcher_words_light";
    let mut shell = shell(Screen::Insights, name);
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(SharedSink::default())));
    let mut harness = wgpu_tray(shell, Theme::Light);
    request_line(&mut harness, WORDS);
    compare(&mut harness, name);
}

/// A fictional decisions file in `folder`: `count` skips for the folder at `root`, and one for a folder elsewhere.
fn decisions_file(folder: &Path, root: &Path, count: usize) {
    let record = |at: usize| serde_json::json!({ "key": format!("triage:fictional-{at}"), "name": "Kestrel Labs", "outcome": "skipped", "reason": null, "on": "2026-10-30" });
    let mut file = serde_json::Map::new();
    file.insert(root.display().to_string(), (0..count).map(record).collect());
    file.insert(ELSEWHERE.to_owned(), serde_json::Value::Array(vec![record(0)]));
    fs::create_dir_all(folder).unwrap();
    fs::write(folder.join("decisions.json"), serde_json::Value::Object(file).to_string()).unwrap();
}

/// Another job search folder, whose decisions a clear must keep.
const ELSEWHERE: &str = "/elsewhere/ai-job-search";

/// The sample folder on Settings › Data and privacy, its tray read from a fictional decisions file in `data` with `count`
/// decisions of its own.
fn shell_with_decisions(name: &str, data: &Path, count: usize) -> Shell {
    let mut shell = labelled(shell(Screen::InTray, name));
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let workspace = jobdesk::workspace::open(&root).unwrap();
    decisions_file(&data.join("intray"), workspace.path(), count);
    *shell.tray_slot() = Some(jobdesk::intray::open_tray(data.join("intray"), workspace.path()));
    shell.open_settings(Page::Data);
    shell
}

fn clear_decisions_button<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Clear tray decisions")
}

/// Clear tray decisions asks first. Esc, Keep them and a click elsewhere each keep them, and Settings stays open.
/// Clear decisions forgets the open folder's only, so another folder keeps its own.
#[test]
fn clearing_tray_decisions_asks_first() {
    let data = tempfile::tempdir().unwrap();
    let (mut harness, shell) = shared_harness(shell_with_decisions("settings_clear_decisions", data.path(), 2));
    let count = |shell: &Rc<RefCell<Shell>>| shell.borrow_mut().tray_slot().as_ref().unwrap().decision_count();
    let asking = |harness: &Harness<'static>| harness.query_all_by_label("Clear 2 tray decisions?").next().is_some();
    let open = |harness: &mut Harness<'static>| {
        clear_decisions_button(harness).click();
        harness.run_steps(3);
        assert!(asking(harness), "it asks first");
    };
    assert_eq!(count(&shell), 2);

    open(&mut harness);
    assert!(harness.get_by_label("Keep them").accesskit_node().is_focused(), "Keep them is the default");
    press(&mut harness, Key::Escape);
    assert!(!asking(&harness));
    assert!(shell.borrow().settings_open(), "Esc keeps them, and Settings stays");
    assert_eq!(count(&shell), 2);

    open(&mut harness);
    harness.get_by_label("Keep them").click();
    harness.run_steps(3);
    assert!(!asking(&harness));
    assert_eq!(count(&shell), 2);

    open(&mut harness);
    harness.get_by_label(Page::Data.lead()).click();
    harness.run_steps(3);
    assert!(!asking(&harness), "a click elsewhere keeps them");
    assert_eq!(count(&shell), 2);

    open(&mut harness);
    harness.get_by_label("Clear decisions").click();
    harness.run_steps(3);
    assert_eq!(count(&shell), 0);
    assert!(shell.borrow().settings_open());
    assert!(clear_decisions_button(&harness).accesskit_node().is_disabled(), "nothing is left to clear");
    let file: serde_json::Value = serde_json::from_str(&fs::read_to_string(data.path().join("intray/decisions.json")).unwrap()).unwrap();
    assert_eq!(file[ELSEWHERE].as_array().map(Vec::len), Some(1), "the other folder keeps its decisions");
}

/// Settings left by Cmd K, not Back, closes the popover too, so Cmd , reopens Data and privacy with nothing asking.
#[test]
fn leaving_settings_any_way_closes_the_clear_popover() {
    let data = tempfile::tempdir().unwrap();
    let mut shell = shell_with_decisions("settings_clear_cmd_k", data.path(), 2);
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(SharedSink::default())));
    let (mut harness, shell) = shared_harness(shell);
    let asking = |harness: &Harness<'static>| harness.query_all_by_label("Clear 2 tray decisions?").next().is_some();
    clear_decisions_button(&harness).click();
    harness.run_steps(3);
    assert!(asking(&harness));

    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open());
    assert!(is_shown(&harness, LAUNCHER));

    cmd_comma(&mut harness);
    assert!(shell.borrow().settings_open());
    assert!(is_shown(&harness, Page::Data.lead()), "Settings reopens on Data and privacy");
    assert!(!asking(&harness), "with the popover closed");
    assert_eq!(shell.borrow_mut().tray_slot().as_ref().unwrap().decision_count(), 2);
}

/// Ask me before each tool saves the setting, and the open desk then waits on a permission instead of allowing it.
#[test]
fn ask_me_turns_prompts_on() {
    let sink = SharedSink::default();
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    desk.start(APPLY_LINE, &StartOptions { model: None, budget_usd: 5.0 }, run_started()).unwrap();
    let mut shell = shell(Screen::InTray, "settings_ask_me");
    *shell.desk_slot() = Some(desk);
    shell.open_settings(Page::Permissions);
    let (mut harness, shell) = shared_harness(shell);
    assert!(!shell.borrow_mut().desk_slot().as_ref().unwrap().asks_permission(), "allowed and noted by default");

    harness.get_by_label("Ask me before each tool").click();
    harness.run_steps(3);
    assert_eq!(shell.borrow_mut().settings_mut().preferences().permissions, PermissionMode::Ask);
    assert!(shell.borrow_mut().desk_slot().as_ref().unwrap().asks_permission());
    assert!(is_shown(&harness, "Ask first"), "the rail says so");

    emit_all(&sink, [permission_prompt()]);
    harness.run_steps(3);
    assert_eq!(shell.borrow_mut().desk_slot().as_ref().unwrap().waiting_count(), 1, "the task waits on the permission");
    assert!(is_shown(&harness, "A task waits for you"));
}

/// Every tool but bun and pdftotext answers, and Claude Code says a fictional version.
struct VersionedTools;

impl ToolProber for VersionedTools {
    fn probe(&self, tool: &ToolSpec) -> Option<String> {
        if tool.name == "claude" { Some("2.1.4 (Claude Code)".to_owned()) } else { FixedTools.probe(tool) }
    }
}

/// Copy details puts both versions and the system on the clipboard, for a bug report.
#[test]
fn copy_details_copies_the_versions() {
    let mut shell = labelled(probed(shell(Screen::InTray, "settings_copy_details"), VersionedTools));
    shell.open_settings(Page::About);
    let copied = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&copied);
    let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(install_look).build_ui(move |ui| {
        shell.show(ui);
        let texts: Vec<String> = ui.ctx().output(|output| {
            output.commands.iter().filter_map(|command| if let egui::OutputCommand::CopyText(text) = command { Some(text.clone()) } else { None }).collect()
        });
        seen.borrow_mut().extend(texts);
    });
    settle(&mut harness);
    assert!(is_shown(&harness, "2.1.4"));
    harness.get_by_label("Copy details").click();
    harness.run_steps(3);
    let system = if cfg!(target_os = "macos") { "macOS" } else { "Windows" };
    assert_eq!(*copied.borrow(), [format!("Jobdesk 0.1.0\n{system} ({})\nClaude Code 2.1.4", std::env::consts::ARCH)]);
}

/// While the first tool check runs, About says Claude Code is being checked, never that it is missing.
#[test]
fn about_says_checking_until_the_tools_answer() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let release = Arc::new(AtomicBool::new(false));
    let mut shell = labelled(shell(Screen::InTray, "settings_about_checking"));
    // No update check runs, so the only Checking… on the page is Claude Code's.
    shell.settings_mut().set_check_updates(false);
    let mut checks = EnvironmentChecks::new(Arc::new(GatedTools(Arc::clone(&release))), Arc::new(|| {}));
    checks.ensure_started();
    *shell.environment_slot() = Some(checks);
    shell.open_settings(Page::About);
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, "Checking…"));
    assert!(!is_shown(&harness, "Not found"), "a check still running never reads as a missing program");

    release.store(true, Ordering::SeqCst);
    settle(&mut harness);
    assert!(!is_shown(&harness, "Checking…"), "the answer replaces it");
    assert!(!is_shown(&harness, "Not found"));
}

/// Settings on `page` over the usage fixture, with bun missing so Runtime status says "1 missing".
fn render_settings_shell(mut shell: Shell, page: Page) -> Harness<'static> {
    shell.open_settings(page);
    probed_wgpu(probed(shell, VersionedTools), Theme::Light)
}

/// Run defaults as the canvas draws it: a $60 cap, three commands with their own model and budget, and October's $41.20.
#[test]
fn settings_defaults_light() {
    let name = "settings_defaults_light";
    let mut shell = shell_with_usage(name, usage_tasks());
    let settings = shell.settings_mut();
    settings.set_cap_on(true);
    settings.set_cap_amount(Dollars::parse("60"));
    command_row(settings, "/scrape", ModelChoice::Haiku, "1.00");
    command_row(settings, "/rank", ModelChoice::Sonnet, "3.00");
    command_row(settings, "/apply", ModelChoice::Opus, "5.00");
    let mut harness = render_settings_shell(shell, Page::Defaults);
    compare(&mut harness, name);
}

#[test]
fn settings_permissions_light() {
    let name = "settings_permissions_light";
    let mut harness = render_settings_shell(shell_with_usage(name, usage_tasks()), Page::Permissions);
    compare(&mut harness, name);
}

/// Data and privacy at the default Forever, with Clear tray decisions asking about 214 fictional decisions.
#[test]
fn settings_data_light() {
    let name = "settings_data_light";
    let data = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("data");
    let _ = fs::remove_dir_all(&data);
    let mut harness = render_settings_shell(shell_with_decisions(name, &data, 214), Page::Data);
    clear_decisions_button(&harness).click();
    harness.run_steps(3);
    // The pointer leaves the button, so it shows at rest under the popover.
    harness.hover_at(egui::pos2(700.0, 700.0));
    harness.run_steps(3);
    compare(&mut harness, name);
}

/// About with the switch on and the last check at 09:12 finding nothing newer: Up to date, and Check now.
#[test]
fn settings_about_light() {
    let name = "settings_about_light";
    let current = Release { version: Version { major: 0, minor: 1, patch: 0 }, ..release_020() };
    let shell = with_update(shell_with_usage(name, usage_tasks()), vec![ServiceEvent::Checked(Ok(current))]);
    let mut harness = render_settings_shell(shell, Page::About);
    assert!(is_shown(&harness, "Up to date") && is_shown(&harness, "Last checked today at 09:12."));
    compare(&mut harness, name);
}

/// Fictional notes for 0.2.0, as git-cliff writes them, with a link and an image that show only as words.
const RELEASE_NOTES: &str = "### Features\n\n- **settings**: keep run logs for 30 days, 90 days or forever ([#41](https://example.com/jobdesk/pull/41))\n- **today**: Later can set a card aside until Monday\n- **tasks**: reopen a stopped task with a reply ([#44](https://example.com/jobdesk/pull/44))\n\n![The new Data and privacy page](https://example.com/jobdesk/data.png)\n\n### Bug fixes\n\n- **jobs**: a cut title shows in full on hover\n- **profile**: Save keeps your draft when the file changed on disk\n- **shell**: the window keeps its size between launches\n- **settings**: search finds Spend at most again\n";
const RELEASE_PAGE: &str = "https://github.com/Nicolas-Widart-SRL/jobdesk/releases/tag/v0.2.0";

/// A fictional 0.2.0, released the day before the sample folder is read.
fn release_020() -> Release {
    Release {
        version: Version { major: 0, minor: 2, patch: 0 },
        released: NaiveDate::from_ymd_opt(2026, 10, 30),
        notes: RELEASE_NOTES.to_owned(),
        size: 14_200_000,
        signature: "fictional".to_owned(),
        pin: None,
    }
}

/// The check finds 0.2.0, then each download started ends as `then` says.
fn found_then(then: impl IntoIterator<Item = ServiceEvent>) -> Vec<ServiceEvent> {
    std::iter::once(ServiceEvent::Checked(Ok(release_020()))).chain(then).collect()
}

fn ended(end: DownloadEnd) -> ServiceEvent {
    ServiceEvent::Ended { id: DownloadId(0), end }
}

/// `shell` as a Mac copy of 0.1.0 whose updates answer from `script`, with the clock at 09:12 on the day it is read.
fn with_update(shell: Shell, script: Vec<ServiceEvent>) -> Shell {
    let at = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(9, 12, 0).unwrap();
    labelled(shell).with_now(at).with_updates(scripted_updates("0.1.0", script, None, true, Some(Platform::MacArm)))
}

/// Today with every tool ready, as the update canvases draw it, its updates answering from `script`.
fn update_harness(screen: Screen, name: &str, script: Vec<ServiceEvent>, theme: Theme) -> Harness<'static> {
    probed_wgpu(probed(with_update(shell(screen, name), script), AllTools), theme)
}

/// Clicks `label`, then takes the pointer off the window, so nothing shows its hover and no pointer is drawn.
fn click_away(harness: &mut Harness, label: &str) {
    harness.get_by_label(label).click();
    harness.run_steps(3);
    harness.event(egui::Event::PointerGone);
    harness.run_steps(3);
}

/// Today, as the check found 0.2.0: Jobdesk 0.2.0 is out, and Download.
#[test]
fn update_available_light() {
    let name = "update_available_light";
    let mut harness = update_harness(Screen::InTray, name, found_then([]), Theme::Light);
    assert!(is_shown(&harness, "Jobdesk 0.2.0 is out") && is_shown(&harness, "Download"));
    compare(&mut harness, name);
}

fn notes_open(name: &str, script: Vec<ServiceEvent>, mark: &str, theme: Theme) {
    let mut harness = update_harness(Screen::InTray, name, script, theme);
    click_away(&mut harness, mark);
    compare(&mut harness, name);
}

/// The notes open over Today from the bar, with Download 0.2.0 focused.
#[test]
fn update_notes_light() {
    notes_open("update_notes_light", found_then([]), "Jobdesk 0.2.0 is out", Theme::Light);
}

#[test]
fn update_notes_dark() {
    notes_open("update_notes_dark", found_then([]), "Jobdesk 0.2.0 is out", Theme::Dark);
}

/// After Download, each way the download goes, as the bar says it.
fn downloaded(name: &str, then: ServiceEvent) -> Harness<'static> {
    let mut harness = update_harness(Screen::InTray, name, found_then([then]), Theme::Light);
    click_away(&mut harness, "Download");
    harness
}

#[test]
fn update_downloading_light() {
    let name = "update_downloading_light";
    let mut harness = downloaded(name, ServiceEvent::Progress { id: DownloadId(0), done: 5_400_000, total: 14_200_000 });
    assert!(is_shown(&harness, "Downloading 0.2.0, 38%, 5.4 of 14.2 MB") && is_shown(&harness, "Cancel"));
    compare(&mut harness, name);
}

#[test]
fn update_ready_light() {
    let name = "update_ready_light";
    let mut harness = downloaded(name, ended(DownloadEnd::Done("/fictional/updates/Jobdesk.app.tar.gz".into())));
    assert!(is_shown(&harness, "0.2.0 is downloaded and checked") && is_shown(&harness, "Restart to update"));
    compare(&mut harness, name);
}

/// The notes once ready: what Restart does, then Restart to update in Download's place.
#[test]
fn update_notes_ready_light() {
    let name = "update_notes_ready_light";
    let mut harness = downloaded(name, ended(DownloadEnd::Done("/fictional/updates/Jobdesk.app.tar.gz".into())));
    click_away(&mut harness, "0.2.0 is downloaded and checked");
    assert!(is_shown(&harness, "Jobdesk closes, installs 0.2.0 and opens again with the same folder and settings. Restart waits while a task is open."));
    compare(&mut harness, name);
}

#[test]
fn update_failed_light() {
    let name = "update_failed_light";
    let mut harness = downloaded(name, ended(DownloadEnd::Failed { percent: 41, failure: Failure::Interrupted }));
    assert!(is_shown(&harness, "Download of 0.2.0 stopped at 41%: the connection dropped") && is_shown(&harness, "Retry"));
    compare(&mut harness, name);
}

#[test]
fn update_bad_signature_light() {
    let name = "update_bad_signature_light";
    let mut harness = downloaded(name, ended(DownloadEnd::BadSignature));
    assert!(is_shown(&harness, "0.2.0 failed its signature check. It was deleted, not installed."));
    compare(&mut harness, name);
}

/// Tasks with a run working: Restart to update is disabled, and says why on hover.
fn ready_busy(name: &str, theme: Theme) {
    let mut shell = with_update(shell(Screen::Runs, name), found_then([ended(DownloadEnd::Done("/fictional/updates/Jobdesk.app.tar.gz".into()))]));
    *shell.desk_slot() = Some(apply_run_ending([]));
    let mut harness = probed_wgpu(probed(shell, AllTools), theme);
    click_away(&mut harness, "Download");
    let restart = harness.get_by_label("Restart to update");
    assert!(restart.accesskit_node().is_disabled());
    restart.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "A task is running. Restart once it finishes, so it is not cut off halfway."));
    compare(&mut harness, name);
}

#[test]
fn update_ready_busy_light() {
    ready_busy("update_ready_busy_light", Theme::Light);
}

#[test]
fn update_ready_busy_dark() {
    ready_busy("update_ready_busy_dark", Theme::Dark);
}

/// About with the switch off and Check now pressed: Checking…, and Check now disabled.
#[test]
fn settings_about_update_checking_light() {
    let name = "settings_about_update_checking_light";
    let mut shell = with_update(shell_with_usage(name, usage_tasks()), Vec::new());
    shell.settings_mut().set_check_updates(false);
    let mut harness = render_settings_shell(shell, Page::About);
    assert!(is_shown(&harness, "Not checked yet."), "no check starts against the switch");
    click_away(&mut harness, "Check now");
    assert!(is_shown(&harness, "Asking GitHub for the latest release…"));
    assert!(harness.get_by_label("Check now").accesskit_node().is_disabled());
    compare(&mut harness, name);
}

/// The check timed out at 09:12: the reason in About, and Check failed beside About on the rail. The bar stays quiet.
#[test]
fn settings_about_update_failed_light() {
    let name = "settings_about_update_failed_light";
    let shell = with_update(shell_with_usage(name, usage_tasks()), vec![ServiceEvent::Checked(Err(Failure::TimedOut))]);
    let mut harness = render_settings_shell(shell, Page::About);
    assert!(is_shown(&harness, "Could not reach GitHub at 09:12: the connection timed out."));
    assert_eq!(harness.query_all_by_label("Check failed").count(), 2, "the tag and the rail");
    compare(&mut harness, name);
}

/// 0.2.0 is out: What's new, and Download 0.2.0.
#[test]
fn settings_about_update_light() {
    let name = "settings_about_update_light";
    let shell = with_update(shell_with_usage(name, usage_tasks()), found_then([]));
    let mut harness = render_settings_shell(shell, Page::About);
    assert!(is_shown(&harness, "Jobdesk 0.2.0, released Fri 30 Oct. You have 0.1.0. Last checked today at 09:12."));
    assert!(is_shown(&harness, "0.2.0 is out"), "the rail");
    compare(&mut harness, name);
}

/// Settings open while a task waits on the user, with 0.2.0 ready: Restart to update in About is disabled, and says why.
fn about_ready_waits(name: &str, theme: Theme) {
    let mut shell = with_update(shell_with_usage(name, usage_tasks()), found_then([ended(DownloadEnd::Done("/fictional/updates/Jobdesk.app.tar.gz".into()))]));
    *shell.desk_slot() = Some(tray_waiting_apply_run());
    shell.open_settings(Page::About);
    let mut harness = probed_wgpu(probed(shell, VersionedTools), theme);
    click_away(&mut harness, "Download 0.2.0");
    assert!(is_shown(&harness, "A task waits for you") && is_shown(&harness, "0.2.0 ready"));
    let about_restart = harness.get_all_by_label("Restart to update").min_by(|a, b| a.rect().top().total_cmp(&b.rect().top())).unwrap();
    assert!(about_restart.accesskit_node().is_disabled());
    let why = |harness: &Harness| harness.query_all_by_label("A task waits for your reply in Tasks. End its session there, then restart.").count();
    assert_eq!(why(&harness), 1, "the help says why, for the keyboard");
    about_restart.hover();
    harness.run_steps(8);
    assert_eq!(why(&harness), 2, "and the hover");
    compare(&mut harness, name);
}

#[test]
fn settings_about_ready_waits_light() {
    about_ready_waits("settings_about_ready_waits_light", Theme::Light);
}

#[test]
fn settings_about_ready_waits_dark() {
    about_ready_waits("settings_about_ready_waits_dark", Theme::Dark);
}

/// What's new opens the notes over Settings, Esc closes them before it leaves Settings, and the one link out is the
/// release page built from the version, never an address from the notes.
#[test]
fn the_notes_open_over_settings_and_esc_closes_them_first() {
    let shell = with_update(shell_with_usage("update_notes_esc", usage_tasks()), found_then([]));
    let mut shell = shell;
    shell.open_settings(Page::About);
    let opened = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&opened);
    let shared = Rc::new(RefCell::new(shell));
    let inner = Rc::clone(&shared);
    let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(install_look).build_ui(move |ui| {
        inner.borrow_mut().show(ui);
        let urls: Vec<String> = ui.ctx().output(|output| {
            output.commands.iter().filter_map(|command| if let egui::OutputCommand::OpenUrl(open) = command { Some(open.url.clone()) } else { None }).collect()
        });
        seen.borrow_mut().extend(urls);
    });
    harness.run_steps(3);
    let notes_shown = |harness: &Harness| harness.query_by_label("From the GitHub release. Its links and images do not open here.").is_some();
    assert!(!notes_shown(&harness));
    harness.get_by_label("What's new").click();
    harness.run_steps(3);
    assert!(notes_shown(&harness), "the notes open over Settings");
    harness.key_press(Key::Escape);
    harness.run_steps(3);
    assert!(!notes_shown(&harness), "Esc closes the notes");
    assert!(shared.borrow().settings_open(), "and Settings stays");

    harness.get_by_label("Jobdesk 0.2.0 is out").click();
    harness.run_steps(3);
    for link in harness.query_all_by_label("#41") {
        link.click();
    }
    harness.run_steps(3);
    harness.get_by_label("Open the release page").click();
    harness.run_steps(3);
    assert_eq!(*opened.borrow(), [RELEASE_PAGE], "only the page built from the version opens");
    harness.key_press(Key::Escape);
    harness.run_steps(3);
    harness.key_press(Key::Escape);
    harness.run_steps(3);
    assert!(!shared.borrow().settings_open(), "a second Esc leaves Settings");
}

/// After a Restart to update that did not install, and before a check finds the release again, the bar says why in plain
/// words: there are no notes to open, so nothing there is a button or takes the focus.
#[test]
fn a_restart_that_did_not_install_says_why_in_words_that_take_no_focus() {
    let mut updates = scripted_updates("0.1.0", Vec::new(), None, true, Some(Platform::MacArm));
    updates.apply_failed(ApplyFailed { version: Version { major: 0, minor: 2, patch: 0 }, failure: ApplyFailure::Replace });
    let mut harness = tray_harness(shell(Screen::InTray, "update_apply_failed").with_updates(updates));
    let words = "0.2.0 was not installed: the new copy could not be put in place";
    assert_eq!(harness.get_by_label(words).accesskit_node().role(), egui::accesskit::Role::Label);
    for _ in 0..40 {
        harness.key_press(Key::Tab);
        harness.run_steps(1);
        assert!(!harness.get_by_label(words).accesskit_node().is_focused(), "Tab never stops on the words");
    }
}

/// In the smallest window, with Settings open and a task waiting, the group's words are cut first: Retry and Settings
/// stay whole inside the window, and nothing of the group lies over the left of the bar.
#[test]
fn the_update_group_fits_the_smallest_window() {
    let script = found_then([ended(DownloadEnd::Failed { percent: 41, failure: Failure::Interrupted })]);
    let mut shell = with_update(shell(Screen::InTray, "update_smallest_window"), script);
    *shell.desk_slot() = Some(tray_waiting_apply_run());
    let mut harness = Harness::builder().with_size([1040.0, 640.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    harness.get_by_label("Download").click();
    harness.run_steps(3);
    harness.key_press_modifiers(COMMAND, Key::Comma);
    harness.run_steps(3);
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1040.0, 640.0));
    let rect = |label: &str| harness.get_by_label(label).rect();
    // The rail's title says Settings too.
    let settings = harness.get_by_role_and_label(egui::accesskit::Role::Button, STATUS_LINE).rect();
    let (retry, words, waits) = (rect("Retry"), rect("Download of 0.2.0 stopped at 41%: the connection dropped"), rect("A task waits for you"));
    assert!(window.contains_rect(retry) && window.contains_rect(settings), "{retry:?} {settings:?}");
    assert!(retry.right() < settings.left() && words.right() < retry.left());
    assert!(words.left() >= waits.right(), "the group never lies over the left: {words:?} {waits:?}");
}

/// Today with every tool ready, as the update canvases draw it, its folder update showing `stage`, as Jobdesk 0.4.0.
fn folder_harness(name: &str, stage: Stage, theme: Theme) -> Harness<'static> {
    let shell = with_update(shell(Screen::InTray, name), Vec::new()).with_folder_update(idle_folder_update("0.4.0").showing(stage, true));
    probed_wgpu(probed(shell, AllTools), theme)
}

fn side(path: &str, side: &str) -> SideCopy {
    SideCopy { path: path.into(), side: side.into() }
}

/// A fictional update to 1.8.0 that met the user's changes, the update's fix of two safety files, and one the user
/// had removed.
fn report_with_details() -> Report {
    Report {
        version: "1.8.0".into(),
        updated: vec![".claude/commands/apply.md".into()],
        conflicts: vec![side("CLAUDE.md", "CLAUDE.md.update-1.8.0"), side(".claude/commands/scrape.md", ".claude/commands/scrape.md.update-1.8.0")],
        upstream_wins: vec![side(".claude/settings.json", ".claude/settings.json.yours-1.8.0")],
        restored: vec![".claude/hooks/check-paths.py".into()],
        ..Report::default()
    }
}

/// The folder update downloads the release that goes with this Jobdesk: no Undo, nothing to put away.
fn folder_working(name: &str, theme: Theme) {
    let mut harness = folder_harness(name, Stage::Working(Phase::Downloading { received: 0 }), theme);
    assert!(is_shown(&harness, "Updating your job search folder to go with Jobdesk 0.4.0…"));
    assert!(harness.query_by_label("Undo").is_none() && harness.query_by_label("Dismiss").is_none());
    compare(&mut harness, name);
}

#[test]
fn folder_update_working_light() {
    folder_working("folder_update_working_light", Theme::Light);
}

#[test]
fn folder_update_working_dark() {
    folder_working("folder_update_working_dark", Theme::Dark);
}

/// An update that met no change of the user's: one line, Undo and Dismiss, and nothing to open.
fn folder_updated(name: &str, theme: Theme) {
    let report = Report { version: "1.8.0".into(), updated: vec!["CLAUDE.md".into()], ..Report::default() };
    let mut harness = folder_harness(name, Stage::Ended(FolderOutcome::Updated(report)), theme);
    let words = "Your job search folder was updated with Jobdesk 0.4.0.";
    assert_eq!(harness.get_by_label(words).accesskit_node().role(), egui::accesskit::Role::Label, "plain words with nothing to open");
    assert!(is_shown(&harness, "Undo") && is_shown(&harness, "Dismiss"));
    compare(&mut harness, name);
}

#[test]
fn folder_update_updated_light() {
    folder_updated("folder_update_updated_light", Theme::Light);
}

#[test]
fn folder_update_updated_dark() {
    folder_updated("folder_update_updated_dark", Theme::Dark);
}

/// The same line with its details open above it: the files the user changed kept beside the update's copy, the safety
/// files replaced beside the user's, and the one put back, each path as plain text.
fn folder_details(name: &str, theme: Theme) {
    let mut harness = folder_harness(name, Stage::Ended(FolderOutcome::Updated(report_with_details())), theme);
    click_away(&mut harness, "Your job search folder was updated with Jobdesk 0.4.0.");
    for line in [
        "You had changed 2 files that this update also changed. Jobdesk kept yours and saved the new version beside each:",
        "CLAUDE.md, new version in CLAUDE.md.update-1.8.0",
        "Jobdesk replaced 1 safety file with the new version and saved yours beside it:",
        ".claude/settings.json, yours in .claude/settings.json.yours-1.8.0",
        "Jobdesk put back 1 safety file you had removed:",
        ".claude/hooks/check-paths.py",
        "Undo the update",
    ] {
        assert!(is_shown(&harness, line), "{line}");
    }
    compare(&mut harness, name);
}

#[test]
fn folder_update_details_light() {
    folder_details("folder_update_details_light", Theme::Light);
}

#[test]
fn folder_update_details_dark() {
    folder_details("folder_update_details_dark", Theme::Dark);
}

/// An update that stopped at an earlier open is undone, with nothing to keep: one line, and Dismiss.
fn folder_recovered(name: &str, theme: Theme) {
    let reverted = Reverted { reverted: vec!["CLAUDE.md".into(), ".claude/commands/apply.md".into()], kept: Vec::new() };
    let mut harness = folder_harness(name, Stage::Ended(FolderOutcome::Recovered(reverted)), theme);
    assert!(is_shown(&harness, "Jobdesk finished undoing an interrupted update of your job search folder."));
    assert!(harness.query_by_label("Undo").is_none() && is_shown(&harness, "Dismiss"));
    compare(&mut harness, name);
}

#[test]
fn folder_update_recovered_light() {
    folder_recovered("folder_update_recovered_light", Theme::Light);
}

#[test]
fn folder_update_recovered_dark() {
    folder_recovered("folder_update_recovered_dark", Theme::Dark);
}

/// The download had no connection: the failed look, and nothing changed.
fn folder_failed(name: &str, theme: Theme) {
    let mut harness = folder_harness(name, Stage::Ended(FolderOutcome::Failed(FolderFailure::Offline)), theme);
    assert!(is_shown(
        &harness,
        "Your job search folder was not updated: the connection failed or dropped during the download. Nothing changed, and Jobdesk tries again next time."
    ));
    assert!(harness.query_by_label("Undo").is_none());
    compare(&mut harness, name);
}

#[test]
fn folder_update_failed_light() {
    folder_failed("folder_update_failed_light", Theme::Light);
}

#[test]
fn folder_update_failed_dark() {
    folder_failed("folder_update_failed_dark", Theme::Dark);
}

/// Undo put the folder back but for one file the user changed since, which its details list.
fn folder_undone(name: &str, theme: Theme) {
    let reverted = Reverted { reverted: vec![".claude/commands/apply.md".into()], kept: vec!["CLAUDE.md".into()] };
    let mut harness = folder_harness(name, Stage::Undid(UndoOutcome::Undone(reverted)), theme);
    click_away(
        &mut harness,
        "Jobdesk undid the update, except for 1 file that changed since. It updates your job search folder again when a later Jobdesk brings a newer version of its files.",
    );
    assert!(is_shown(&harness, "Jobdesk left 1 file as it is, because it changed after the update wrote it:") && is_shown(&harness, "CLAUDE.md"));
    assert!(harness.query_by_label("Undo the update").is_none(), "nothing left to undo");
    compare(&mut harness, name);
}

#[test]
fn folder_update_undone_light() {
    folder_undone("folder_update_undone_light", Theme::Light);
}

#[test]
fn folder_update_undone_dark() {
    folder_undone("folder_update_undone_dark", Theme::Dark);
}

/// About says, in one quiet line, why Jobdesk leaves a folder under git as it is, and the bar says nothing.
fn about_git_folder(name: &str, theme: Theme) {
    let git = idle_folder_update("0.4.0").showing(Stage::Ended(FolderOutcome::GitFolder), false);
    let current = Release { version: Version { major: 0, minor: 1, patch: 0 }, ..release_020() };
    let mut shell = with_update(shell_with_usage(name, usage_tasks()), vec![ServiceEvent::Checked(Ok(current))]).with_folder_update(git);
    shell.open_settings(Page::About);
    let mut harness = probed_wgpu(probed(shell, VersionedTools), theme);
    assert!(is_shown(&harness, "This folder uses git, so updating it is up to you.") && is_shown(&harness, "Up to date"));
    assert!(harness.query_by_label("Dismiss").is_none(), "no notice");
    compare(&mut harness, name);
}

#[test]
fn settings_about_folder_git_light() {
    about_git_folder("settings_about_folder_git_light", Theme::Light);
}

#[test]
fn settings_about_folder_git_dark() {
    about_git_folder("settings_about_folder_git_dark", Theme::Dark);
}

/// The details' Esc closes them before anything behind them takes it, and Dismiss puts the notice away.
#[test]
fn the_folder_details_close_on_esc_and_dismiss_puts_the_notice_away() {
    let shell = with_update(shell(Screen::InTray, "folder_update_esc"), Vec::new())
        .with_folder_update(idle_folder_update("0.4.0").showing(Stage::Ended(FolderOutcome::Updated(report_with_details())), true));
    let mut harness = tray_harness(shell);
    let words = "Your job search folder was updated with Jobdesk 0.4.0.";
    let details = |harness: &Harness| harness.query_by_label("Jobdesk put back 1 safety file you had removed:").is_some();
    harness.get_by_label(words).click();
    harness.run_steps(3);
    assert!(details(&harness));
    harness.key_press(Key::Escape);
    harness.run_steps(3);
    assert!(!details(&harness), "Esc closes the details");
    harness.get_by_label("Dismiss").click();
    harness.run_steps(3);
    assert!(harness.query_by_label(words).is_none(), "Dismiss puts the notice away");
}

/// Every required tool answers, so the menu has no Setup tab.
#[test]
fn menu_without_setup_light() {
    let mut harness = probed_wgpu(probed(shell(Screen::InTray, "menu_without_setup_light"), AllTools), Theme::Light);
    compare(&mut harness, "menu_without_setup_light");
}

/// The sample folder, open but not read yet.
fn loading_shell(name: &str) -> Shell {
    loading_on(Screen::InTray, name)
}

/// The sample folder, open on `screen` but not read yet.
fn loading_on(screen: Screen, name: &str) -> Shell {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    Shell::loading(screen, jobdesk::workspace::open(&root).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 31).unwrap())
}

/// Until the first read, no screen says it has nothing: each says it is reading, and its head actions stay.
#[test]
fn loading_screens_promise_nothing() {
    let empty = [
        "No jobs wait here.",
        "No jobs wait here",
        "No applications yet",
        "No applications sent yet.",
        "No skill gap report yet",
        "No portals installed.",
        "No portals yet",
    ];
    let cases: [(Screen, &str, &[&str]); 6] = [
        (Screen::Jobs, "Reading your saved jobs…", &["Find new jobs…", "Score", "Worth knowing"]),
        (Screen::Tracker, "Reading your applications…", &["Record a result…", "Check mail for replies…", "Not sent yet", "Applied", "Closed"]),
        (Screen::Insights, "Reading your jobs, applications and reports…", &["Find skill gaps…", "Rebuild dashboard…", "Result", "Funnel", "By sector"]),
        (Screen::Workspace, "Reading your profile, tools and portals…", &["Change folder", "Environment", ".agents/skills"]),
        (Screen::Profile, "Reading your profile…", &["Topics", "On your CV", "What guides Claude"]),
        (Screen::Portals, "Reading your portals…", &[]),
    ];
    for (screen, subtitle, shown) in cases {
        let mut shell = loading_on(screen, &format!("loading_{screen:?}"));
        let mut checks = EnvironmentChecks::new(Arc::new(FixedTools), Arc::new(|| {}));
        checks.ensure_started();
        *shell.environment_slot() = Some(checks);
        let harness = tray_harness(shell);
        assert!(is_shown(&harness, subtitle), "{screen:?} says it is reading");
        for text in shown {
            assert!(is_shown(&harness, text), "{screen:?} shows {text}");
        }
        for text in empty {
            assert!(!is_shown(&harness, text), "{screen:?} promises {text}");
        }
        // The Profile tab is always there, so the panel's own "Profile" is the one past it.
        if matches!(screen, Screen::Insights | Screen::Workspace) {
            assert_eq!(harness.query_all_by_label("Profile").count(), 2, "{screen:?} names its Profile panel beside the tab");
        }
    }
    let harness = tray_harness(loading_on(Screen::Workspace, "loading_setup_name"));
    assert!(!is_shown(&harness, "ai-job-search"), "the folder name waits for the read");
}

#[test]
fn jobs_loading_light() {
    render_probed(loading_on(Screen::Jobs, "jobs_loading_light"), Theme::Light, "jobs_loading_light");
}

#[test]
fn tracker_loading_light() {
    render_probed(loading_on(Screen::Tracker, "tracker_loading_light"), Theme::Light, "tracker_loading_light");
}

#[test]
fn insights_loading_dark() {
    render_probed(loading_on(Screen::Insights, "insights_loading_dark"), Theme::Dark, "insights_loading_dark");
}

#[test]
fn loading_deals_no_card_and_ignores_keys() {
    let mut shell = loading_shell("tray_loading");
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(SharedSink::default())));
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, "Reading your job search…"));
    assert!(is_shown(&harness, "Your first card appears in a moment."));
    assert!(is_shown(&harness, "Saturday 31 October"), "the stamp is empty and reads as the date");

    press(&mut harness, Key::Y);
    press(&mut harness, Key::L);

    assert!(!is_shown(&harness, LAUNCHER), "Y opens no launcher");
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
fn run_working_dark() {
    render_desk(apply_drafting().0, Theme::Dark, "run_working_dark");
}

/// No update for 3 min: the edge is a still dashed rule, and What next says long writes can be quiet.
#[test]
fn run_quiet_light() {
    render_desk(apply_drafting_quiet(), Theme::Light, "run_quiet_light");
}

const ESTIMATE: &str = "Estimated until Claude's turn ends.";

/// A harness on the Tasks screen that keeps the shell within reach, so a test can move the desk's clock.
fn live_harness(desk: RunDesk, name: &str) -> (Harness<'static>, Rc<RefCell<Shell>>) {
    let mut shell = shell(Screen::Runs, name);
    *shell.desk_slot() = Some(desk);
    shared_harness(shell)
}

/// Moves the clock of the shell's desk to `seconds`, then draws.
fn move_clock(harness: &mut Harness, shell: &Rc<RefCell<Shell>>, seconds: f64) {
    clock_at(shell.borrow_mut().desk_slot().as_mut().unwrap(), seconds);
    harness.run_steps(3);
}

#[test]
fn the_cost_is_an_estimate_until_the_turn_ends() {
    let (desk, sink) = apply_drafting();
    let (mut harness, _shell) = live_harness(desk, "cost_estimate");
    assert!(is_shown(&harness, "About $1.24 of $3.00") && is_shown(&harness, ESTIMATE), "while Claude works");
    assert!(is_shown(&harness, "$1.24"), "Recent runs shows the live cost without About");
    emit_all(&sink, [AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 1.31, duration_ms: 1 }]);
    harness.run_steps(3);
    assert!(is_shown(&harness, "$1.31 of $3.00"), "the result's exact figure");
    assert!(!is_shown(&harness, ESTIMATE) && !is_shown(&harness, "About $1.31 of $3.00"));
}

#[test]
fn elapsed_ticks_while_claude_works_and_holds_when_done() {
    let (desk, sink) = apply_drafting();
    let (mut harness, shell) = live_harness(desk, "elapsed_holds");
    assert!(is_shown(&harness, "4:13"));
    move_clock(&mut harness, &shell, 300.0);
    assert!(is_shown(&harness, "5:00"), "it ticks while Claude works");
    emit_all(&sink, [AgentEvent::AssistantText { text: "Both documents are drafted.".into(), nested: false }]);
    emit_all(&sink, [AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 1.31, duration_ms: 1 }]);
    harness.run_steps(3);
    move_clock(&mut harness, &shell, 400.0);
    assert!(is_shown(&harness, "Claude is done for now"));
    assert!(is_shown(&harness, "5:00") && !is_shown(&harness, "6:40"), "done for now holds it");

    let id = shell.borrow_mut().desk_slot().as_ref().unwrap().runs()[0].id.clone();
    shell.borrow_mut().desk_slot().as_mut().unwrap().reply(&id, "Make the letter shorter.").unwrap();
    move_clock(&mut harness, &shell, 410.0);
    assert!(is_shown(&harness, "5:10"), "a reply goes on from the held figure");
}

#[test]
fn elapsed_keeps_going_through_a_question() {
    let (mut harness, shell) = live_harness(apply_run(question_prompt()), "elapsed_question");
    assert!(is_shown(&harness, "9:31"), "Claude asked at 8:05");
    assert!(is_shown(&harness, "About $1.42 of $5.00") && is_shown(&harness, ESTIMATE), "a pending prompt keeps About");
    move_clock(&mut harness, &shell, 600.0);
    assert!(is_shown(&harness, "10:00"));
}

#[test]
fn the_whose_turn_cell_reads_its_quiet_state_not_the_seconds() {
    let (desk, _sink) = apply_drafting();
    let (mut harness, shell) = live_harness(desk, "spoken_quiet");
    let fresh = "Claude's turn. Last update under 30 seconds ago";
    assert!(is_shown(&harness, fresh) && !is_shown(&harness, "Last update 3s ago"), "the note is read by its state");
    move_clock(&mut harness, &shell, 260.0);
    assert!(is_shown(&harness, fresh), "the same words ten seconds on");
    move_clock(&mut harness, &shell, 300.0);
    assert!(is_shown(&harness, "Claude's turn. No update for over 30 seconds"));
    move_clock(&mut harness, &shell, 430.1);
    assert!(is_shown(&harness, "Claude's turn. No update for over 2 minutes"));
    assert!(is_shown(&harness, "Long writes can be quiet. Stop run keeps the files so far."));
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
    let mut harness =
        with_renderer(Harness::builder().with_size([1040.0, 650.0]).with_pixels_per_point(1.5).with_theme(Theme::Light).with_context_setup(install_look))
            .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    compare(&mut harness, "run_turn_1040_light");
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

/// The run logs sit outside the checkout, so the shell asks for a reload when a run stops working.
fn reload_harness(name: &str) -> (Harness<'static>, SharedSink, std::sync::mpsc::Receiver<()>) {
    let mut shell = shell(Screen::Runs, name);
    let reloads = shell.reload_feed();
    let sink = SharedSink::default();
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    desk.start(APPLY_LINE, &StartOptions { model: None, budget_usd: 3.0 }, run_started()).unwrap();
    *shell.desk_slot() = Some(desk);
    let harness = tray_harness(shell);
    (harness, sink, reloads)
}

#[test]
fn a_run_that_ends_its_turn_sends_one_reload() {
    let (mut harness, sink, reloads) = reload_harness("reload_nudge_ended");
    assert!(reloads.try_recv().is_err(), "a working run sends none");

    emit_all(&sink, [AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.4, duration_ms: 9_000 }]);
    harness.run_steps(4);

    assert!(reloads.try_recv().is_ok(), "the end of the turn asks for a reload");
    assert!(reloads.try_recv().is_err(), "and only once");
}

#[test]
fn a_run_that_keeps_working_sends_no_reload() {
    let (mut harness, sink, reloads) = reload_harness("reload_nudge_working");

    emit_all(&sink, [tool("1", "Read", "cv/main.tex", false), done("1")]);
    harness.run_steps(4);

    assert!(reloads.try_recv().is_err());
}

const CANDIDATE: &str = ".claude/skills/job-application-assistant/01-candidate-profile.md";
const PROFILE_SUBTITLE: &str = "Claude reads this before it scores a job or writes a CV or a letter.";
const CLAUDE_UPDATE: &str = "Update with Claude…";

/// The sample folder at a fixed place, for a test that changes its files as a run would.
fn sample_root(name: &str) -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    root
}

fn rewrite(root: &Path, relative: &str, from: &str, to: &str) {
    let path = root.join(relative);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(from), "{from} is in {relative}");
    fs::write(&path, text.replace(from, to)).unwrap();
}

/// The sample on Profile, with a `/setup` run working and the feed that hands the shell snapshots, as the folder watcher does.
fn profile_with_run(name: &str) -> (Shell, std::path::PathBuf, SharedSink, std::sync::mpsc::Sender<Snapshot>) {
    profile_with_run_on(sample_root(name))
}

/// The same on a folder a test has prepared.
fn profile_with_run_on(root: std::path::PathBuf) -> (Shell, std::path::PathBuf, SharedSink, std::sync::mpsc::Sender<Snapshot>) {
    let mut shell = Shell::with_snapshot(Screen::Profile, jobdesk::workspace::open(&root).unwrap(), Snapshot::load(&root, day()));
    let feed = shell.snapshot_feed();
    let sink = SharedSink::default();
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    desk.start("/setup --section experience", &StartOptions { model: None, budget_usd: 2.0 }, run_started()).unwrap();
    *shell.desk_slot() = Some(desk);
    (shell, root, sink, feed)
}

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
}

/// The run's result: its turn ends and its process exits, so nothing works any more.
fn end_run(sink: &SharedSink) {
    emit_all(sink, [AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.98, duration_ms: 95_000 }, AgentEvent::ProcessExited { failure: None }]);
}

/// Clicks a topic's row on the rail, which comes before the paper.
fn pick_topic(harness: &mut Harness, title: &str) {
    harness.get_all_by_label(title).next().unwrap_or_else(|| panic!("{title} is on the rail")).click();
    harness.run_steps(3);
}

/// The rail's Updated marks, by the row they sit on.
fn marked_rows(harness: &Harness) -> Vec<String> {
    let marks: Vec<egui::Rect> = harness.query_all_by_label("Updated").map(|mark| mark.rect()).collect();
    TOPIC_TITLES
        .iter()
        .filter(|title| {
            let row = harness.get_all_by_label(title).next().unwrap().rect();
            marks.iter().any(|mark| (mark.center().y - row.center().y).abs() < 4.0 && mark.left() < 320.0)
        })
        .map(|title| title.to_string())
        .collect()
}

/// A snapshot read before the run ended may already hold some of its writes, yet it may miss the last. It marks
/// nothing, and the first one read after the end marks what the run changed.
#[test]
fn only_a_snapshot_read_after_the_end_marks_the_run() {
    let (shell, root, sink, feed) = profile_with_run("profile_marks_late");
    let mut harness = tray_harness(shell);
    pick_topic(&mut harness, "Identity and languages");
    rewrite(&root, CANDIDATE, "Mentored four engineers", "Mentored five engineers");
    let early = Snapshot::load(&root, day());
    end_run(&sink);
    harness.run_steps(3);
    feed.send(early).unwrap();
    harness.run_steps(3);
    assert!(marked_rows(&harness).is_empty(), "the snapshot was read before the run ended");

    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    assert_eq!(marked_rows(&harness), ["Experience"]);
}

/// `/setup` asks and waits, so a reply can start the next turn before the load that follows the first turn arrives.
/// The profile kept at the run's start stays, so the end of the second turn marks the changes of both, even though a
/// snapshot that holds the first turn's change arrived while it worked.
#[test]
fn a_reply_before_the_reload_keeps_the_first_turns_changes() {
    let (shell, root, sink, feed) = profile_with_run("profile_marks_reply");
    let (mut harness, shell) = shared_harness(shell);
    pick_topic(&mut harness, "Education");
    let identity = harness.get_all_by_label("Identity and languages").next().unwrap().rect().width();
    rewrite(&root, CANDIDATE, "Mentored four engineers", "Mentored five engineers");
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    emit_all(&sink, [AgentEvent::TurnEnded { failed: false, message: Some("Anything else?".into()), cost_usd: 0.4, duration_ms: 40_000 }]);
    harness.run_steps(3);

    let id = shell.borrow_mut().desk_slot().as_ref().unwrap().runs()[0].id.clone();
    shell.borrow_mut().desk_slot().as_mut().unwrap().reply(&id, "Add Namur to my identity.").unwrap();
    harness.run_steps(3);
    rewrite(&root, CANDIDATE, "- **Location:** Namur, Belgium\n", "- **Location:** Namur, Belgium (since 2016)\n");
    end_run(&sink);
    harness.run_steps(3);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);

    assert_eq!(marked_rows(&harness), ["Identity and languages", "Experience"], "both turns' changes are marked");
    assert_eq!(harness.get_all_by_label("Identity and languages").next().unwrap().rect().width(), identity, "the longest title fits beside the mark");
    harness.get_by_label("Education, Filled");

    // A sighted keyboard user who tabs to the marked row reads what the pointer would.
    assert!(!is_shown(&harness, "Updated by the last task. Open the topic to see it."));
    harness.get_by_label("Identity and languages, Filled. Updated by the last task. Open the topic to see it.").focus();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Updated by the last task. Open the topic to see it."), "focus shows the mark's tip");
}

/// A run that starts before the folder is first read takes that first read as its start, not the empty snapshot.
#[test]
fn a_run_started_before_the_first_read_still_marks_its_changes() {
    let root = sample_root("profile_marks_first_read");
    let mut shell = Shell::loading(Screen::Profile, jobdesk::workspace::open(&root).unwrap(), day());
    let feed = shell.snapshot_feed();
    let sink = SharedSink::default();
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    desk.start("/setup --section experience", &StartOptions { model: None, budget_usd: 2.0 }, run_started()).unwrap();
    *shell.desk_slot() = Some(desk);
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, "Reading your profile…"));
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    pick_topic(&mut harness, "Education");

    rewrite(&root, CANDIDATE, "Mentored four engineers", "Mentored five engineers");
    end_run(&sink);
    harness.run_steps(3);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    assert_eq!(marked_rows(&harness), ["Experience"]);
}

/// The empty topic shows the command it opens, on asking, and starts nothing.
#[test]
fn an_empty_topic_shows_its_command() {
    let started = SharedSink::default();
    let mut shell = shell(Screen::Profile, "profile_empty_command");
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(Arc::clone(&started))));
    let mut harness = tray_harness(shell);
    pick_topic(&mut harness, "Goals and deal-breakers");
    assert!(!is_shown(&harness, "/setup --section goals"));
    harness.get_by_label("Show the command").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "/setup --section goals"));
    assert!(!is_shown(&harness, LAUNCHER) && started.lock().unwrap().is_none(), "nothing opens or starts");
    pick_topic(&mut harness, "Experience");
    pick_topic(&mut harness, "Goals and deal-breakers");
    assert!(!is_shown(&harness, "/setup --section goals"), "another topic closes the line");
}

/// A hidden topic says so without counting the others, which another file may have hidden.
#[test]
fn a_hidden_topic_names_no_count() {
    let root = sample_root("profile_hidden_note");
    fs::write(root.join(CANDIDATE), b"# Candidate Profile\n\nCaf\xe9\n").unwrap();
    fs::write(root.join(".claude/skills/job-application-assistant/02-behavioral-profile.md"), b"Caf\xe9\n").unwrap();
    let shell = Shell::with_snapshot(Screen::Profile, jobdesk::workspace::open(&root).unwrap(), Snapshot::load(&root, day()));
    let mut harness = tray_harness(shell);
    pick_topic(&mut harness, "Experience");
    assert!(harness.query_by_label_contains("so it is not shown").is_some());
    assert!(harness.query_by_label_contains("others").is_none());
}

/// A run that rewrites one role between its start and its result marks Experience alone. Opening it clears the rail
/// mark and shows the head line, which names the role.
#[test]
fn a_run_that_changes_one_role_marks_experience_only() {
    let (shell, root, sink, feed) = profile_with_run("profile_marks");
    let mut harness = tray_harness(shell);
    pick_topic(&mut harness, "Identity and languages");
    assert!(!is_shown(&harness, "Updated"), "nothing is marked while the run works");

    rewrite(&root, CANDIDATE, "Mentored four engineers", "Mentored five engineers");
    end_run(&sink);
    harness.run_steps(3);
    assert!(!is_shown(&harness, "Updated"), "the snapshot on show predates the run's writes");
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);

    assert_eq!(harness.query_all_by_label("Updated").count(), 1, "one topic is marked");
    let mark = harness.get_by_label("Updated").rect();
    let row = harness.get_all_by_label("Experience").next().unwrap().rect();
    assert!((mark.center().y - row.center().y).abs() < 4.0, "the mark {mark:?} sits on Experience's row {row:?}");
    assert!(harness.query_by_label_contains("It changed").is_none(), "Identity's head has no line");

    pick_topic(&mut harness, "Experience");
    let line = harness.get_by_label_contains("It changed");
    assert_eq!(
        line.accesskit_node().value().unwrap_or_default(),
        "by the last task, just now. It changed Senior Backend Engineer - Wren Health (2017 – 2021)."
    );
    assert_eq!(harness.query_all_by_label("Updated").count(), 1, "the rail mark cleared, and the head's shows");
    assert!(harness.get_by_label("Updated").rect().left() > row.right(), "the mark left is the head's, on the paper");

    pick_topic(&mut harness, "Education");
    pick_topic(&mut harness, "Experience");
    assert!(harness.query_by_label_contains("It changed").is_none(), "leaving the topic dropped its head line");
}

/// Update with Claude… opens the launcher on the topic's section, and the user starts the run there.
#[test]
fn update_with_claude_fills_the_launcher_and_starts_nothing() {
    let started = SharedSink::default();
    let mut shell = shell(Screen::Profile, "profile_update_with_claude");
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(Arc::clone(&started))));
    let mut harness = tray_harness(shell);
    assert!(is_shown(&harness, "5 roles. Claude picks from these for each CV."), "Experience opens first: it has blanks to fill");

    harness.get_by_label(CLAUDE_UPDATE).click();
    harness.run_steps(3);

    assert!(is_shown(&harness, LAUNCHER), "the launcher opens");
    assert!(harness.query_all_by_label("/setup --section experience").count() > 0, "the line names the topic's section");
    assert!(started.lock().unwrap().is_none(), "no run starts");
}

/// While a run works, Update with Claude… waits and says why.
#[test]
fn update_with_claude_waits_while_a_run_works() {
    let (shell, _root, _sink, _feed) = profile_with_run("profile_update_busy");
    let mut harness = tray_harness(shell);
    let update = harness.get_by_label(CLAUDE_UPDATE);
    assert!(update.accesskit_node().is_disabled(), "a run works");
    update.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "Another run is active. One run at a time keeps your files safe."));
}

/// Only the profile part of `CLAUDE.md` reaches the screen, on every topic.
#[test]
fn the_rest_of_claude_md_never_shows() {
    let mut harness = tray_harness(shell(Screen::Profile, "profile_claude_md"));
    for title in TOPIC_TITLES {
        pick_topic(&mut harness, title);
        for hidden in ["Repo Structure", "Folder layout notes", "Copied to `CLAUDE.md`", "Copied to CLAUDE.md"] {
            assert!(harness.query_by_label_contains(hidden).is_none(), "{title} shows {hidden}");
        }
    }
}

const TOPIC_TITLES: [&str; 11] = [
    "Identity and languages",
    "Experience",
    "Education",
    "Skills",
    "Projects and publications",
    "References",
    "Profile statements",
    "How I work",
    "Goals and deal-breakers",
    "Interview stories",
    "Job search",
];

/// `/setup` has no section for these two, so they have no Claude action.
#[test]
fn statements_and_stories_have_no_claude_action() {
    let mut harness = tray_harness(shell(Screen::Profile, "profile_no_section"));
    assert!(is_shown(&harness, CLAUDE_UPDATE), "Experience has one");
    for title in ["Profile statements", "Interview stories"] {
        pick_topic(&mut harness, title);
        assert!(!is_shown(&harness, CLAUDE_UPDATE), "{title} has none");
    }
}

/// Setup's Profile panel is one line: the count of topics filled, and the way to Profile.
#[test]
fn setup_sums_up_the_profile_and_opens_it() {
    let mut harness = tray_harness(probed(shell(Screen::Workspace, "setup_profile_line"), FixedTools));
    settle(&mut harness);
    assert!(is_shown(&harness, "7 of 11 topics filled"));
    for gone in ["Your writing style", "Cover letter templates", "03-writing-style.md", "06-cover-letter-templates.md", "Update…"] {
        assert!(harness.query_by_label(gone).is_none(), "Setup still shows {gone}");
    }
    harness.get_by_label("Open Profile").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, PROFILE_SUBTITLE), "Profile is on show");
}

/// Runtime status sums up the profile in Setup's line, and Open Profile leaves Settings for Profile.
#[test]
fn runtime_status_opens_profile() {
    let mut shell = shell(Screen::InTray, "runtime_profile_line");
    shell.open_settings(Page::Runtime);
    let (mut harness, shell) = shared_harness(probed(shell, FixedTools));
    settle(&mut harness);
    assert!(is_shown(&harness, "7 of 11 topics filled"));
    harness.get_by_label("Open Profile").click();
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open());
    assert!(is_shown(&harness, PROFILE_SUBTITLE), "Profile is on show");
}

/// The smallest window still shows every tab and New task.
#[test]
fn the_menu_fits_the_smallest_window() {
    let mut shell = probed(shell(Screen::Profile, "profile_smallest"), FixedTools);
    let mut harness = Harness::builder().with_size([1040.0, 640.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
    settle(&mut harness);
    let tabs: Vec<String> = harness.get_all_by_role(egui::accesskit::Role::Tab).map(|tab| tab.accesskit_node().label().unwrap_or_default()).collect();
    for name in ["Today", "Jobs", "Applications", "Insights", "Tasks", "Profile", "Portals", "Setup"] {
        assert!(tabs.iter().any(|tab| tab.starts_with(name)), "{name} is in {tabs:?}");
    }
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1040.0, 640.0));
    for tab in harness.get_all_by_role(egui::accesskit::Role::Tab) {
        assert!(window.contains_rect(tab.rect()), "{:?} at {:?}", tab.accesskit_node().label(), tab.rect());
    }
    assert_in_window(&harness, [1040.0, 640.0], &["New task", CLAUDE_UPDATE]);
}

/// Renders Profile once the tool check is back, after `pick` picks a topic, if it does.
fn render_profile(shell: Shell, pick: Option<&str>, theme: Theme, name: &str) {
    let mut harness = probed_wgpu(probed(shell, FixedTools), theme);
    if let Some(title) = pick {
        pick_topic(&mut harness, title);
    }
    compare(&mut harness, name);
}

#[test]
fn profile_view_light() {
    render_profile(shell(Screen::Profile, "profile_view_light"), None, Theme::Light, "profile_view_light");
}

#[test]
fn profile_view_dark() {
    render_profile(shell(Screen::Profile, "profile_view_dark"), None, Theme::Dark, "profile_view_dark");
}

/// Identity holds the Languages table, which takes the paper's width beside its Edit.
#[test]
fn profile_identity_light() {
    render_profile(shell(Screen::Profile, "profile_identity_light"), Some("Identity and languages"), Theme::Light, "profile_identity_light");
}

#[test]
fn profile_empty_light() {
    render_profile(shell(Screen::Profile, "profile_empty_light"), Some("Goals and deal-breakers"), Theme::Light, "profile_empty_light");
}

#[test]
fn profile_loading_light() {
    render_profile(loading_on(Screen::Profile, "profile_loading_light"), None, Theme::Light, "profile_loading_light");
}

/// `01` cannot be read, so its six topics are hidden. Experience is picked, as on the canvas.
#[test]
fn profile_failed_light() {
    let root = sample_root("profile_failed_light");
    fs::write(root.join(CANDIDATE), b"# Candidate Profile\n\nCaf\xe9\n").unwrap();
    let snapshot = Snapshot::load(&root, day());
    assert_eq!(snapshot.issues.len(), 1, "{:?}", snapshot.issues);
    let shell = Shell::with_snapshot(Screen::Profile, jobdesk::workspace::open(&root).unwrap(), snapshot);
    render_profile(shell, Some("Experience"), Theme::Light, "profile_failed_light");
}

/// A run changed Identity and one role. Experience is open, so its rail mark has cleared and its head says what changed.
#[test]
fn profile_updated_light() {
    let (shell, root, sink, feed) = profile_with_run("profile_updated_light");
    let mut harness = probed_wgpu(probed(shell, FixedTools), Theme::Light);
    rewrite(&root, CANDIDATE, "- **Location:** Namur, Belgium\n", "- **Location:** Namur, Belgium (since 2016)\n");
    rewrite(&root, CANDIDATE, "update within a minute.\n", "update within a minute.\n- Introduced a weekly planning review with depot managers.\n");
    end_run(&sink);
    harness.run_steps(3);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    compare(&mut harness, "profile_updated_light");
}

const CV_DOCUMENTS: &str = "CV documents";
const REBUILD: &str = "Rebuild the PDF…";
const CV_SHOWS: &str = "Jobdesk shows these PDFs. /build-cv builds them.";

/// The moment the CV documents' ages count to.
fn cv_now() -> NaiveDateTime {
    run_started()
}

/// A stem, the hours since its `.tex` and its PDF changed, the PDF's size and its pages.
type CvFile = (&'static str, Option<i64>, Option<i64>, usize, usize);

/// The canvas's nine CV documents, by stem: hours before `cv_now` their `.tex` and PDF were last changed, if they exist,
/// the PDF's size and its pages. Halyard has no PDF yet, Tessel's `.tex` changed after its PDF, Brightwell's PDF is one
/// no renderer reads (no pages), and Linden has no `.tex`.
const CV_FILES: [CvFile; 9] = [
    ("main_example", Some(5 * 24), Some(3 * 24), 84_000, 2),
    ("main_example_engineer", Some(14 * 24), Some(12 * 24), 97_000, 2),
    ("main_halyard_labs_tech_lead", Some(1), None, 0, 0),
    ("main_tessel_platform_lead", Some(3), Some(2 * 24), 91_000, 2),
    ("main_brightwell_eng_manager", Some(3 * 24), Some(24), 88_000, 0),
    ("main_ardent_backend_engineer", Some(6 * 24), Some(4 * 24), 86_000, 2),
    ("main_granite_cloud_staff_engineer", Some(10 * 24), Some(9 * 24), 102_000, 3),
    ("main_orchard_systems_architect", Some(16 * 24), Some(14 * 24), 83_000, 2),
    ("main_linden_analytics_data_lead", None, Some(35 * 24), 79_000, 2),
];

/// Words only the fictional CV's pages hold, so a test can tell they never reach the screen as text.
const CV_PAGE_WORDS: [&str; 2] = ["Verbena Quarry Freight", "Ostrava rail yard"];

/// What one page of the fictional CV draws: a name on the first, then sections, each a bar, a title and grey lines.
fn cv_page_content(page: usize) -> String {
    let mut content = String::new();
    let mut y = 770;
    if page == 0 {
        content.push_str(&format!("0.15 0.2 0.28 rg BT /F1 26 Tf 56 {y} Td (Sample Candidate) Tj ET\n"));
        content.push_str(&format!("BT /F1 11 Tf 56 {} Td (Lead Software Engineer at {}) Tj ET\n", y - 22, CV_PAGE_WORDS[0]));
        y -= 70;
    }
    let sections = [["Profile", "Experience", "Projects"], ["Experience", "Education", "Skills"], ["Languages", "References", "Projects"]];
    for (index, title) in sections[page % 3].iter().enumerate() {
        content.push_str(&format!("0.15 0.2 0.28 rg 56 {} 40 4 re f BT /F1 12 Tf 104 {} Td ({title}) Tj ET\n", y + 3, y));
        if index == 1 {
            content.push_str(&format!("BT /F1 9 Tf 104 {} Td ({}) Tj ET\n", y - 16, CV_PAGE_WORDS[1]));
        }
        y -= 28;
        for line in 0..5 {
            let width = 420 - (line * 37) % 120;
            content.push_str(&format!("0.82 0.86 0.91 rg 104 {y} {width} 4 re f\n"));
            y -= 11;
        }
        y -= 26;
    }
    content
}

/// The fictional CV as a PDF of `pages` pages in Helvetica, with `padding` bytes of comment lines after its header.
/// The cross-reference offsets are counted as the file is written.
fn cv_pdf_padded(pages: usize, padding: usize) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut left = padding;
    while left > 0 {
        let line = left.min(80);
        if line > 1 {
            out.push(b'%');
            out.extend(std::iter::repeat_n(b'-', line - 2));
        }
        out.push(b'\n');
        left -= line;
    }
    let kids: Vec<String> = (0..pages).map(|page| format!("{} 0 R", 4 + 2 * page)).collect();
    let mut objects = vec![
        "<</Type/Catalog/Pages 2 0 R>>".to_owned(),
        format!("<</Type/Pages/Kids[{}]/Count {pages}>>", kids.join(" ")),
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_owned(),
    ];
    for page in 0..pages {
        objects.push(format!("<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]/Contents {} 0 R/Resources<</Font<</F1 3 0 R>>>>>>", 5 + 2 * page));
        let content = cv_page_content(page);
        objects.push(format!("<</Length {}>>\nstream\n{content}\nendstream", content.len()));
    }
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes());
    out
}

/// The fictional CV of `pages` pages, padded to exactly `size` bytes. No pages gives a file that only starts like a PDF.
fn cv_pdf(pages: usize, size: usize) -> Vec<u8> {
    if pages == 0 {
        let mut bytes = b"%PDF-1.4\nThis file was cut short before its first object.\n".to_vec();
        bytes.resize(size, b'\n');
        return bytes;
    }
    let mut padding = 0;
    // The offsets grow with the padding, and only their digits can change the length again, so this settles at once.
    for _ in 0..4 {
        let bytes = cv_pdf_padded(pages, padding);
        if bytes.len() == size {
            return bytes;
        }
        padding = (padding + size).checked_sub(bytes.len()).expect("the CV fits in its size");
    }
    panic!("the CV did not settle at {size} bytes");
}

/// Writes `bytes` to `path`, changed `hours` before `cv_now`.
fn write_aged(path: &Path, bytes: &[u8], hours: i64) {
    use chrono::TimeZone;
    fs::write(path, bytes).unwrap();
    let at = chrono::Local.from_local_datetime(&(cv_now() - chrono::Duration::hours(hours))).unwrap();
    fs::File::options().write(true).open(path).unwrap().set_modified(at.into()).unwrap();
}

/// Empties the sample's `cv/`.
fn empty_cv(root: &Path) {
    for entry in fs::read_dir(root.join("cv")).unwrap().flatten() {
        fs::remove_file(entry.path()).unwrap();
    }
}

/// The command file that gives the folder `/build-cv`, which the sample does not hold.
const BUILD_CV: &str = ".claude/commands/build-cv.md";

/// Gives the folder `/build-cv`, so an out-of-date document offers its build.
fn add_build_cv(root: &Path) {
    fs::write(root.join(BUILD_CV), "Builds a CV PDF from its .tex.\n").unwrap();
}

/// Puts the canvas's nine documents in place of the sample's `cv/`, in a folder that has `/build-cv`. Each PDF is the
/// fictional CV, at its size.
fn fill_cv(root: &Path) {
    add_build_cv(root);
    empty_cv(root);
    for (stem, tex, built, size, pages) in CV_FILES {
        if let Some(hours) = tex {
            write_aged(&root.join(format!("cv/{stem}.tex")), b"\\documentclass{article}\n\\begin{document}\nSample Candidate\n\\end{document}\n", hours);
        }
        if let Some(hours) = built {
            write_aged(&root.join(format!("cv/{stem}.pdf")), &cv_pdf(pages, size), hours);
        }
    }
}

/// The sample with the canvas's nine CV documents, on Profile, its ages counted to `cv_now`.
fn cv_shell(name: &str) -> (Shell, std::path::PathBuf) {
    let root = sample_root(name);
    fill_cv(&root);
    cv_shell_on(root)
}

/// The same in a folder with no `/build-cv`.
fn cv_shell_without_build_cv(name: &str) -> (Shell, std::path::PathBuf) {
    let root = sample_root(name);
    fill_cv(&root);
    fs::remove_file(root.join(BUILD_CV)).unwrap();
    cv_shell_on(root)
}

/// The shell on a folder whose `cv/` a test has filled.
fn cv_shell_on(root: std::path::PathBuf) -> (Shell, std::path::PathBuf) {
    let snapshot = Snapshot::load(&root, day());
    assert!(snapshot.issues.is_empty(), "{:?}", snapshot.issues);
    (Shell::with_snapshot(Screen::Profile, jobdesk::workspace::open(&root).unwrap(), snapshot).with_now(cv_now()), root)
}

/// Opens CV documents from the rail.
fn open_cv(harness: &mut Harness) {
    pick_topic(harness, CV_DOCUMENTS);
}

/// The row under the topics counts what a build would help, and the sample's two PDFs with no `.tex` read as documents.
#[test]
fn the_cv_row_says_how_many_are_out_of_date() {
    let harness = tray_harness(cv_shell("profile_cv_row").0);
    assert!(is_shown(&harness, "CV documents, 2 out of date"));
    assert!(harness.query_all_by_label("7 of 11 filled").count() > 0, "the row stays out of the count");
    let harness = tray_harness(shell(Screen::Profile, "profile_cv_row_sample"));
    assert!(is_shown(&harness, "CV documents, 2 documents"));
}

/// Masters come first, then applications newest first, and only a tag that is not Current shows on a row.
#[test]
fn cv_documents_list_masters_then_applications() {
    let mut harness = tray_harness(cv_shell("profile_cv_list").0);
    open_cv(&mut harness);
    let rows = [
        "Master CV",
        "Master CV, engineer",
        "Halyard labs tech lead, Not built yet",
        "Tessel platform lead, Source newer",
        "Brightwell eng manager",
        "Ardent backend engineer",
        "Granite cloud staff engineer",
        "Orchard systems architect",
        "Linden analytics data lead",
    ];
    let tops: Vec<f32> = rows.iter().map(|row| harness.get_by_role_and_label(egui::accesskit::Role::Button, row).rect().top()).collect();
    assert!(tops.is_sorted(), "{tops:?}");
    assert!(is_shown(&harness, "main_example.pdf"), "the first document opens");
    assert!(is_shown(&harness, "84 KB, built 3 days ago"));
    assert!(is_shown(&harness, "Its .tex has not changed since this PDF was built."));
    assert!(is_shown(&harness, CV_SHOWS));
}

/// A row cuts a long label, and keyboard focus on the row shows the whole label, as the pointer does.
#[test]
fn a_cut_cv_label_shows_in_full_on_focus() {
    let mut harness = tray_harness(cv_shell("profile_cv_focus_tip").0);
    open_cv(&mut harness);
    let label = "Granite cloud staff engineer";
    let before = harness.query_all_by_label(label).count();
    harness.get_by_role_and_label(egui::accesskit::Role::Button, label).focus();
    harness.run_steps(4);
    assert!(harness.get_by_role_and_label(egui::accesskit::Role::Button, label).is_focused());
    assert_eq!(harness.query_all_by_label(label).count(), before + 1, "the tooltip shows the whole label");
}

/// Rebuild the PDF… opens the launcher with the document's stem, and the user starts the run there.
#[test]
fn rebuild_opens_the_launcher_and_starts_nothing() {
    let calls = SharedCalls::default();
    let (mut shell, _root) = cv_shell("profile_cv_rebuild");
    *shell.desk_slot() = Some(desk_on(RecordingGateway(SharedSink::default(), Arc::clone(&calls))));
    let mut harness = tray_harness(shell);
    open_cv(&mut harness);
    harness.get_by_label("Tessel platform lead, Source newer").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Its .tex changed 3 hours ago, after this PDF was built. Run /build-cv in the launcher to build it again."));
    let rebuild = harness.get_by_role_and_label(egui::accesskit::Role::Button, REBUILD);
    rebuild.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "Opens the launcher with /build-cv main_tessel_platform_lead. Nothing runs until you start it there."));

    harness.get_by_role_and_label(egui::accesskit::Role::Button, REBUILD).click();
    harness.run_steps(3);
    assert!(is_shown(&harness, LAUNCHER), "the launcher opens");
    assert!(harness.query_all_by_label("/build-cv main_tessel_platform_lead").count() > 0, "the line is filled in");
    assert_eq!(calls.lock().unwrap().starts, 0, "no run starts");
}

/// While a run works, the build waits and says why.
#[test]
fn rebuild_waits_while_a_run_works() {
    let root = sample_root("profile_cv_busy");
    fill_cv(&root);
    let (shell, _root, _sink, _feed) = profile_with_run_on(root);
    let mut harness = tray_harness(shell.with_now(cv_now()));
    open_cv(&mut harness);
    harness.get_by_label("Tessel platform lead, Source newer").click();
    harness.run_steps(3);
    let rebuild = harness.get_by_role_and_label(egui::accesskit::Role::Button, REBUILD);
    assert!(rebuild.accesskit_node().is_disabled(), "a run works");
    rebuild.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "Another run is active. One run at a time keeps your files safe."));
}

/// A stem that would not read as one word on the command line gets no build to open.
#[test]
fn a_stem_that_cannot_be_typed_gets_no_build() {
    let root = sample_root("profile_cv_bad_stem");
    add_build_cv(&root);
    empty_cv(&root);
    write_aged(&root.join("cv/main_bad name.tex"), b"\\documentclass{article}\n", 1);
    let shell = Shell::with_snapshot(Screen::Profile, jobdesk::workspace::open(&root).unwrap(), Snapshot::load(&root, day())).with_now(cv_now());
    let mut harness = tray_harness(shell);
    open_cv(&mut harness);
    assert!(is_shown(&harness, "Bad name, Not built yet"), "the document is listed");
    assert!(is_shown(&harness, "Its .tex has no PDF beside it yet. Run /build-cv in the launcher to build one."));
    assert!(harness.query_by_label("Build the PDF…").is_none(), "no build for a stem with a space");
}

/// What an out-of-date document says in a folder with no `/build-cv`.
const NO_BUILD_CV: &str = "This folder has no /build-cv command, so update your ai-job-search checkout to a version that has it.";

/// A folder with no `/build-cv` keeps each tag and the rail word, says why, and offers no build, disabled or not.
#[test]
fn a_folder_with_no_build_cv_offers_no_build() {
    let mut harness = tray_harness(cv_shell_without_build_cv("profile_cv_no_command").0);
    assert!(is_shown(&harness, "CV documents, 2 out of date"), "the rail word is unchanged");
    open_cv(&mut harness);
    assert!(is_shown(&harness, "Its .tex has not changed since this PDF was built."), "a Current line is unchanged");
    harness.get_by_label("Tessel platform lead, Source newer").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, &format!("Its .tex changed 3 hours ago, after this PDF was built. {NO_BUILD_CV}")));
    assert!(harness.query_by_label(REBUILD).is_none(), "no build without the command");
    assert!(is_shown(&harness, "Open PDF"));
    harness.get_by_label("Halyard labs tech lead, Not built yet").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, &format!("Its .tex has no PDF beside it yet. {NO_BUILD_CV}")));
    assert!(harness.query_by_label("Build the PDF…").is_none());
    assert!(is_shown(&harness, if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in Explorer" }));
}

/// While a run works, the no-command line stays as it is: there is no build to disable.
#[test]
fn a_folder_with_no_build_cv_says_the_same_while_a_run_works() {
    let root = sample_root("profile_cv_no_command_busy");
    fill_cv(&root);
    fs::remove_file(root.join(BUILD_CV)).unwrap();
    let (shell, _root, _sink, _feed) = profile_with_run_on(root);
    let mut harness = tray_harness(shell.with_now(cv_now()));
    open_cv(&mut harness);
    harness.get_by_label("Tessel platform lead, Source newer").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, &format!("Its .tex changed 3 hours ago, after this PDF was built. {NO_BUILD_CV}")));
    assert!(harness.query_by_label(REBUILD).is_none());
}

/// A checkout update that adds `/build-cv` brings the build back on the next snapshot.
#[test]
fn adding_build_cv_brings_the_build_back() {
    let (mut shell, root) = cv_shell_without_build_cv("profile_cv_command_added");
    let feed = shell.snapshot_feed();
    let mut harness = tray_harness(shell);
    open_cv(&mut harness);
    harness.get_by_label("Tessel platform lead, Source newer").click();
    harness.run_steps(3);
    assert!(harness.query_by_label(REBUILD).is_none());

    add_build_cv(&root);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Its .tex changed 3 hours ago, after this PDF was built. Run /build-cv in the launcher to build it again."));
    assert!(harness.query_by_role_and_label(egui::accesskit::Role::Button, REBUILD).is_some(), "the build is back");
}

/// A draft with unsaved changes asks before CV documents open in its place.
#[test]
fn a_draft_asks_before_cv_documents_open() {
    let (shell, _root) = profile_at("profile_edit_leave_for_cv");
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    open_cv(&mut harness);
    assert!(is_shown(&harness, DISCARD_ASK));
    assert!(harness.query_by_label_contains("Opening CV documents drops them.").is_some());
    button(&harness, "Keep editing").click();
    harness.run_steps(3);
    assert!(field(&harness).value().unwrap_or_default().ends_with(BUDGET_FILLED), "the draft stays");
    assert!(!is_shown(&harness, CV_SHOWS));

    open_cv(&mut harness);
    button(&harness, "Discard changes").click();
    harness.run_steps(3);
    assert!(!editing(&harness));
    assert!(is_shown(&harness, CV_SHOWS), "CV documents opened");
}

/// No `.tex` or PDF in `cv/`: no list, and a note on what fills it.
#[test]
fn profile_cv_empty_light() {
    let root = sample_root("profile_cv_empty_light");
    empty_cv(&root);
    let shell = Shell::with_snapshot(Screen::Profile, jobdesk::workspace::open(&root).unwrap(), Snapshot::load(&root, day()));
    let mut harness = probed_wgpu(probed(shell, FixedTools), Theme::Light);
    open_cv(&mut harness);
    assert!(is_shown(&harness, "No CV documents in your folder yet"));
    assert!(is_shown(&harness, "CV documents, None yet"));
    compare(&mut harness, "profile_cv_empty_light");
}

/// `cv/` cannot be read: the alert, the row Hidden, and hidden, not gone on paper.
#[test]
fn profile_cv_unread_light() {
    let mut harness = probed_wgpu(probed(broken_shell(Screen::Profile, "profile_cv_unread_light", &[Source::CvDocuments]), FixedTools), Theme::Light);
    open_cv(&mut harness);
    assert!(is_shown(&harness, "Your CV documents are hidden, not gone"));
    assert!(is_shown(&harness, "CV documents, Hidden"));
    compare(&mut harness, "profile_cv_unread_light");
}

const RENDERING: &str = "Rendering the pages…";
const BRIGHTWELL: &str = "Brightwell eng manager";
const NOT_RENDERED: &str = "This PDF could not be rendered.";

/// Steps until the picked document's pages are no longer rendering, ten seconds at most.
fn rendered(harness: &mut Harness) {
    let start = std::time::Instant::now();
    harness.run_steps(1);
    while is_shown(harness, RENDERING) && start.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.run_steps(1);
    }
    harness.run_steps(3);
}

/// Picks the document whose row reads `row`, then takes the pointer off the window, so no hover shows.
fn pick_cv(harness: &mut Harness, row: &str) {
    harness.get_by_role_and_label(egui::accesskit::Role::Button, row).click();
    harness.run_steps(3);
    harness.event(egui::Event::PointerGone);
    harness.run_steps(2);
}

/// The renderer the app uses, for a shell that renders PDF pages.
fn page_store() -> jobdesk::profile::application::PageStore {
    jobdesk::dossier::page_store(Arc::new(|| {}))
}

/// CV documents on a rendering harness. Brightwell renders first, so its row says Render failed as the canvas shows,
/// then the document whose row reads `row` opens, and the scene waits for its pages.
fn cv_scene(shell: Shell, theme: Theme, row: &str) -> Harness<'static> {
    let mut harness = probed_wgpu(probed(shell, FixedTools), theme);
    open_cv(&mut harness);
    pick_cv(&mut harness, BRIGHTWELL);
    rendered(&mut harness);
    assert!(is_shown(&harness, NOT_RENDERED), "Brightwell's PDF is one no renderer reads");
    pick_cv(&mut harness, row);
    rendered(&mut harness);
    harness
}

fn render_cv(theme: Theme, row: &str, name: &str) -> Harness<'static> {
    let mut harness = cv_scene(cv_shell(name).0.with_page_store(page_store()), theme, row);
    compare(&mut harness, name);
    harness
}

/// Master CV, Current, its two pages side by side, and each tag in the list.
#[test]
fn profile_cv_full_light() {
    let harness = render_cv(Theme::Light, "Master CV", "profile_cv_full_light");
    assert!(is_shown(&harness, "2 pages, 84 KB, built 3 days ago"), "the page count comes from the render");
    assert!(is_shown(&harness, "Page 1 of 2") && is_shown(&harness, "Page 2 of 2"));
    assert!(is_shown(&harness, "Brightwell eng manager, Render failed"), "a refused render stays on its row");
}

#[test]
fn profile_cv_full_dark() {
    render_cv(Theme::Dark, "Master CV", "profile_cv_full_dark");
}

/// An application's `.tex` changed after its PDF: Source newer, and Rebuild the PDF….
#[test]
fn profile_cv_source_light() {
    render_cv(Theme::Light, "Tessel platform lead, Source newer", "profile_cv_source_light");
}

/// The same while a run works: Rebuild the PDF… waits.
#[test]
fn profile_cv_busy_light() {
    let root = sample_root("profile_cv_busy_light");
    fill_cv(&root);
    let (shell, _root, _sink, _feed) = profile_with_run_on(root);
    let mut harness = cv_scene(shell.with_now(cv_now()).with_page_store(page_store()), Theme::Light, "Tessel platform lead, Source newer");
    assert!(harness.get_by_role_and_label(egui::accesskit::Role::Button, REBUILD).accesskit_node().is_disabled());
    compare(&mut harness, "profile_cv_busy_light");
}

/// A `.tex` with no PDF: Not built yet, an empty page, Show in Finder only, and Build the PDF….
#[test]
fn profile_cv_missing_light() {
    let harness = render_cv(Theme::Light, "Halyard labs tech lead, Not built yet", "profile_cv_missing_light");
    assert!(is_shown(&harness, "No PDF yet. It shows here once /build-cv has built it."));
    assert!(!is_shown(&harness, "Open PDF"), "there is no PDF to open");
    assert!(is_shown(&harness, "Build the PDF…"));
}

/// A PDF with no `.tex`: Current, and nothing to build it from.
#[test]
fn profile_cv_notex_light() {
    let harness = render_cv(Theme::Light, "Linden analytics data lead", "profile_cv_notex_light");
    assert!(is_shown(&harness, "No .tex sits beside this PDF, so there is nothing to build it from again."));
    assert!(is_shown(&harness, "Open PDF"));
}

/// A PDF the renderer refused: Render failed, and its reason in a closed box.
#[test]
fn profile_cv_failed_light() {
    let harness = render_cv(Theme::Light, "Brightwell eng manager, Render failed", "profile_cv_failed_light");
    assert!(is_shown(&harness, NOT_RENDERED));
    assert!(is_shown(&harness, "The file is there, but Jobdesk could not draw its pages."));
    assert!(is_shown(&harness, "88 KB, built yesterday"), "no page count for pages that never rendered");
    assert!(is_shown(&harness, "Open PDF"), "the system's viewer may still open it");
}

/// The folder has no `/build-cv`: the application CV keeps Source newer, says the command is missing, and offers no build.
#[test]
fn profile_cv_nocommand_light() {
    let shell = cv_shell_without_build_cv("profile_cv_nocommand_light").0.with_page_store(page_store());
    let mut harness = cv_scene(shell, Theme::Light, "Tessel platform lead, Source newer");
    assert!(is_shown(&harness, &format!("Its .tex changed 3 hours ago, after this PDF was built. {NO_BUILD_CV}")));
    assert!(harness.query_by_label(REBUILD).is_none());
    assert!(is_shown(&harness, "Halyard labs tech lead, Not built yet"), "the list still tags it");
    compare(&mut harness, "profile_cv_nocommand_light");
}

/// Refuses Brightwell's PDF at once, and renders no other.
struct Stuck;

impl jobdesk::dossier::application::PageRasterizer for Stuck {
    fn render(&self, path: &Path) -> Result<jobdesk::profile::application::RenderedPages, String> {
        if path.to_string_lossy().contains("brightwell") {
            return Err("The file is not a PDF the renderer can read.".into());
        }
        loop {
            std::thread::park();
        }
    }
}

/// The list and head show at once, and the pages are skeletons while they render.
#[test]
fn profile_cv_loading_light() {
    let store = jobdesk::profile::application::PageStore::new(Arc::new(Stuck), Arc::new(|| {}));
    let shell = cv_shell("profile_cv_loading_light").0.with_page_store(store);
    let mut harness = probed_wgpu(probed(shell, FixedTools), Theme::Light);
    open_cv(&mut harness);
    pick_cv(&mut harness, BRIGHTWELL);
    rendered(&mut harness);
    pick_cv(&mut harness, "Granite cloud staff engineer");
    harness.run_steps(3);
    assert!(is_shown(&harness, RENDERING));
    assert!(is_shown(&harness, "102 KB, built 9 days ago"), "no page count before the render");
    assert!(!is_shown(&harness, "Page 1 of 2"));
    compare(&mut harness, "profile_cv_loading_light");
}

/// Pages reach the screen as images only: the one text a page gives assistive technology is its alt, never the PDF's words.
#[test]
fn cv_pages_reach_the_screen_as_images_only() {
    let mut harness = tray_harness(cv_shell("profile_cv_images_only").0.with_page_store(page_store()));
    open_cv(&mut harness);
    rendered(&mut harness);
    assert!(is_shown(&harness, "Master CV page 1") && is_shown(&harness, "Master CV page 2"));
    for words in CV_PAGE_WORDS.into_iter().chain(["Sample Candidate"]) {
        assert!(harness.query_by_label_contains(words).is_none(), "{words} stays in the page image");
    }
    let images = harness.query_all_by_role(egui::accesskit::Role::Image).filter_map(|node| node.accesskit_node().label());
    let alts: Vec<String> = images.filter(|alt| alt != "Jobdesk mark").collect();
    assert_eq!(alts, ["Master CV page 1", "Master CV page 2"]);
}

/// A PDF that `/build-cv` rewrites renders again, and its page count follows.
#[test]
fn a_rebuilt_cv_pdf_renders_again() {
    let (shell, root) = cv_shell("profile_cv_rebuilt");
    let mut shell = shell.with_page_store(page_store());
    let feed = shell.snapshot_feed();
    let mut harness = tray_harness(shell);
    open_cv(&mut harness);
    rendered(&mut harness);
    assert!(is_shown(&harness, "2 pages, 84 KB, built 3 days ago"));
    write_aged(&root.join("cv/main_example.pdf"), &cv_pdf(3, 84_000), 1);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    rendered(&mut harness);
    assert!(is_shown(&harness, "3 pages, 84 KB, built 1 hour ago"), "the new build rendered");
    assert!(is_shown(&harness, "Page 3 of 3"));
}

/// Master CV, its PDF replaced by `bytes`, opened on a shell that renders it. The feed hands the shell new snapshots.
fn master_cv_as(name: &str, bytes: &[u8]) -> (Harness<'static>, std::path::PathBuf, std::sync::mpsc::Sender<Snapshot>) {
    let (shell, root) = cv_shell(name);
    write_aged(&root.join("cv/main_example.pdf"), bytes, 3 * 24);
    let mut shell = shell.with_page_store(page_store());
    let feed = shell.snapshot_feed();
    feed.send(Snapshot::load(&root, day())).unwrap();
    let mut harness = tray_harness(shell);
    open_cv(&mut harness);
    rendered(&mut harness);
    (harness, root, feed)
}

/// A PDF longer than the render shows its first pages, and never passes them off as the whole.
#[test]
fn a_long_cv_says_only_its_first_pages_show() {
    let (harness, _root, _feed) = master_cv_as("profile_cv_long", &cv_pdf(8, 84_000));
    assert!(is_shown(&harness, "8 pages, the first 6 shown, 84 KB, built 3 days ago"));
    assert!(is_shown(&harness, "Page 6 of 8"));
    assert!(!is_shown(&harness, "Page 6 of 6") && !is_shown(&harness, "Page 7 of 8"));
}

/// A PDF with no page is no CV to show: the render failed, with the reason.
#[test]
fn a_cv_with_no_pages_failed_to_render() {
    let (harness, _root, _feed) = master_cv_as("profile_cv_no_pages", &cv_pdf_padded(0, 0));
    assert!(is_shown(&harness, NOT_RENDERED));
    assert!(is_shown(&harness, "This PDF has no pages."));
    assert!(is_shown(&harness, "Master CV, Render failed"));
    assert!(harness.query_by_label_contains("0 pages").is_none());
}

/// A PDF read while `/build-cv` was still writing it renders once the folder is read again, even when the finished file
/// keeps the same modification time to the second.
#[test]
fn a_cv_read_half_written_renders_once_whole() {
    let whole = cv_pdf(2, 84_000);
    let (shell, root) = cv_shell("profile_cv_half_written");
    let path = root.join("cv/main_example.pdf");
    // Written a few seconds ago, and read before the task finished writing it.
    let written = std::time::SystemTime::now() - std::time::Duration::from_secs(5);
    fs::write(&path, &whole[..whole.len() / 3]).unwrap();
    fs::File::options().write(true).open(&path).unwrap().set_modified(written).unwrap();
    let mut shell = shell.with_page_store(page_store());
    let feed = shell.snapshot_feed();
    feed.send(Snapshot::load(&root, day())).unwrap();
    let mut harness = tray_harness(shell);
    open_cv(&mut harness);
    rendered(&mut harness);
    assert!(is_shown(&harness, NOT_RENDERED), "the half-written file failed");

    // The task finishes within the same second, so the file keeps the time the failed read saw.
    fs::write(&path, &whole).unwrap();
    fs::File::options().write(true).open(&path).unwrap().set_modified(written).unwrap();
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    rendered(&mut harness);
    assert!(!is_shown(&harness, NOT_RENDERED), "the failure did not stick");
    assert!(is_shown(&harness, "Page 2 of 2"));
}

/// A PDF that failed and has not changed for a while stays failed when the folder is read again, so it does not render
/// over and over.
#[test]
fn an_old_cv_that_failed_stays_failed() {
    let (mut harness, root, feed) = master_cv_as("profile_cv_old_failure", b"%PDF-1.4\nnot a PDF after all\n");
    assert!(is_shown(&harness, NOT_RENDERED));
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    assert!(!is_shown(&harness, RENDERING), "no new render started");
    assert!(is_shown(&harness, NOT_RENDERED));
}

/// A broken PDF written a few seconds ago is tried once more, then stays failed however often a task makes the folder
/// read again, so the pages area does not flip between loading and failed.
#[test]
fn a_broken_cv_just_written_is_tried_again_only_once() {
    let (shell, root) = cv_shell("profile_cv_broken_just_written");
    let path = root.join("cv/main_example.pdf");
    fs::write(&path, b"%PDF-1.4\nnot a PDF after all\n").unwrap();
    fs::File::options().write(true).open(&path).unwrap().set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(5)).unwrap();
    let mut shell = shell.with_page_store(page_store());
    let feed = shell.snapshot_feed();
    feed.send(Snapshot::load(&root, day())).unwrap();
    let mut harness = tray_harness(shell);
    open_cv(&mut harness);
    rendered(&mut harness);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(1);
    assert!(is_shown(&harness, RENDERING), "the one retry");
    rendered(&mut harness);
    for _ in 0..3 {
        feed.send(Snapshot::load(&root, day())).unwrap();
        harness.run_steps(1);
        assert!(!is_shown(&harness, RENDERING), "no second retry");
        assert!(is_shown(&harness, NOT_RENDERED));
    }
}

/// The last line of the sample's first role, and the same with its blank filled in.
const BUDGET: &str = "- Budget or headcount you owned: [PLACEHOLDER]";
const BUDGET_FILLED: &str = "- Budget or headcount you owned: EUR 1.2M a year, eight people at the peak";
const SAVE_WAITS: &str = "A task is running and may change this file. Save once it ends.";
const SAVE_WAITS_LINE: &str = "A task is running, so Save waits until it ends. Your text stays here.";
const DISCARD_ASK: &str = "Discard your changes?";
const SAVE_OVER: &str = "Save mine over it";

fn field<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
    harness.get_by_role(egui::accesskit::Role::MultilineTextInput)
}

fn editing(harness: &Harness) -> bool {
    harness.query_by_role(egui::accesskit::Role::MultilineTextInput).is_some()
}

fn button<'a>(harness: &'a Harness<'static>, label: &'a str) -> egui_kittest::Node<'a> {
    harness.get_by_role_and_label(egui::accesskit::Role::Button, label)
}

/// Scrolls the paper to the button, then clicks it.
fn press_button(harness: &mut Harness<'static>, label: &str) {
    button(harness, label).scroll_to_me();
    harness.run_steps(3);
    button(harness, label).click();
    harness.run_steps(3);
}

/// Opens the first role, Lead Software Engineer, in the editor.
fn edit_first_role(harness: &mut Harness<'static>) {
    harness.get_all_by_label("Edit").next().expect("the first role's Edit").click();
    harness.run_steps(3);
    assert!(editing(harness), "the role opened in the editor");
}

/// Opens the first role and types its budget over the blank at the end of the draft.
fn fill_the_budget(harness: &mut Harness<'static>) {
    edit_first_role(harness);
    field(harness).focus();
    harness.run_steps(1);
    for _ in 0.."[PLACEHOLDER]".len() {
        harness.key_press(Key::Backspace);
    }
    field(harness).type_text("EUR 1.2M a year, eight people at the peak");
    harness.run_steps(3);
    assert!(field(harness).value().unwrap_or_default().ends_with(BUDGET_FILLED), "{:?}", field(harness).value());
}

fn candidate(root: &Path) -> String {
    fs::read_to_string(root.join(CANDIDATE)).unwrap()
}

/// The sample on Profile with no run, at a fixed place whose files a test can read.
fn profile_at(name: &str) -> (Shell, std::path::PathBuf) {
    let root = sample_root(name);
    (Shell::with_snapshot(Screen::Profile, jobdesk::workspace::open(&root).unwrap(), Snapshot::load(&root, day())), root)
}

/// The first role as the sample holds it, with its budget filled in.
fn first_role_filled(text: &str) -> String {
    let start = text.find("### Lead Software Engineer").unwrap();
    let end = text.find(BUDGET).unwrap() + BUDGET.len();
    text[start..end].replace(BUDGET, BUDGET_FILLED)
}

/// Save replaces the role's lines, keeps every other byte, and the saved text shows at once.
#[test]
fn a_saved_role_changes_only_its_lines() {
    let (shell, root) = profile_at("profile_edit_save");
    let before = candidate(&root);
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    assert!(is_shown(&harness, "Unsaved changes"));
    assert!(harness.get_all_by_label("Edit").all(|edit| edit.accesskit_node().is_disabled()), "one edit at a time");
    assert!(button(&harness, "Add a role").accesskit_node().is_disabled());

    cmd_enter(&mut harness);
    assert_eq!(candidate(&root), before.replace(BUDGET, BUDGET_FILLED));
    assert!(!editing(&harness), "the editor closed");
    assert!(harness.query_by_label_contains("EUR 1.2M a year").is_some(), "the saved text shows without waiting for the folder watcher");
    assert!(harness.get_all_by_label("Edit").all(|edit| !edit.accesskit_node().is_disabled()));
}

/// A snapshot the folder watcher read before the save never shows the old text again.
#[test]
fn a_snapshot_read_before_a_save_keeps_the_saved_text() {
    let (mut shell, root) = profile_at("profile_edit_stale");
    let feed = shell.snapshot_feed();
    let mut harness = tray_harness(shell);
    let early = Snapshot::load(&root, day());
    fill_the_budget(&mut harness);
    cmd_enter(&mut harness);
    feed.send(early).unwrap();
    harness.run_steps(3);
    assert!(harness.query_by_label_contains("EUR 1.2M a year").is_some(), "the early snapshot keeps the text read back");
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    assert!(harness.query_by_label_contains("EUR 1.2M a year").is_some());
}

/// While a run works, Save waits and says why. The draft outlives the run, and Save writes once the run's last load is in.
#[test]
fn save_waits_while_a_run_works_and_the_draft_survives_it() {
    let (shell, root, sink, feed) = profile_with_run("profile_edit_busy");
    let before = candidate(&root);
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    assert!(is_shown(&harness, SAVE_WAITS_LINE));
    let save = button(&harness, "Save");
    assert!(save.accesskit_node().is_disabled());
    save.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, SAVE_WAITS));
    cmd_enter(&mut harness);
    assert_eq!(candidate(&root), before, "nothing was written");

    end_run(&sink);
    harness.run_steps(3);
    assert!(button(&harness, "Save").accesskit_node().is_disabled(), "the run's changes are not in yet");
    assert!(is_shown(&harness, "Reading the files the task changed, so Save waits a moment. Your text stays here."), "the task has ended");
    assert!(!is_shown(&harness, SAVE_WAITS_LINE));
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    assert!(field(&harness).value().unwrap_or_default().ends_with(BUDGET_FILLED), "the draft stays");
    assert!(!is_shown(&harness, SAVE_WAITS_LINE));
    press_button(&mut harness, "Save");
    assert_eq!(candidate(&root), before.replace(BUDGET, BUDGET_FILLED));
}

/// A folder update that works, as an idle one showing a download keeps working with no worker to end it.
fn working_folder_update() -> jobdesk::folder_update::application::FolderUpdate {
    idle_folder_update("0.4.0").showing(Stage::Working(Phase::Downloading { received: 0 }), false)
}

const FOLDER_SAVE_WAITS_LINE: &str = "Jobdesk is updating your job search folder, so Save waits until it ends. Your text stays here.";

/// While Jobdesk updates the job search folder, Save waits and says why, so neither write is lost. It writes once the
/// update has ended.
#[test]
fn save_waits_while_the_folder_updates() {
    let (shell, root) = profile_at("profile_edit_folder_update");
    let before = candidate(&root);
    let (mut harness, shared) = shared_harness(shell.with_folder_update(working_folder_update()));
    fill_the_budget(&mut harness);
    cmd_enter(&mut harness);
    assert_eq!(candidate(&root), before, "nothing was written");
    assert!(is_shown(&harness, FOLDER_SAVE_WAITS_LINE));
    assert!(button(&harness, "Save").accesskit_node().is_disabled());

    *shared.borrow_mut().folder_update_slot() = idle_folder_update("0.4.0");
    harness.run_steps(3);
    press_button(&mut harness, "Save");
    assert_eq!(candidate(&root), before.replace(BUDGET, BUDGET_FILLED), "the draft is saved once the update has ended");
}

/// A reply that starts a turn while the editor is open turns Save off again, and the draft waits for the turn.
#[test]
fn a_reply_that_starts_a_turn_holds_the_save() {
    let (shell, root, sink, feed) = profile_with_run("profile_edit_reply");
    let before = candidate(&root);
    let (mut harness, shell) = shared_harness(shell);
    emit_all(&sink, [AgentEvent::TurnEnded { failed: false, message: Some("Anything else?".into()), cost_usd: 0.4, duration_ms: 40_000 }]);
    harness.run_steps(3);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    fill_the_budget(&mut harness);
    assert!(!button(&harness, "Save").accesskit_node().is_disabled(), "the run waits for a reply");

    let id = shell.borrow_mut().desk_slot().as_ref().unwrap().runs()[0].id.clone();
    shell.borrow_mut().desk_slot().as_mut().unwrap().reply(&id, "Add my certifications too.").unwrap();
    harness.run_steps(3);
    assert!(button(&harness, "Save").accesskit_node().is_disabled());
    cmd_enter(&mut harness);
    assert_eq!(candidate(&root), before, "nothing was written");

    end_run(&sink);
    harness.run_steps(3);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    assert!(field(&harness).value().unwrap_or_default().ends_with(BUDGET_FILLED), "the draft stays");
    cmd_enter(&mut harness);
    assert_eq!(candidate(&root), before.replace(BUDGET, BUDGET_FILLED));
}

/// A role changed on disk shows its new lines, and Save becomes Save mine over it, which writes the draft over them.
#[test]
fn a_role_changed_on_disk_is_written_over_only_when_asked() {
    let (shell, root) = profile_at("profile_edit_conflict");
    let before = candidate(&root);
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    rewrite(&root, CANDIDATE, "update within a minute.\n", "update within a minute.\n- Introduced a weekly planning review with depot managers.\n");
    let theirs = candidate(&root);
    cmd_enter(&mut harness);

    assert_eq!(candidate(&root), theirs, "nothing was written");
    assert!(harness.query_by_label_contains("This role changed while you were editing.").is_some());
    assert!(is_shown(&harness, "- Introduced a weekly planning review with depot managers."), "the new line shows");
    assert!(is_shown(&harness, "Changed on disk just now"));
    assert!(harness.query_by_role_and_label(egui::accesskit::Role::Button, "Save").is_none());
    let foot = harness.get_by_label("Changed on disk just now").rect();
    assert!(button(&harness, SAVE_OVER).rect().bottom() <= foot.top(), "Save mine over it scrolled into view");
    press_button(&mut harness, SAVE_OVER);
    assert_eq!(candidate(&root), before.replace(BUDGET, BUDGET_FILLED), "the draft replaced the role as the file held it");
    assert!(!editing(&harness));
}

/// Use the new version takes the file's text, and the next Save compares with it.
#[test]
fn use_the_new_version_takes_the_files_text() {
    let (shell, root) = profile_at("profile_edit_theirs");
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    rewrite(&root, CANDIDATE, "update within a minute.\n", "update within a minute.\n- Introduced a weekly planning review with depot managers.\n");
    cmd_enter(&mut harness);
    press_button(&mut harness, "Use the new version");
    let draft = field(&harness).value().unwrap_or_default();
    assert!(draft.contains("- Introduced a weekly planning review with depot managers.") && draft.ends_with(BUDGET), "{draft}");
    assert!(!button(&harness, "Save").accesskit_node().is_disabled(), "Save is plain Save again");
    assert!(harness.query_by_label_contains("changed while you were editing").is_none());
}

/// A role renamed on disk is gone under its old name. The box says so, offers no new version, and Save mine over it
/// writes the draft back at the end of its section.
#[test]
fn a_role_gone_from_the_file_is_written_back_only_when_asked() {
    let (shell, root) = profile_at("profile_edit_gone");
    let before = candidate(&root);
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    rewrite(&root, CANDIDATE, "### Lead Software Engineer - Calder Freight", "### Staff Engineer - Calder Freight");
    let renamed = candidate(&root);
    cmd_enter(&mut harness);

    assert_eq!(candidate(&root), renamed, "nothing was written");
    assert!(harness.query_by_label_contains("This role is no longer in the file.").is_some());
    assert!(harness.query_by_label("Use the new version").is_none(), "there is no new version to take");
    press_button(&mut harness, SAVE_OVER);
    let after = candidate(&root);
    let role = first_role_filled(&before);
    assert!(after.contains(&format!("{role}\n\n## Independent Projects")), "{after}");
    assert_eq!(after.replace(&format!("{role}\n\n"), ""), renamed);
}

/// Picking another tab with unsaved text asks first. Keep editing is the default and stays on Profile.
#[test]
fn leaving_the_tab_with_a_draft_asks_first() {
    let mut harness = tray_harness(probed(shell(Screen::Profile, "profile_edit_leave_tab"), FixedTools));
    settle(&mut harness);
    fill_the_budget(&mut harness);
    let setup = |harness: &Harness<'static>| harness.get_by_role_and_label(egui::accesskit::Role::Tab, "Setup").click();
    setup(&harness);
    harness.run_steps(3);
    assert!(is_shown(&harness, DISCARD_ASK));
    assert!(button(&harness, "Keep editing").is_focused(), "Keep editing is the default");
    button(&harness, "Keep editing").click();
    harness.run_steps(3);
    assert!(!is_shown(&harness, DISCARD_ASK));
    assert!(is_shown(&harness, PROFILE_SUBTITLE), "Profile stays");
    assert!(field(&harness).value().unwrap_or_default().ends_with(BUDGET_FILLED), "the draft stays");

    setup(&harness);
    harness.run_steps(3);
    press(&mut harness, Key::Escape);
    assert!(!is_shown(&harness, DISCARD_ASK) && editing(&harness), "Esc keeps editing");

    setup(&harness);
    harness.run_steps(3);
    button(&harness, "Discard changes").click();
    harness.run_steps(3);
    assert!(!is_shown(&harness, PROFILE_SUBTITLE), "Setup is on show");
    harness.get_by_role_and_label(egui::accesskit::Role::Tab, "Profile").click();
    harness.run_steps(3);
    assert!(!editing(&harness), "the draft was dropped");
}

/// Picking another topic with unsaved text asks first too.
#[test]
fn leaving_the_topic_with_a_draft_asks_first() {
    let (shell, _root) = profile_at("profile_edit_leave_topic");
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    pick_topic(&mut harness, "Education");
    assert!(is_shown(&harness, DISCARD_ASK));
    button(&harness, "Keep editing").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "5 roles. Claude picks from these for each CV."), "Experience stays");
    assert!(field(&harness).value().unwrap_or_default().ends_with(BUDGET_FILLED));

    pick_topic(&mut harness, "Education");
    button(&harness, "Discard changes").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Your degrees and courses. Claude lists them on each CV."));
    assert!(!editing(&harness));
}

/// The first Esc leaves the field, and the second closes the editor. With unsaved text the second asks first.
#[test]
fn esc_leaves_the_field_then_closes_the_editor() {
    let (shell, _root) = profile_at("profile_edit_esc");
    let mut harness = tray_harness(shell);
    edit_first_role(&mut harness);
    field(&harness).focus();
    harness.run_steps(1);
    press(&mut harness, Key::Escape);
    assert!(editing(&harness) && !field(&harness).is_focused(), "the field let go and the editor stays");
    press(&mut harness, Key::Escape);
    assert!(!editing(&harness), "the second Esc closed it");

    fill_the_budget(&mut harness);
    press(&mut harness, Key::Escape);
    press(&mut harness, Key::Escape);
    assert!(is_shown(&harness, DISCARD_ASK), "unsaved text asks first");
    button(&harness, "Discard changes").click();
    harness.run_steps(3);
    assert!(!editing(&harness));
}

/// A save is the user's own, so it never puts Updated on a topic, even when a run starts and ends right after it.
#[test]
fn a_save_never_marks_a_topic() {
    let (shell, root, sink, feed) = profile_with_run("profile_edit_marks");
    let (mut harness, shell) = shared_harness(shell);
    end_run(&sink);
    harness.run_steps(3);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    fill_the_budget(&mut harness);
    cmd_enter(&mut harness);
    assert_eq!(candidate(&root).matches(BUDGET_FILLED).count(), 1, "saved");

    let options = StartOptions { model: None, budget_usd: 2.0 };
    shell.borrow_mut().desk_slot().as_mut().unwrap().start("/setup --section skills", &options, run_started()).unwrap();
    harness.run_steps(3);
    end_run(&sink);
    harness.run_steps(3);
    feed.send(Snapshot::load(&root, day())).unwrap();
    harness.run_steps(3);
    assert!(!is_shown(&harness, "Updated"), "a save marks nothing");
}

/// Add a role opens a new role after the last one, and Save puts it there.
#[test]
fn add_a_role_goes_after_the_last_role() {
    let (shell, root) = profile_at("profile_edit_add_role");
    let before = candidate(&root);
    let mut harness = tray_harness(shell);
    press_button(&mut harness, "Add a role");
    assert_eq!(field(&harness).value().as_deref(), Some("### Title - Company (dates)"));
    field(&harness).focus();
    harness.run_steps(1);
    field(&harness).type_text(" and more");
    harness.run_steps(3);
    cmd_enter(&mut harness);
    let after = candidate(&root);
    let added = "### Title - Company (dates) and more";
    assert_eq!(after, before.replace("cultural venues.\n\n## Independent Projects", &format!("cultural venues.\n\n{added}\n\n## Independent Projects")));
}

/// A topic of one section edits it from its head, and a topic of several from each section's sub-heading.
#[test]
fn edit_section_sits_where_the_sections_are() {
    let (shell, _root) = profile_at("profile_edit_sections");
    let mut harness = tray_harness(shell);
    assert_eq!(harness.query_all_by_label("Edit section").count(), 2, "Roles and the summary in CLAUDE.md");
    pick_topic(&mut harness, "References");
    assert_eq!(harness.query_all_by_label("Edit section").count(), 1, "one section, in the head");
    harness.get_by_label("Edit section").click();
    harness.run_steps(3);
    assert!(field(&harness).value().unwrap_or_default().starts_with("## References"));
    assert!(harness.get_by_label("Edit section").accesskit_node().is_disabled());
}

/// Write it myself opens an empty topic's first section.
#[test]
fn write_it_myself_opens_the_first_section() {
    let (shell, _root) = profile_at("profile_edit_write");
    let mut harness = tray_harness(shell);
    pick_topic(&mut harness, "Goals and deal-breakers");
    button(&harness, "Write it myself").click();
    harness.run_steps(3);
    assert!(editing(&harness));
    assert!(!is_shown(&harness, "Not filled in yet"), "the editor takes the empty state's place");
}

#[test]
fn profile_edit_light() {
    let mut harness = probed_wgpu(probed(shell(Screen::Profile, "profile_edit_light"), FixedTools), Theme::Light);
    fill_the_budget(&mut harness);
    compare(&mut harness, "profile_edit_light");
}

/// A run works, so Save waits and says why on hover.
#[test]
fn profile_busy_light() {
    let (shell, _root, _sink, _feed) = profile_with_run("profile_busy_light");
    let mut harness = probed_wgpu(probed(shell, FixedTools), Theme::Light);
    fill_the_budget(&mut harness);
    button(&harness, "Save").hover();
    harness.run_steps(8);
    compare(&mut harness, "profile_busy_light");
}

/// A task added a line to the role while it was open.
#[test]
fn profile_conflict_light() {
    let (shell, root) = profile_at("profile_conflict_light");
    let mut harness = probed_wgpu(probed(shell, FixedTools), Theme::Light);
    fill_the_budget(&mut harness);
    rewrite(&root, CANDIDATE, "update within a minute.\n", "update within a minute.\n- Introduced a weekly planning review with depot managers.\n");
    cmd_enter(&mut harness);
    compare(&mut harness, "profile_conflict_light");
}

/// Save mine over it writes only over the version the box shows. A newer one opens the box again on it.
#[test]
fn save_mine_over_it_never_writes_over_an_unseen_version() {
    let (shell, root) = profile_at("profile_edit_conflict_twice");
    let before = candidate(&root);
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    rewrite(&root, CANDIDATE, "update within a minute.\n", "update within a minute.\n- Introduced a weekly planning review with depot managers.\n");
    cmd_enter(&mut harness);
    rewrite(&root, CANDIDATE, "with depot managers.\n", "with depot managers.\n- Cut fuel use by a tenth.\n");
    let newer = candidate(&root);
    press_button(&mut harness, SAVE_OVER);
    assert_eq!(candidate(&root), newer, "the version the box did not show stays");
    assert!(is_shown(&harness, "- Cut fuel use by a tenth."), "the box shows the newer version");
    cmd_enter(&mut harness);
    assert_eq!(candidate(&root), before.replace(BUDGET, BUDGET_FILLED), "once shown, Save mine over it writes");
}

/// Each block opened has a field of its own, so undo never brings back another block's text.
#[test]
fn undo_stays_with_the_block_opened() {
    let (shell, root) = profile_at("profile_edit_undo");
    let before = candidate(&root);
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    press(&mut harness, Key::Escape);
    press(&mut harness, Key::Escape);
    press_button(&mut harness, "Discard changes");
    harness.get_all_by_label("Edit").nth(1).expect("the second role's Edit").click();
    harness.run_steps(3);
    let second = field(&harness).value().unwrap_or_default();
    assert!(second.starts_with("### [JOB_TITLE] - Harbour Lane Agency"), "{second}");
    harness.key_press_modifiers(COMMAND, Key::Z);
    harness.run_steps(3);
    assert_eq!(field(&harness).value().unwrap_or_default(), second, "undo has nothing of the first role's");
    cmd_enter(&mut harness);
    assert_eq!(candidate(&root), before);
}

/// The editor's field takes the focus as it opens, from a click or from the keyboard.
#[test]
fn the_field_takes_the_focus_as_the_editor_opens() {
    let (shell, _root) = profile_at("profile_edit_focus");
    let mut harness = tray_harness(shell);
    edit_first_role(&mut harness);
    assert!(field(&harness).is_focused(), "after a click");
    press(&mut harness, Key::Escape);
    press(&mut harness, Key::Escape);
    assert!(!editing(&harness));
    harness.get_all_by_label("Edit").nth(1).unwrap().focus();
    harness.run_steps(1);
    press(&mut harness, Key::Enter);
    assert!(editing(&harness) && field(&harness).is_focused(), "after Enter on Edit");
}

/// The alert's way to Setup leaves Profile as a tab does: unsaved text asks first.
#[test]
fn the_alerts_way_to_setup_asks_first_too() {
    let mut harness = tray_harness(probed(broken_shell(Screen::Profile, "profile_edit_alert_setup", &[Source::Profile]), FixedTools));
    settle(&mut harness);
    fill_the_budget(&mut harness);
    button(&harness, "Open Setup").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, DISCARD_ASK));
    button(&harness, "Keep editing").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, PROFILE_SUBTITLE) && editing(&harness), "Profile and the draft stay");
}

/// Quitting with unsaved text asks first, and Keep editing keeps the window and the draft.
#[test]
fn quitting_with_a_draft_asks_first() {
    let (shell, _root) = profile_at("profile_edit_quit");
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    harness.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
    harness.run_steps(3);
    assert!(is_shown(&harness, DISCARD_ASK));
    assert!(harness.query_by_label_contains("Quitting drops them.").is_some());
    assert!(button(&harness, "Keep editing").is_focused());
    press(&mut harness, Key::Escape);
    assert!(!is_shown(&harness, DISCARD_ASK), "Esc keeps editing");
    assert!(field(&harness).value().unwrap_or_default().ends_with(BUDGET_FILLED));
}

/// A write that fails keeps the editor open with the draft, and says why under it.
#[cfg(unix)]
#[test]
fn a_failed_write_keeps_the_editor_open() {
    let (shell, root) = profile_at("profile_edit_failed");
    let outside = tempfile::tempdir().unwrap();
    let file = root.join(CANDIDATE);
    let elsewhere = outside.path().join("candidate.md");
    fs::copy(&file, &elsewhere).unwrap();
    fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &file).unwrap();
    let before = fs::read_to_string(&elsewhere).unwrap();
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    cmd_enter(&mut harness);
    assert_eq!(fs::read_to_string(&elsewhere).unwrap(), before);
    let reason = harness.get_by_label_contains("Not saved. 01-candidate-profile.md:").accesskit_node().value().unwrap_or_default();
    assert!(reason.contains("outside"), "{reason}");
    assert!(field(&harness).value().unwrap_or_default().ends_with(BUDGET_FILLED), "the editor stays open with the draft");
}

/// An empty topic whose section is not in its file has nothing to open, so Write it myself waits and says why.
#[test]
fn write_it_myself_waits_for_a_missing_section() {
    let root = sample_root("profile_edit_missing");
    rewrite(&root, "CLAUDE.md", "### Deal-breakers", "### Hard limits");
    let shell = Shell::with_snapshot(Screen::Profile, jobdesk::workspace::open(&root).unwrap(), Snapshot::load(&root, day()));
    let mut harness = tray_harness(shell);
    pick_topic(&mut harness, "Goals and deal-breakers");
    let write = button(&harness, "Write it myself");
    assert!(write.accesskit_node().is_disabled());
    write.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "This topic's section is not in the file. Update with Claude… adds it."));
}

fn ask_to_close(harness: &mut Harness) {
    harness.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
    harness.run_steps(3);
}

/// Closing the window while the tab's ask shows leaves one ask on screen: the quit's.
#[test]
fn one_ask_shows_at_a_time() {
    let mut harness = tray_harness(probed(shell(Screen::Profile, "profile_edit_one_ask"), FixedTools));
    settle(&mut harness);
    fill_the_budget(&mut harness);
    harness.get_by_role_and_label(egui::accesskit::Role::Tab, "Setup").click();
    harness.run_steps(3);
    assert!(harness.query_by_label_contains("Leaving Profile drops them.").is_some());
    ask_to_close(&mut harness);
    assert_eq!(harness.query_all_by_label(DISCARD_ASK).count(), 1, "one ask");
    assert!(harness.query_by_label_contains("Quitting drops them.").is_some());
    assert!(harness.query_by_label_contains("Leaving Profile drops them.").is_none());
}

/// Discard on the quit ask closes the window, and a live session asks next. Keep working there keeps the draft too.
#[test]
fn discard_on_quit_waits_for_the_session_ask() {
    let (shell, _root, _sink, _feed) = profile_with_run("profile_edit_quit_session");
    let mut harness = tray_harness(shell);
    fill_the_budget(&mut harness);
    ask_to_close(&mut harness);
    button(&harness, "Discard changes").click();
    let mut sent = Vec::new();
    for _ in 0..3 {
        harness.run_steps(1);
        sent.extend(harness.output().viewport_output.get(&egui::ViewportId::ROOT).map(|viewport| viewport.commands.clone()).unwrap_or_default());
    }
    assert!(sent.contains(&egui::ViewportCommand::Close), "Discard asks the window to close");
    assert!(!is_shown(&harness, DISCARD_ASK));

    // The platform answers with a close request, and the live session asks.
    ask_to_close(&mut harness);
    assert!(is_shown(&harness, "Keep working"));
    assert!(!is_shown(&harness, DISCARD_ASK), "the draft does not ask again");
    harness.get_by_label("Keep working").click();
    harness.run_steps(3);
    assert!(field(&harness).value().unwrap_or_default().ends_with(BUDGET_FILLED), "the quit was cancelled, so the draft stays");

    ask_to_close(&mut harness);
    assert!(is_shown(&harness, DISCARD_ASK), "the next quit asks about the draft again");
}

/// Fictional keys only: neither has ever been a real key.
const FAKE_KEY: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";
const REFUSED_KEY: &str = "sk-ant-api03-FAKE-not-a-real-key-1111-Xq7m";
/// The API key's line on the left, which names where the key is kept.
const KEY_LINE: &str = if cfg!(windows) { "A Console key, in Credential Manager" } else { "A Console key, in your keychain" };

/// A Test connection that answers at once, as Claude Code would once its process ends.
struct AnsweringTester {
    outcome: Outcome,
    cost_usd: f64,
    said: &'static str,
    /// How Claude Code said it signed in.
    source: Option<&'static str>,
}

impl ConnectionTester for AnsweringTester {
    fn start(&self, _: Option<ApiKey>, done: TestDone) {
        done(TestReport { outcome: self.outcome, cost_usd: self.cost_usd, said: self.said.to_owned(), source: self.source.map(str::to_owned) });
    }
}

/// A tester of the saved key, which Claude Code reports as `ANTHROPIC_API_KEY` once it got that far.
fn answering(outcome: Outcome, cost_usd: f64, said: &'static str) -> Box<dyn ConnectionTester> {
    let source = (outcome == Outcome::Passed).then_some("ANTHROPIC_API_KEY");
    Box::new(AnsweringTester { outcome, cost_usd, said, source })
}

/// A tester whose Claude Code signed in through `source`, as the init line says.
fn answering_from(outcome: Outcome, said: &'static str, source: &'static str) -> Box<dyn ConnectionTester> {
    Box::new(AnsweringTester { outcome, cost_usd: 0.004, said, source: Some(source) })
}

/// 31 October at `hour`:`minute`, the clock Providers counts ages to.
fn at(hour: u32, minute: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(hour, minute, 0).unwrap()
}

/// A desk whose one task, started at `started` with `method`, said it signed in through `source`, then ended.
fn signed_in_desk(source: &str, method: SignIn, started: NaiveDateTime) -> RunDesk {
    let sink = SharedSink::default();
    let mut desk = desk_on(ScriptedGateway(Arc::clone(&sink)));
    desk.set_sign_in(method);
    desk.start("/rank", &StartOptions { model: None, budget_usd: 1.0 }, started).unwrap();
    emit_all(
        &sink,
        [
            AgentEvent::SessionStarted { session_id: "7c41e2d0-0b9a".into(), model: "claude-sonnet-4-5".into(), api_key_source: source.into() },
            AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.31, duration_ms: 42_000 },
            AgentEvent::ProcessExited { failure: None },
        ],
    );
    desk
}

/// Settings › Providers over Today at 11:30, with the usage fixture and a $60 cap, as the canvas's rail shows them.
/// The last task, started with `method` at `started`, signed in through `source`, and `providers` holds the key and
/// Test connection.
fn providers_shell(name: &str, providers: Providers, (source, method): (&str, SignIn), started: NaiveDateTime) -> Shell {
    let mut shell = shell_with_usage(name, usage_tasks()).with_providers(providers).with_now(at(11, 30));
    shell.settings_mut().set_cap_on(true);
    shell.settings_mut().set_cap_amount(Dollars::parse("60"));
    *shell.desk_slot() = Some(signed_in_desk(source, method, started));
    shell.open_settings(Page::Providers);
    shell
}

/// No key saved, and the last task signed in with the Claude sign-in two hours ago.
fn sign_in_shell(name: &str, tester: Box<dyn ConnectionTester>) -> Shell {
    providers_shell(name, open_providers(memory_keychain(None), tester), ("none", SignIn::ClaudeSignIn), at(9, 30))
}

/// A key saved and in use, its test passed a minute ago, and the last task signed in with it four minutes ago.
fn key_saved_shell(name: &str) -> Shell {
    let providers = open_providers(memory_keychain(Some(FAKE_KEY)), idle_tester());
    let mut shell = providers_shell(name, providers, ("ANTHROPIC_API_KEY", SignIn::ApiKey), at(11, 26));
    shell.settings_mut().set_sign_in(SignIn::ApiKey);
    shell.settings_mut().set_key_test(Some(KeyTest { passed: true, at: at(11, 29), cost_usd: 0.004 }));
    shell
}

fn key_field<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
    harness.get_by_role(egui::accesskit::Role::PasswordInput)
}

/// Picks the API key on the left and pastes `key` into its field.
fn type_key(harness: &mut Harness<'static>, key: &str) {
    harness.get_by_label(KEY_LINE).click();
    harness.run_steps(3);
    key_field(harness).focus();
    harness.run_steps(1);
    key_field(harness).type_text(key);
    harness.run_steps(3);
}

/// Saves the key typed, which tests it at once, and lets the test's answer reach the page.
fn save_key(harness: &mut Harness<'static>) {
    harness.get_by_label("Save key").click();
    harness.run_steps(4);
}

fn desk_sign_in(shell: &Rc<RefCell<Shell>>) -> SignIn {
    shell.borrow_mut().desk_slot().as_ref().unwrap().sign_in()
}

/// No label, value or description on screen holds `key` in full.
fn assert_never_shown(harness: &Harness, key: &str) {
    let holds = harness.query_all_by(|node| {
        [node.label(), node.value(), node.description()].into_iter().flatten().any(|text| text.contains(key) || text.contains("FAKE-not-a-real"))
    });
    assert_eq!(holds.count(), 0, "the key shows only as its mask");
}

#[test]
fn save_key_waits_for_a_key() {
    let mut harness = tray_harness(sign_in_shell("providers_save_waits", idle_tester()));
    harness.get_by_label(KEY_LINE).click();
    harness.run_steps(3);
    let save = harness.get_by_label("Save key");
    assert!(save.accesskit_node().is_disabled());
    save.hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, "Paste a key first."));
    key_field(&harness).focus();
    harness.run_steps(1);
    key_field(&harness).type_text("   ");
    harness.run_steps(3);
    assert!(harness.get_by_label("Save key").accesskit_node().is_disabled(), "spaces are no key");
}

/// A key that passes its test signs the next task in, and the rail and the desk say so. The key never shows in full.
#[test]
fn a_saved_key_that_passes_signs_tasks_in() {
    let (mut harness, shell) = shared_harness(sign_in_shell("providers_key_passes", answering(Outcome::Passed, 0.004, "ready")));
    assert_eq!(desk_sign_in(&shell), SignIn::ClaudeSignIn);
    type_key(&mut harness, FAKE_KEY);
    assert_never_shown(&harness, FAKE_KEY);
    save_key(&mut harness);
    assert_eq!(desk_sign_in(&shell), SignIn::ApiKey, "the next task signs in with the key");
    assert_eq!(harness.query_all_by_label("API key").count(), 3, "the rail says so, beside the row's title and the detail's");
    assert!(is_shown(&harness, "sk-ant-••••••••"));
    assert!(is_shown(&harness, "R2kd"));
    assert!(is_shown(&harness, "Passed"));
    assert!(is_shown(&harness, "just now, for $0.004"));
    assert_eq!(key_field_count(&harness), 0, "the field is gone, emptied");
    assert_never_shown(&harness, FAKE_KEY);
}

fn key_field_count(harness: &Harness) -> usize {
    harness.query_all_by_role(egui::accesskit::Role::PasswordInput).count()
}

/// A refused key stays saved, says what Claude Code said, and leaves tasks on the Claude sign-in.
#[test]
fn a_refused_key_leaves_tasks_on_the_claude_sign_in() {
    let (mut harness, shell) = shared_harness(sign_in_shell("providers_key_refused", answering(Outcome::Refused, 0.0, "Invalid API key")));
    type_key(&mut harness, REFUSED_KEY);
    save_key(&mut harness);
    assert_eq!(desk_sign_in(&shell), SignIn::ClaudeSignIn);
    assert!(is_shown(&harness, "Key refused"), "the rail says so");
    assert!(is_shown(&harness, "Claude Code refused this key."));
    assert!(is_shown(&harness, "Invalid API key"), "what Claude Code said, as plain text");
    assert!(is_shown(&harness, "Xq7m"), "the key stays saved");
    assert_never_shown(&harness, REFUSED_KEY);
}

/// A test that never reached the point of trying the key reads as such, never as a refusal, and changes no method.
#[test]
fn an_unreached_test_is_no_refusal() {
    let (mut harness, shell) =
        shared_harness(sign_in_shell("providers_key_unreached", answering(Outcome::Unreached, 0.0, "Claude Code did not answer within a minute.")));
    type_key(&mut harness, FAKE_KEY);
    save_key(&mut harness);
    assert_eq!(desk_sign_in(&shell), SignIn::ClaudeSignIn, "tasks keep their method");
    assert!(is_shown(&harness, "Claude Code could not test this key."));
    assert!(is_shown(&harness, "Claude Code did not answer within a minute."));
    assert!(harness.query_by_label_contains("efused").is_none(), "nothing says it was refused");
    assert!(is_shown(&harness, "R2kd"), "the key stays saved");
    assert!(!is_shown(&harness, "Key refused"), "the rail keeps the Claude sign-in");
}

/// While a task works, Test connection waits and says why. One Claude process runs at a time.
#[test]
fn test_connection_waits_while_a_task_runs() {
    let mut shell = sign_in_shell("providers_task_running", idle_tester());
    *shell.desk_slot() = Some(apply_run(permission_prompt()));
    let mut harness = tray_harness(shell);
    let reason = "A task is running. Test once it ends.";
    assert!(is_shown(&harness, reason), "the reason shows without the pointer, for the keyboard and a screen reader");
    let test = harness.get_by_label("Test connection");
    assert!(test.accesskit_node().is_disabled());
    test.hover();
    harness.run_steps(8);
    assert_eq!(harness.query_all_by_label(reason).count(), 2, "and on hover");
}

/// A key saved while a task is open is not tested, and the note says so before Save key.
#[test]
fn a_key_saved_while_a_task_is_open_waits_untested() {
    let mut shell = sign_in_shell("providers_save_task_open", answering(Outcome::Passed, 0.004, "ready"));
    *shell.desk_slot() = Some(apply_run(permission_prompt()));
    let mut harness = tray_harness(shell);
    type_key(&mut harness, FAKE_KEY);
    assert!(
        harness.query_by_label_contains("A task is open, so it waits at Not tested yet: once that ends, choose Test connection or Use for tasks.").is_some()
    );
    assert!(harness.query_by_label_contains("then tests it for about a cent").is_none(), "no test is promised");
    save_key(&mut harness);
    assert!(is_shown(&harness, "Not tested yet"), "the tester was not asked");
    assert!(is_shown(&harness, "R2kd"), "the key is saved");
}

/// A key saved while a test of the Claude sign-in still runs is not tested either, and the note says why.
#[test]
fn a_key_saved_while_another_test_finishes_waits_untested() {
    let mut harness = tray_harness(sign_in_shell("providers_save_testing", idle_tester()));
    harness.get_by_label("Test connection").click();
    harness.run_steps(3);
    type_key(&mut harness, FAKE_KEY);
    let note = "Another test is finishing, so it waits at Not tested yet: once that ends, choose Test connection or Use for tasks.";
    assert!(harness.query_by_label_contains(note).is_some());
    assert!(harness.query_by_label_contains("then tests it for about a cent").is_none(), "no test is promised");
    save_key(&mut harness);
    assert!(is_shown(&harness, "Not tested yet"), "the key is not under test");
    assert!(is_shown(&harness, "R2kd"), "the key is saved");
}

/// Remove asks first. Esc keeps the key; Remove key deletes it and moves tasks back to the Claude sign-in.
#[test]
fn remove_asks_first_and_esc_keeps_the_key() {
    let keys = memory_keychain(Some(FAKE_KEY));
    let providers = open_providers(Arc::clone(&keys), idle_tester());
    let mut shell = providers_shell("providers_remove", providers, ("ANTHROPIC_API_KEY", SignIn::ApiKey), at(11, 26));
    shell.settings_mut().set_sign_in(SignIn::ApiKey);
    let (mut harness, shell) = shared_harness(shell);
    let asking = |harness: &Harness<'static>| harness.query_all_by_label("Remove the API key?").next().is_some();
    harness.get_by_label("Remove").click();
    harness.run_steps(3);
    assert!(asking(&harness), "it asks first");
    assert!(harness.get_by_label("Keep it").accesskit_node().is_focused(), "Keep it is the default");
    press(&mut harness, Key::Escape);
    assert!(!asking(&harness));
    assert!(shell.borrow().settings_open(), "Esc keeps the key, and Settings stays");
    assert!(keys.read().unwrap().is_some());
    assert_eq!(desk_sign_in(&shell), SignIn::ApiKey);

    harness.get_by_label("Remove").click();
    harness.run_steps(3);
    harness.get_by_label("Remove key").click();
    harness.run_steps(3);
    assert!(keys.read().unwrap().is_none(), "the keychain item is gone");
    assert_eq!(desk_sign_in(&shell), SignIn::ClaudeSignIn);
    assert_eq!(harness.query_all_by_label("Not set").count(), 2, "the row and the detail say no key is set");
}

/// Open the task leaves Settings for that task on Tasks.
#[test]
fn open_the_task_selects_it() {
    let (mut harness, shell) = shared_harness(sign_in_shell("providers_open_task", idle_tester()));
    assert!(is_shown(&harness, "Last task signed in with your Claude sign-in"));
    assert!(is_shown(&harness, ", 2 hours ago. Claude Code reports this itself."));
    harness.get_by_label("Open the task").click();
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open());
    assert!(harness.query_all_by_label("/rank").next().is_some(), "the task is on show");
}

/// Before any task, the note says it will show after the first one.
#[test]
fn before_any_task_the_note_waits_for_one() {
    let mut shell = sign_in_shell("providers_no_task", idle_tester());
    *shell.desk_slot() = Some(desk_on(ScriptedGateway(SharedSink::default())));
    let harness = tray_harness(shell);
    assert!(is_shown(&harness, "The sign-in each task uses shows here after your first task. Claude Code reports it itself."));
    assert!(!is_shown(&harness, "Open the task"));
}

/// Test connection runs Claude Code, so the launcher's Run waits for it to end and says why.
#[test]
fn run_waits_while_a_test_runs() {
    let calls = SharedCalls::default();
    let mut shell = recording_shell(Screen::InTray, "providers_run_waits", &calls).with_providers(open_providers(memory_keychain(None), idle_tester()));
    shell.open_settings(Page::Providers);
    let mut harness = tray_harness(shell);
    harness.get_by_label("Test connection").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Testing…"));
    harness.key_press_modifiers(COMMAND, Key::K);
    harness.run_steps(3);
    let run = harness.get_by_label("Run");
    assert!(run.accesskit_node().is_disabled());
    run.hover();
    harness.run_steps(8);
    assert!(harness.query_all_by_label("Test connection is running. Run opens once it ends.").next().is_some());
    cmd_enter(&mut harness);
    assert_eq!(calls.lock().unwrap().starts, 0, "nothing starts while the test runs");
}

fn render_providers(shell: Shell, theme: Theme) -> Harness<'static> {
    probed_wgpu(probed(shell, VersionedTools), theme)
}

#[test]
fn settings_providers_light() {
    let name = "settings_providers_light";
    let mut harness = render_providers(sign_in_shell(name, idle_tester()), Theme::Light);
    compare(&mut harness, name);
}

#[test]
fn settings_key_entry_light() {
    let name = "settings_key_entry_light";
    let mut harness = render_providers(sign_in_shell(name, idle_tester()), Theme::Light);
    type_key(&mut harness, FAKE_KEY);
    // The pointer leaves the row it picked, so the row shows at rest.
    harness.hover_at(egui::pos2(700.0, 700.0));
    harness.run_steps(3);
    compare(&mut harness, name);
}

#[test]
fn settings_key_saved_light() {
    let name = "settings_key_saved_light";
    let mut harness = render_providers(key_saved_shell(name), Theme::Light);
    compare(&mut harness, name);
}

#[test]
fn settings_key_saved_dark() {
    let name = "settings_key_saved_dark";
    let mut harness = render_providers(key_saved_shell(name), Theme::Dark);
    compare(&mut harness, name);
}

#[test]
fn settings_key_failed_light() {
    let name = "settings_key_failed_light";
    let mut harness = render_providers(sign_in_shell(name, answering(Outcome::Refused, 0.0, "Invalid API key")), Theme::Light);
    type_key(&mut harness, REFUSED_KEY);
    save_key(&mut harness);
    // The pointer leaves Save key's place, so nothing under it shows as hovered.
    harness.hover_at(egui::pos2(700.0, 700.0));
    harness.run_steps(3);
    compare(&mut harness, name);
}

/// The note names the method each task started with, not the one in use now.
#[test]
fn the_note_names_the_method_the_task_started_with() {
    // A task on the Claude sign-in found a key in the environment, then the user saved a key that passed.
    let (mut harness, shell) = shared_harness(providers_shell(
        "providers_note_env_key",
        open_providers(memory_keychain(None), answering(Outcome::Passed, 0.004, "ready")),
        ("ANTHROPIC_API_KEY", SignIn::ClaudeSignIn),
        at(9, 30),
    ));
    type_key(&mut harness, FAKE_KEY);
    save_key(&mut harness);
    assert_eq!(desk_sign_in(&shell), SignIn::ApiKey);
    assert!(is_shown(&harness, "Last task signed in with an API key from your environment"), "not the key saved since");

    // A task on the saved key, then the user removed it.
    let keys = memory_keychain(Some(FAKE_KEY));
    let mut shell = providers_shell("providers_note_removed", open_providers(keys, idle_tester()), ("ANTHROPIC_API_KEY", SignIn::ApiKey), at(11, 26));
    shell.settings_mut().set_sign_in(SignIn::ApiKey);
    let (mut harness, shell) = shared_harness(shell);
    harness.get_by_label("Remove").click();
    harness.run_steps(3);
    harness.get_by_label("Remove key").click();
    harness.run_steps(3);
    assert_eq!(desk_sign_in(&shell), SignIn::ClaudeSignIn);
    assert!(is_shown(&harness, "Last task signed in with your API key"), "the key the task used, though it is gone now");
}

/// A key typed and not saved is forgotten whichever way the user leaves it.
#[test]
fn a_typed_key_is_forgotten_on_the_way_out() {
    let empty = |harness: &Harness<'static>| key_field(harness).value().unwrap_or_default().is_empty();
    let typed = |name: &str| {
        let (mut harness, shell) = shared_harness(sign_in_shell(name, idle_tester()));
        type_key(&mut harness, FAKE_KEY);
        assert!(!empty(&harness), "the field holds the key");
        (harness, shell)
    };
    let back_to_key = |harness: &mut Harness<'static>| {
        harness.get_by_label(KEY_LINE).click();
        harness.run_steps(3);
    };

    let (mut harness, _) = typed("providers_forget_row");
    harness.get_by_label("Your login in Claude Code").click();
    harness.run_steps(3);
    back_to_key(&mut harness);
    assert!(empty(&harness), "another row was picked");

    let (mut harness, _) = typed("providers_forget_page");
    harness.get_by_label("General").click();
    harness.run_steps(3);
    harness.get_by_label("Providers").click();
    harness.run_steps(3);
    back_to_key(&mut harness);
    assert!(empty(&harness), "another page was opened");

    let (mut harness, shell) = typed("providers_forget_esc");
    press(&mut harness, Key::Escape);
    press(&mut harness, Key::Escape);
    assert!(!shell.borrow().settings_open(), "the first Esc leaves the field, the second closes Settings");
    cmd_comma(&mut harness);
    back_to_key(&mut harness);
    assert!(empty(&harness), "Settings was closed");

    let (mut harness, _) = typed("providers_forget_cancel");
    harness.get_by_label("Cancel").click();
    harness.run_steps(3);
    back_to_key(&mut harness);
    assert!(empty(&harness), "Cancel");
}

/// A keychain that will not take the key: the field is emptied all the same, and the page says why.
struct LockedKeychain;

impl KeyStore for LockedKeychain {
    fn read(&self) -> Result<Option<ApiKey>, KeyStoreError> {
        Ok(None)
    }

    fn write(&self, _: &ApiKey) -> Result<(), KeyStoreError> {
        Err(KeyStoreError { message: "The keychain is locked.".to_owned() })
    }

    fn delete(&self) -> Result<(), KeyStoreError> {
        Ok(())
    }
}

#[test]
fn a_refused_keychain_write_still_empties_the_field() {
    let providers = open_providers(Arc::new(LockedKeychain), idle_tester());
    let mut harness = tray_harness(providers_shell("providers_locked", providers, ("none", SignIn::ClaudeSignIn), at(9, 30)));
    type_key(&mut harness, FAKE_KEY);
    save_key(&mut harness);
    assert!(is_shown(&harness, "The keychain is locked."));
    assert!(key_field(&harness).value().unwrap_or_default().is_empty());
    assert_never_shown(&harness, FAKE_KEY);
}

/// A keychain whose first read says it started, then waits for the test's answer, as behind its prompt.
/// With `denied` set, the read before it is denied at once instead, as a prompt the user turned down.
struct PromptKeychain {
    denied: std::sync::atomic::AtomicBool,
    asked: Mutex<std::sync::mpsc::Sender<()>>,
    answered: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl KeyStore for PromptKeychain {
    fn read(&self) -> Result<Option<ApiKey>, KeyStoreError> {
        if self.denied.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return Err(KeyStoreError { message: "access denied".to_owned() });
        }
        let _ = self.asked.lock().unwrap().send(());
        let answered = self.answered.lock().unwrap().take();
        if let Some(answered) = answered {
            answered.recv().unwrap();
        }
        Ok(ApiKey::parse(FAKE_KEY))
    }

    fn write(&self, _: &ApiKey) -> Result<(), KeyStoreError> {
        Ok(())
    }

    fn delete(&self) -> Result<(), KeyStoreError> {
        Ok(())
    }
}

/// While a task's start reads the keychain, Providers does not know yet whether a key is saved, so it offers nothing to do.
#[test]
fn providers_waits_for_a_keychain_read_a_task_started() {
    let (asked, started) = std::sync::mpsc::channel();
    let (answer, answered) = std::sync::mpsc::channel();
    let keys = PromptKeychain { denied: false.into(), asked: Mutex::new(asked), answered: Mutex::new(Some(answered)) };
    let providers = open_providers(Arc::new(keys), idle_tester());
    let reader = providers.key_reader();
    let task = std::thread::spawn(move || reader().map(|key| key.is_some()));
    started.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    let mut harness = tray_harness(providers_shell("providers_keychain_waits", providers, ("none", SignIn::ClaudeSignIn), at(9, 30)));
    harness.get_by_label(KEY_LINE).click();
    harness.run_steps(3);
    assert_waits_for_the_keychain(&harness);
    answer.send(()).unwrap();
    assert_eq!(task.join().unwrap(), Ok(true));
    harness.run_steps(3);
    assert!(!is_shown(&harness, "Waiting for the keychain…"));
    assert!(is_shown(&harness, "Remove") && !is_shown(&harness, "Save key"), "the saved key shows once the keychain answers");
}

/// A first read denied shows its reason, with Replace and Remove. A task's start that reads again behind a new prompt
/// may change that answer, and Replace or Remove would write the keychain under the prompt, so the page waits for it too.
#[test]
fn providers_waits_for_a_task_that_reads_again_after_a_denied_read() {
    let (asked, started) = std::sync::mpsc::channel();
    let (answer, answered) = std::sync::mpsc::channel();
    let keys = PromptKeychain { denied: true.into(), asked: Mutex::new(asked), answered: Mutex::new(Some(answered)) };
    let providers = open_providers(Arc::new(keys), idle_tester());
    let reader = providers.key_reader();
    let mut harness = tray_harness(providers_shell("providers_keychain_waits_again", providers, ("none", SignIn::ClaudeSignIn), at(9, 30)));
    harness.get_by_label(KEY_LINE).click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "access denied") && is_shown(&harness, "Remove"), "the denial shows first");
    let task = std::thread::spawn(move || reader().map(|key| key.is_some()));
    started.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    harness.run_steps(3);
    assert_waits_for_the_keychain(&harness);
    assert!(!is_shown(&harness, "access denied"), "the old denial may not be the answer");
    answer.send(()).unwrap();
    assert_eq!(task.join().unwrap(), Ok(true));
    harness.run_steps(3);
    assert!(!is_shown(&harness, "Waiting for the keychain…"));
    assert!(is_shown(&harness, "Remove") && !is_shown(&harness, "access denied"), "the key the task read shows once the keychain answers");
}

/// Nothing on the API key's side acts while it waits: no field, and no button that writes or tests the key.
fn assert_waits_for_the_keychain(harness: &Harness) {
    assert!(is_shown(harness, "Waiting for the keychain…"));
    for action in ["Save key", "Replace", "Remove", "Test connection", "Use for tasks"] {
        assert!(!is_shown(harness, action), "{action}");
    }
    assert!(harness.query_by_role(egui::accesskit::Role::PasswordInput).is_none(), "no field to paste a key over the saved one");
}

/// A task whose turn ended keeps its process up while it waits for a reply, and Test connection waits for it too. Its
/// reason says how to free it, as a working task's cannot.
#[test]
fn test_connection_waits_for_a_session_that_waits_on_a_reply() {
    let mut shell = sign_in_shell("providers_session_waits", idle_tester());
    *shell.desk_slot() = Some(tray_waiting_apply_run());
    let harness = tray_harness(shell);
    assert!(harness.get_by_label("Test connection").accesskit_node().is_disabled());
    assert!(is_shown(&harness, "A task is still open. Reply or end it, then test."), "the reason shows without the pointer");
    assert!(!is_shown(&harness, "A task is running. Test once it ends."), "nothing is working");
}

/// While Test connection runs, a reply that would start a new process waits and says why.
#[test]
fn a_reply_that_needs_a_process_waits_for_the_test() {
    let calls = SharedCalls::default();
    let sink = SharedSink::default();
    let mut desk = desk_on(RecordingGateway(Arc::clone(&sink), Arc::clone(&calls)));
    desk.start(APPLY_LINE, &StartOptions { model: None, budget_usd: 3.0 }, run_started()).unwrap();
    emit_all(
        &sink,
        [
            AgentEvent::SessionStarted { session_id: "3f9a21c0-5d7e".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() },
            AgentEvent::AssistantText { text: STOP_EVALUATION.into(), nested: false },
            AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 1.12, duration_ms: 303_000 },
            AgentEvent::ProcessExited { failure: None },
        ],
    );
    let mut shell = shell(Screen::InTray, "providers_reply_waits").with_providers(open_providers(memory_keychain(None), idle_tester()));
    *shell.desk_slot() = Some(desk);
    shell.open_settings(Page::Providers);
    let mut harness = tray_harness(shell);
    harness.get_by_label("Test connection").click();
    harness.run_steps(3);
    press(&mut harness, Key::Escape);
    harness.get_by_role_and_label(egui::accesskit::Role::Tab, "Tasks").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Test connection is running. You can reply once it ends."));
    assert!(harness.get_by_label("Send").accesskit_node().is_disabled());
    assert_eq!(calls.lock().unwrap().starts, 1, "no process started for a reply");
}

/// Testing… is the shown method's own: a test of the other method, or one turned away from, reads as still finishing.
#[test]
fn another_methods_test_reads_as_still_finishing() {
    let keys = memory_keychain(Some(FAKE_KEY));
    let mut harness = tray_harness(providers_shell("providers_other_test", open_providers(keys, idle_tester()), ("none", SignIn::ClaudeSignIn), at(9, 30)));
    harness.get_by_label("Test connection").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Testing…"), "the Claude sign-in's own test");
    harness.get_by_label(KEY_LINE).click();
    harness.run_steps(3);
    let test = harness.get_by_label("Test connection");
    assert!(test.accesskit_node().is_disabled());
    assert!(!is_shown(&harness, "Testing…"));
    assert!(is_shown(&harness, "Another test is still finishing."));
}

/// A Claude sign-in test that a key in the environment answered says so: its pass is that key's, and its refusal is
/// mended there, not with /login.
#[test]
fn a_sign_in_test_names_a_key_from_the_environment() {
    let mut harness = tray_harness(sign_in_shell("providers_env_pass", answering_from(Outcome::Passed, "ready", "ANTHROPIC_API_KEY")));
    harness.get_by_label("Test connection").click();
    harness.run_steps(4);
    assert!(is_shown(&harness, "Passed"));
    assert!(is_shown(
        &harness,
        "Claude Code used an API key from your environment, not your Claude sign-in. Tasks on the Claude sign-in use it too while it is set."
    ));

    let mut harness = tray_harness(sign_in_shell("providers_env_refused", answering_from(Outcome::Refused, "Invalid API key", "ANTHROPIC_API_KEY")));
    harness.get_by_label("Test connection").click();
    harness.run_steps(4);
    assert!(is_shown(&harness, "The API key in your environment was refused."));
    assert!(!is_shown(&harness, "Claude Code is not signed in."));
    assert!(harness.query_by_label_contains("then /login, and test again").is_none(), "/login would not mend it");

    let mut harness = tray_harness(sign_in_shell("providers_login_refused", answering_from(Outcome::Refused, "Not logged in", "none")));
    harness.get_by_label("Test connection").click();
    harness.run_steps(4);
    assert!(is_shown(&harness, "Claude Code is not signed in."));

    // Claude Code names an account signed in with /login this way: it is the Claude sign-in.
    let mut harness = tray_harness(sign_in_shell("providers_login_key_pass", answering_from(Outcome::Passed, "ready", "/login managed key")));
    harness.get_by_label("Test connection").click();
    harness.run_steps(4);
    assert!(is_shown(&harness, "Passed"));
    assert!(harness.query_by_label_contains("not your Claude sign-in").is_none(), "the pass is the sign-in's own");

    let mut harness = tray_harness(sign_in_shell("providers_login_key_refused", answering_from(Outcome::Refused, "OAuth token expired", "/login managed key")));
    harness.get_by_label("Test connection").click();
    harness.run_steps(4);
    assert!(is_shown(&harness, "Claude Code is not signed in."), "/login mends it");
    assert!(!is_shown(&harness, "Claude Code was refused with a sign-in it chose itself."));
}

/// Each way in is one control a screen reader names, with whether it is picked. Cloud providers says why it waits.
#[test]
fn the_ways_in_are_heard() {
    let harness = tray_harness(sign_in_shell("providers_heard", idle_tester()));
    let login = harness.get_by_label("Claude sign-in, Your login in Claude Code, In use");
    assert_eq!(login.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
    let key_name = format!("API key, {KEY_LINE}, Not set");
    let key = harness.get_by_label(&key_name);
    assert_eq!(key.accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
    let cloud = harness.get_by_label("Cloud providers, Amazon Bedrock, Google Vertex AI, Later");
    assert!(cloud.accesskit_node().is_disabled());
    assert_eq!(cloud.accesskit_node().description().as_deref(), Some("Not available yet."));
}

/// First-run setup in the small first window, from a scripted feed, download, helper check and install, over a
/// fictional home folder in the test's own folder. Nothing reaches the network or the user's home.
mod first_run {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    use jobdesk::onboarding::application::{
        Contents, Failure as SetupFailure, Helper, HelperInstaller, HelperProbe, HelperSetup, InstallPhase, Onboarding, Phase, Progress, ProgressStore, Saved,
        SourceFetcher, Step, Unpacker,
    };
    use jobdesk::shared_kernel::LoadError;
    use jobdesk::shared_kernel::installed_source::InstalledSource;
    use jobdesk::updates::application::{Failure as FeedFailure, SourcePin, SourcePinReader};

    use super::*;

    /// The first window, as the shell asks for it.
    const FIRST_WINDOW: [f32; 2] = [720.0, 560.0];
    const HASH: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    /// How long a test waits for a worker's news before it fails.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    #[derive(Default)]
    struct Record(Mutex<Option<Progress>>);

    impl ProgressStore for Record {
        fn load(&self) -> Result<Option<Progress>, LoadError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&self, progress: &Progress) -> Result<Saved, LoadError> {
            *self.0.lock().unwrap() = Some(progress.clone());
            Ok(Saved::Whole)
        }
        fn clear(&self) -> Result<(), LoadError> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    struct Pins(Result<Option<SourcePin>, FeedFailure>);

    impl SourcePinReader for Pins {
        fn read(&self) -> Result<Option<SourcePin>, FeedFailure> {
            self.0.clone()
        }
    }

    /// Hands back a path only `Folders` reads, after it has said some bytes arrived. With `hold` it waits for Cancel.
    /// While `stalled` is set it waits whatever Cancel says, as a read on a stalled network does, then hands the path
    /// back, as a read that had every byte when the cancel came.
    struct Fetcher {
        hold: bool,
        stalled: Option<Arc<AtomicBool>>,
    }

    impl SourceFetcher for Fetcher {
        fn fetch(&self, _: &str, _: &str, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64)) -> Result<PathBuf, SetupFailure> {
            progress(1_000_000);
            if let Some(stalled) = &self.stalled {
                while stalled.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                return Ok(PathBuf::from("/fictional/onboarding/1/release.zip.part"));
            }
            while self.hold && !cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            if cancelled() { Err(SetupFailure::Cancelled) } else { Ok(PathBuf::from("/fictional/onboarding/1/release.zip.part")) }
        }
        fn clear_leftovers(&self) {}
    }

    /// Real folders on disk. Unpacking puts the sample workspace in the folder, or fails the check when `mismatch`.
    struct Folders {
        mismatch: bool,
    }

    impl Unpacker for Folders {
        fn look(&self, folder: &Path) -> Contents {
            match fs::read_dir(folder).map(|mut entries| entries.next().is_none()) {
                Err(_) if !folder.exists() => Contents::Missing,
                Ok(true) => Contents::Empty,
                _ if jobdesk::workspace::open(folder).is_some() => Contents::AiJobSearch,
                _ => Contents::Files,
            }
        }
        fn make(&self, folder: &Path) -> Result<(), SetupFailure> {
            fs::create_dir_all(folder).map_err(|_| SetupFailure::Disk)?;
            if self.look(folder) == Contents::Empty { Ok(()) } else { Err(SetupFailure::NotEmpty) }
        }
        fn unpack(&self, _: &Path, source: &InstalledSource, folder: &Path, _: &dyn Fn() -> bool, phase: &mut dyn FnMut(Phase)) -> Result<(), SetupFailure> {
            phase(Phase::Checking);
            if self.mismatch || source.sha256 != HASH {
                return Err(SetupFailure::Mismatch);
            }
            phase(Phase::Unpacking);
            sample_workspace(folder);
            Ok(())
        }
        fn installed(&self, _: &Path) -> Option<String> {
            None
        }
        fn clear_leftovers(&self, _: &Path) {}
    }

    /// The helpers whose programs run. The check of `held` never ends.
    struct Here {
        here: Mutex<BTreeSet<Helper>>,
        held: Option<Helper>,
    }

    impl HelperProbe for Here {
        fn is_here(&self, helper: Helper) -> bool {
            while self.held == Some(helper) {
                std::thread::sleep(Duration::from_millis(5));
            }
            self.here.lock().unwrap().contains(&helper)
        }
    }

    /// The tool check over the same helpers: `claude` and `pdftotext` always answer, and a helper's programs once it is
    /// here.
    struct HelperTools(Arc<Here>);

    impl ToolProber for HelperTools {
        fn probe(&self, tool: &ToolSpec) -> Option<String> {
            let helper = match tool.name {
                "python3" => Helper::Python,
                "bun" => Helper::Bun,
                "lualatex" | "xelatex" => Helper::Latex,
                _ => return Some("1.0".to_owned()),
            };
            self.0.here.lock().unwrap().contains(&helper).then(|| "1.0".to_owned())
        }
    }

    /// Installs a helper into `here` once it has said a third of it arrived, fails one in `fails`, and holds `held`
    /// there until Cancel. While `stalled` is set, an install waits without looking at Cancel, then puts its helper in
    /// place, as one past its last cancel check does.
    struct Installer {
        here: Arc<Here>,
        fails: BTreeMap<Helper, SetupFailure>,
        held: Option<Helper>,
        stalled: Option<Arc<AtomicBool>>,
    }

    impl HelperInstaller for Installer {
        fn install(&self, helper: Helper, _: &dyn Fn() -> bool, cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(InstallPhase)) -> Result<(), SetupFailure> {
            phase(InstallPhase::Downloading { received: 35, total: 100 });
            if let Some(stalled) = &self.stalled {
                while stalled.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                self.here.here.lock().unwrap().insert(helper);
                return Ok(());
            }
            while self.held == Some(helper) && !cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            if cancelled() {
                return Err(SetupFailure::Cancelled);
            }
            if let Some(failure) = self.fails.get(&helper) {
                return Err(*failure);
            }
            self.here.here.lock().unwrap().insert(helper);
            Ok(())
        }
        fn clear_leftovers(&self) {}
    }

    /// How a scripted setup goes. By default the feed pins 1.7.5, everything downloads and no helper is here.
    #[derive(Default)]
    struct Script {
        feed: Option<Result<Option<SourcePin>, FeedFailure>>,
        record: Option<Progress>,
        hold_download: bool,
        stall_download: Option<Arc<AtomicBool>>,
        mismatch: bool,
        here: Vec<Helper>,
        hold_check: Option<Helper>,
        hold_install: Option<Helper>,
        stall_install: Option<Arc<AtomicBool>>,
        fails: BTreeMap<Helper, SetupFailure>,
    }

    /// A fictional home folder in the test's own folder, emptied first, with an empty `Documents` in it.
    fn home(name: &str) -> PathBuf {
        let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("home");
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join("Documents")).unwrap();
        home
    }

    /// The shell on the first-run setup the script describes, and the home folder it works in.
    fn first_run_shell(name: &str, script: Script) -> (Shell, PathBuf) {
        let home = home(name);
        let feed = script.feed.unwrap_or_else(|| Ok(Some(SourcePin::parse("1.7.5", HASH).unwrap())));
        let probe = Arc::new(Here { here: Mutex::new(script.here.into_iter().collect()), held: script.hold_check });
        let installer = Installer { here: Arc::clone(&probe), fails: script.fails, held: script.hold_install, stalled: script.stall_install };
        let tools = HelperTools(Arc::clone(&probe));
        let helpers = HelperSetup::new(probe, Arc::new(installer));
        let (fetcher, folders) = (Fetcher { hold: script.hold_download, stalled: script.stall_download }, Folders { mismatch: script.mismatch });
        let store = Arc::new(Record(Mutex::new(script.record)));
        let documents = home.join("Documents");
        let onboarding = Onboarding::new(store, Some(Arc::new(Pins(feed))), Arc::new(fetcher), Arc::new(folders), helpers, documents, Arc::new(|| {}));
        (probed(Shell::without_workspace(today()), tools).with_first_run(onboarding, home.clone()), home)
    }

    /// A harness in the first window on `shell`, which renders where snapshots are compared.
    fn first_window(mut shell: Shell, theme: Theme) -> Harness<'static> {
        with_renderer(Harness::builder().with_size(FIRST_WINDOW).with_pixels_per_point(1.5).with_theme(theme).with_context_setup(install_look))
            .build_ui(move |ui| shell.show(ui))
    }

    /// A harness on a shell the test keeps, so it can act on the shell between harnesses.
    fn first_window_on(shell: &mut Shell) -> Harness<'_> {
        Harness::builder().with_size(FIRST_WINDOW).with_context_setup(install_look).build_ui(move |ui| shell.show(ui))
    }

    /// Steps until `label` shows, as the worker's news arrives, then lets the news that follows it settle.
    fn until_shown(harness: &mut Harness, label: &str) {
        let deadline = Instant::now() + PATIENCE;
        while harness.query_by_label_contains(label).is_none() {
            assert!(Instant::now() < deadline, "{label} never showed");
            harness.run_steps(1);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(30));
        harness.run_steps(4);
    }

    /// Clicks the button or link named `label` once it shows and is enabled, as a worker's end turns it on.
    fn click(harness: &mut Harness, label: &str) {
        let deadline = Instant::now() + PATIENCE;
        while harness.query_by_label(label).is_none_or(|node| node.accesskit_node().is_disabled()) {
            assert!(Instant::now() < deadline, "{label} never turned on");
            harness.run_steps(1);
            std::thread::sleep(Duration::from_millis(5));
        }
        harness.get_by_label(label).click();
        harness.run_steps(2);
    }

    /// Moves the pointer off the window, so no button the clicks left under it shows its hover look, then compares.
    fn snapshot(harness: &mut Harness, name: &str) {
        harness.input_mut().events.push(egui::Event::PointerGone);
        harness.run_steps(3);
        compare(harness, name);
    }

    /// Clicks through `steps`, then renders the first window once `label` shows, and compares it to `name`.
    fn render_first_run(script: Script, theme: Theme, name: &str, steps: &[&str], label: &str) {
        let (shell, _) = first_run_shell(name, script);
        let mut harness = first_window(shell, theme);
        for step in steps {
            click(&mut harness, step);
        }
        until_shown(&mut harness, label);
        snapshot(&mut harness, name);
    }

    const TO_DOWNLOAD: [&str; 2] = ["Start setup", "Save here"];
    const TO_COMPUTER: [&str; 3] = ["Start setup", "Save here", "Continue"];
    const TO_INSTALL: [&str; 4] = ["Start setup", "Save here", "Continue", "Install what is missing"];

    #[test]
    fn onboarding_welcome_light() {
        render_first_run(Script::default(), Theme::Light, "onboarding_welcome_light", &[], "Welcome to Jobdesk");
    }

    #[test]
    fn onboarding_welcome_dark() {
        render_first_run(Script::default(), Theme::Dark, "onboarding_welcome_dark", &[], "Welcome to Jobdesk");
    }

    /// The Folder step done, as a setup that stopped before the Computer step leaves its record.
    #[test]
    fn onboarding_resume_light() {
        let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("onboarding_resume_light/home/Documents/Job search");
        let record = Progress { folder: Some(folder), done: BTreeSet::from([Step::Folder]), stopped: Some(today()), ..Progress::default() };
        render_first_run(Script { record: Some(record), ..Script::default() }, Theme::Light, "onboarding_resume_light", &[], "Continue setup");
    }

    /// The Folder and Computer steps done, with a draft kept: the welcome back offers Finish my profile.
    #[test]
    fn onboarding_resume_profile_light() {
        let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("onboarding_resume_profile_light/home/Documents/Job search");
        let draft = jobdesk::onboarding::application::Draft { run: "r1".into(), turn: 3, answers: vec![(1, "Design Lead".into())], text: String::new() };
        let record = Progress {
            folder: Some(folder),
            done: BTreeSet::from([Step::Folder, Step::Computer]),
            stopped: Some(today()),
            run: Some("r1".into()),
            draft: Some(draft),
            ..Progress::default()
        };
        render_first_run(Script { record: Some(record), ..Script::default() }, Theme::Light, "onboarding_resume_profile_light", &[], "Finish my profile");
    }

    /// With a data folder, Continue after the Computer step opens the folder and the Profile chapter over it, and the
    /// window grows to the app's size once.
    #[test]
    fn continue_after_the_computer_step_enters_the_profile_chapter() {
        use egui::ViewportCommand::InnerSize;
        let (shell, home) = first_run_shell("continue_enters_profile", Script::default());
        let data = home.join("data");
        fs::create_dir_all(&data).unwrap();
        let (mut harness, shell) = shared_first_window(shell.with_data_folder(data));
        for step in TO_INSTALL {
            click(&mut harness, step);
        }
        until(&mut harness, |harness| harness.query_by_label("Continue").is_some_and(|node| !node.accesskit_node().is_disabled()), "Continue turning on");
        harness.get_by_label("Continue").click();
        let sent = sent_over(&mut harness, 12);
        until_shown(&mut harness, "Tell Jobdesk about you");
        let grown = sent.iter().filter(|command| matches!(command, InnerSize(_))).count();
        assert_eq!(grown, 1, "one grow: {sent:?}");
        assert_eq!(harness.ctx.content_rect().size(), egui::vec2(1280.0, 820.0));
        assert!(shell.borrow().in_profile_setup());
        assert!(shell.borrow_mut().first_run_mut().is_none(), "the Folder and Computer steps are behind");
        assert!(harness.query_by_label("New task").is_none(), "no menu in the chapter");
    }

    #[test]
    fn onboarding_place_light() {
        render_first_run(Script::default(), Theme::Light, "onboarding_place_light", &["Start setup"], "Save here");
    }

    #[test]
    fn onboarding_notempty_light() {
        let (mut shell, home) = first_run_shell("onboarding_notempty_light", Script::default());
        let projects = home.join("Documents/Projects");
        fs::create_dir_all(&projects).unwrap();
        fs::write(projects.join("notes.txt"), "Fictional notes.").unwrap();
        click(&mut first_window_on(&mut shell), "Start setup");
        // Change… opens the system picker, so the folder it would give is chosen as the shell does once it answers.
        shell.first_run_mut().expect("first-run setup shows").choose(projects);
        let mut harness = first_window(shell, Theme::Light);
        until_shown(&mut harness, "Use this new folder");
        snapshot(&mut harness, "onboarding_notempty_light");
    }

    #[test]
    fn onboarding_download_light() {
        let script = Script { hold_download: true, ..Script::default() };
        render_first_run(script, Theme::Light, "onboarding_download_light", &TO_DOWNLOAD, "The latest version, 1.7.5");
    }

    #[test]
    fn onboarding_offline_light() {
        let script = Script { feed: Some(Err(FeedFailure::Unreachable)), ..Script::default() };
        render_first_run(script, Theme::Light, "onboarding_offline_light", &TO_DOWNLOAD, "Jobdesk could not reach the internet.");
    }

    #[test]
    fn onboarding_mismatch_light() {
        let script = Script { mismatch: true, ..Script::default() };
        render_first_run(script, Theme::Light, "onboarding_mismatch_light", &TO_DOWNLOAD, "This download doesn't match.");
    }

    /// Python found, Bun being checked, LaTeX next.
    #[test]
    fn onboarding_checking_light() {
        let script = Script { here: vec![Helper::Python], hold_check: Some(Helper::Bun), ..Script::default() };
        render_first_run(script, Theme::Light, "onboarding_checking_light", &TO_COMPUTER, "Already here");
    }

    /// Python found, Bun installed, LaTeX a third downloaded.
    #[test]
    fn onboarding_installing_light() {
        let script = Script { here: vec![Helper::Python], hold_install: Some(Helper::Latex), ..Script::default() };
        render_first_run(script, Theme::Light, "onboarding_installing_light", &TO_INSTALL, "Downloading");
    }

    /// Python found, Bun installed, LaTeX stopped.
    #[test]
    fn onboarding_toolfail_light() {
        let script = Script { here: vec![Helper::Python], fails: BTreeMap::from([(Helper::Latex, SetupFailure::Offline)]), ..Script::default() };
        render_first_run(script, Theme::Light, "onboarding_toolfail_light", &TO_INSTALL, "Could not install");
    }

    #[test]
    fn start_setup_leads_to_save_location_in_documents() {
        let (mut shell, _) = first_run_shell("start_setup_leads_to_place", Script::default());
        let mut harness = first_window_on(&mut shell);
        until_shown(&mut harness, "Welcome to Jobdesk");
        assert!(harness.query_by_label_contains("New task").is_none(), "no menu until setup hands over");
        click(&mut harness, "Start setup");
        until_shown(&mut harness, "Where should your job search live?");
        assert!(harness.query_by_label_contains("A new folder in your Documents folder.").is_some());
        click(&mut harness, "Back");
        until_shown(&mut harness, "Welcome to Jobdesk");
    }

    #[test]
    fn a_folder_with_files_leads_to_a_new_folder_inside_it() {
        let (mut shell, home) = first_run_shell("a_folder_with_files_is_refused", Script::default());
        let full = home.join("Documents/Old stuff");
        fs::create_dir_all(&full).unwrap();
        fs::write(full.join("letter.txt"), "Fictional letter.").unwrap();
        click(&mut first_window_on(&mut shell), "Start setup");
        shell.first_run_mut().expect("first-run setup shows").choose(full.clone());
        let mut harness = first_window_on(&mut shell);
        until_shown(&mut harness, "Jobdesk sets up only in a new or empty folder.");
        assert!(harness.query_by_label("Save here").is_none(), "a folder with files cannot be used");
        click(&mut harness, "Use this new folder");
        click(&mut harness, "Save here");
        until_shown(&mut harness, "Your folder is ready.");
        assert!(full.join("Job search/CLAUDE.md").is_file(), "set up inside the folder with files");
        assert_eq!(fs::read_to_string(full.join("letter.txt")).unwrap(), "Fictional letter.", "nothing of the user's changed");
    }

    #[test]
    fn cancel_during_the_download_leaves_the_folder_empty() {
        let (mut shell, home) = first_run_shell("cancel_leaves_the_folder_empty", Script { hold_download: true, ..Script::default() });
        let mut harness = first_window_on(&mut shell);
        for step in TO_DOWNLOAD {
            click(&mut harness, step);
        }
        until_shown(&mut harness, "The latest version, 1.7.5");
        click(&mut harness, "Cancel");
        until_shown(&mut harness, "Save here");
        assert!(fs::read_dir(home.join("Documents/Job search")).unwrap().next().is_none(), "the folder is still empty");
        assert!(harness.query_by_label_contains("An empty folder").is_some(), "Save location offers it again");
    }

    #[test]
    fn continue_without_pdfs_with_no_data_folder_opens_today_in_the_folder() {
        let script = Script { here: vec![Helper::Python, Helper::Bun], fails: BTreeMap::from([(Helper::Latex, SetupFailure::Offline)]), ..Script::default() };
        let (mut shell, home) = first_run_shell("continue_without_pdfs_leads_on", script);
        let mut harness = first_window_on(&mut shell);
        for step in TO_INSTALL {
            click(&mut harness, step);
        }
        until_shown(&mut harness, "Could not install");
        click(&mut harness, "Continue without PDFs");
        until_shown(&mut harness, "New task");
        assert!(harness.query_by_label("Start setup").is_none(), "setup has handed over");
        // LaTeX is still missing, so Setup stays and says so, while what was here reads as here.
        click(&mut harness, "Setup");
        until_shown(&mut harness, "CV: missing lualatex");
        assert!(harness.query_by_label_contains("missing python3").is_none());
        assert!(harness.query_by_label_contains("missing bun").is_none());
        drop(harness);
        assert!(shell.first_run_mut().is_none());
        assert!(home.join("Documents/Job search/CLAUDE.md").is_file());
    }

    /// Steps until `seen` holds of the harness, as the worker's news arrives.
    fn until(harness: &mut Harness, seen: impl Fn(&Harness) -> bool, what: &str) {
        let deadline = Instant::now() + PATIENCE;
        while !seen(harness) {
            assert!(Instant::now() < deadline, "{what} never happened");
            harness.run_steps(1);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The tool check at launch finds no Python, Bun or LaTeX. Once setup has installed them and handed over, the
    /// check runs again and finds them, so the Setup tab goes.
    #[test]
    fn the_helpers_setup_installed_are_found_once_it_hands_over() {
        let (mut shell, _) = first_run_shell("helpers_found_after_handover", Script::default());
        let mut harness = first_window_on(&mut shell);
        for step in TO_INSTALL {
            click(&mut harness, step);
        }
        click(&mut harness, "Continue");
        until_shown(&mut harness, "New task");
        assert_eq!(harness.ctx.content_rect().size(), egui::vec2(1280.0, 820.0), "the window grew to the app's own");
        until(&mut harness, |harness| harness.query_by_label("Setup").is_none(), "the Setup tab going");
    }

    /// Settings stays on the welcome of first-run setup, and its Change folder opens a folder the user already has.
    /// That leaves setup at once, with its download stopped, and grows the window to the app's own.
    #[test]
    fn a_folder_opened_from_settings_leaves_first_run() {
        let (mut shell, home) = first_run_shell("settings_folder_leaves_first_run", Script { hold_download: true, ..Script::default() });
        {
            let mut harness = first_window_on(&mut shell);
            for step in TO_DOWNLOAD {
                click(&mut harness, step);
            }
            until_shown(&mut harness, "The latest version, 1.7.5");
        }
        shell.open_settings(Page::General);
        let theirs = home.join("Their search");
        sample_workspace(&theirs);
        // Change folder opens the system picker, so the folder it would give is chosen as the shell does once it answers.
        shell.choose_folder_from(&egui::Context::default(), theirs);
        assert!(shell.first_run_mut().is_none(), "setup has handed over");
        assert!(fs::read_dir(home.join("Documents/Job search")).unwrap().next().is_none(), "the stopped download left the folder empty");
        let mut harness = first_window_on(&mut shell);
        harness.run_steps(3);
        assert_eq!(harness.ctx.content_rect().size(), egui::vec2(1280.0, 820.0), "the window grew to the app's own");
        assert!(harness.query_by_label("Start setup").is_none());
    }

    /// The window commands the shell sends over `steps` frames.
    fn sent_over(harness: &mut Harness, steps: usize) -> Vec<egui::ViewportCommand> {
        let mut sent = Vec::new();
        for _ in 0..steps {
            harness.run_steps(1);
            sent.extend(harness.output().viewport_output.get(&egui::ViewportId::ROOT).map(|viewport| viewport.commands.clone()).unwrap_or_default());
        }
        sent
    }

    /// A harness in the first window on a shell the test shares, so it can act on the shell while frames run.
    fn shared_first_window(shell: Shell) -> (Harness<'static>, Rc<RefCell<Shell>>) {
        let shared = Rc::new(RefCell::new(shell));
        let inner = Rc::clone(&shared);
        let harness = Harness::builder().with_size(FIRST_WINDOW).with_context_setup(install_look).build_ui(move |ui| inner.borrow_mut().show(ui));
        (harness, shared)
    }

    /// Tells the shell the window stands at `min` with `inner` under a 28-point title bar, as the system says once the
    /// window has taken a size and a position.
    fn place_window(harness: &mut Harness, min: egui::Pos2, inner: egui::Vec2) {
        let viewport = harness.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default();
        viewport.outer_rect = Some(egui::Rect::from_min_size(min, inner + egui::vec2(0.0, 28.0)));
        viewport.inner_rect = Some(egui::Rect::from_min_size(min + egui::vec2(0.0, 28.0), inner));
    }

    /// Settings needs the app's smallest window, so opening it over first-run setup grows the window to the app's own.
    /// Closing it while setup still shows gives the small first window back. Both keep the window's centre, so it
    /// ends where it started.
    #[test]
    fn settings_over_first_run_takes_the_app_size_and_gives_it_back() {
        use egui::ViewportCommand::{InnerSize, MinInnerSize, OuterPosition};
        let (small, app, smallest) = (egui::vec2(720.0, 560.0), egui::vec2(1280.0, 820.0), egui::vec2(1040.0, 640.0));
        let (start, grown_at) = (egui::pos2(400.0, 200.0), egui::pos2(120.0, 70.0));
        let (shell, _) = first_run_shell("settings_over_first_run", Script::default());
        let (mut harness, shell) = shared_first_window(shell);
        until_shown(&mut harness, "Welcome to Jobdesk");
        place_window(&mut harness, start, small);
        harness.get_by_role_and_label(egui::accesskit::Role::Button, "Settings").click();
        let sent = sent_over(&mut harness, 3);
        assert!(shell.borrow().settings_open());
        assert_eq!(sent, [InnerSize(app), MinInnerSize(smallest), OuterPosition(grown_at)], "Settings asks for the app's size round the same centre");
        assert_eq!(harness.ctx.content_rect().size(), app);
        place_window(&mut harness, grown_at, app);
        // Pressed in one frame, so that frame's commands are the ones read.
        let escape = egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        harness.input_mut().events.push(escape);
        let sent = sent_over(&mut harness, 3);
        assert!(!shell.borrow().settings_open());
        assert_eq!(sent, [MinInnerSize(small), InnerSize(small), OuterPosition(start)], "closing Settings gives the first window back where it was");
        assert_eq!(harness.ctx.content_rect().size(), small);
        assert!(harness.query_by_label_contains("Welcome to Jobdesk").is_some(), "setup still shows");
    }

    /// Every file under `root`, by its path inside it, with its bytes.
    fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let (mut found, mut folders) = (BTreeMap::new(), vec![root.to_owned()]);
        while let Some(folder) = folders.pop() {
            for entry in fs::read_dir(&folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                } else {
                    found.insert(path.strip_prefix(root).unwrap().to_owned(), fs::read(&path).unwrap());
                }
            }
        }
        found
    }

    /// A read on a stalled network does not see the cancel until it returns, so a folder opened from Settings hands
    /// over without waiting for it. The worker ends once the read returns, and writes only into its own folder, never
    /// into the one opened.
    #[test]
    fn a_folder_opened_from_settings_hands_over_while_a_read_is_stalled() {
        let stalled = Arc::new(AtomicBool::new(true));
        // A hand-over that waited for the worker would wait forever, so the read returns after a while anyway, and
        // the hand-over's time then fails the test instead of hanging it.
        let unstall = Arc::clone(&stalled);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(3));
            unstall.store(false, Ordering::SeqCst);
        });
        let (shell, home) = first_run_shell("settings_folder_while_stalled", Script { stall_download: Some(Arc::clone(&stalled)), ..Script::default() });
        let (mut harness, shell) = shared_first_window(shell);
        for step in TO_DOWNLOAD {
            click(&mut harness, step);
        }
        until_shown(&mut harness, "The latest version, 1.7.5");
        shell.borrow_mut().open_settings(Page::General);
        let theirs = home.join("Their search");
        sample_workspace(&theirs);
        let before = files(&theirs);
        let started = Instant::now();
        shell.borrow_mut().choose_folder_from(&harness.ctx.clone(), theirs.clone());
        assert!(started.elapsed() < Duration::from_secs(1), "the hand-over did not wait for the stalled read");
        assert!(shell.borrow_mut().first_run_mut().is_none(), "setup has handed over");
        harness.run_steps(3);
        assert!(shell.borrow().setup_stopping(), "the worker still ends behind the app");
        stalled.store(false, Ordering::SeqCst);
        until(&mut harness, |_| !shell.borrow().setup_stopping(), "the worker being joined");
        assert_eq!(files(&theirs), before, "nothing of the folder opened changed");
        assert!(home.join("Documents/Job search/CLAUDE.md").is_file(), "the worker unpacked only into its own folder");
    }

    /// An install past its last cancel check still puts its helper in place after the hand-over, so the tool check
    /// the hand-over started can miss it. Once the stopped worker is joined, the check runs again and finds it.
    #[test]
    fn a_helper_installed_after_the_hand_over_is_found_once_the_worker_ends() {
        let stalled = Arc::new(AtomicBool::new(true));
        let (shell, home) = first_run_shell("helper_after_handover", Script { stall_install: Some(Arc::clone(&stalled)), ..Script::default() });
        let (mut harness, shell) = shared_first_window(shell);
        for step in TO_INSTALL {
            click(&mut harness, step);
        }
        until_shown(&mut harness, "Python");
        shell.borrow_mut().open_settings(Page::General);
        let theirs = home.join("Their search");
        sample_workspace(&theirs);
        shell.borrow_mut().choose_folder_from(&harness.ctx.clone(), theirs);
        assert!(shell.borrow_mut().first_run_mut().is_none(), "setup has handed over");
        let python = |shell: &Rc<RefCell<Shell>>| {
            let mut shell = shell.borrow_mut();
            let checks = shell.environment_slot().as_ref().unwrap();
            (checks.is_running(), checks.version_of("python3").is_some())
        };
        until(&mut harness, |_| !python(&shell).0, "the hand-over's tool check");
        assert!(!python(&shell).1, "the hand-over's check ran before the install ended");
        stalled.store(false, Ordering::SeqCst);
        until(&mut harness, |_| !shell.borrow().setup_stopping(), "the worker being joined");
        until(&mut harness, |_| python(&shell) == (false, true), "the tool check finding Python");
    }

    /// A folder opened while Settings shows over first-run setup leaves setup, and the window keeps the app's size.
    #[test]
    fn a_folder_opened_from_settings_keeps_the_app_size() {
        let (shell, home) = first_run_shell("settings_folder_keeps_app_size", Script::default());
        let (mut harness, shell) = shared_first_window(shell);
        until_shown(&mut harness, "Welcome to Jobdesk");
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Comma);
        harness.run_steps(3);
        assert_eq!(harness.ctx.content_rect().size(), egui::vec2(1280.0, 820.0), "Cmd , grows the window for Settings");
        let theirs = home.join("Their search");
        sample_workspace(&theirs);
        shell.borrow_mut().choose_folder_from(&harness.ctx.clone(), theirs);
        assert!(shell.borrow_mut().first_run_mut().is_none(), "setup has handed over");
        let sent = sent_over(&mut harness, 3);
        assert!(
            !sent.iter().any(|command| matches!(command, egui::ViewportCommand::InnerSize(_) | egui::ViewportCommand::MinInnerSize(_))),
            "nothing asks for another size: {sent:?}"
        );
        assert_eq!(harness.ctx.content_rect().size(), egui::vec2(1280.0, 820.0));
    }
}

/// First-run setup's Profile chapter and its recap, in the window grown to the app's size, over a copy of the sample
/// folder. A scripted gateway plays Claude's side with the fictional `/setup` messages of `tests/fixtures/setup`, and
/// the documents are copies of the sample folder's PDFs under fictional names.
mod profile_chapter {
    use std::path::PathBuf;

    use jobdesk::onboarding::application::{PATH_A_REPLY, Progress, Step};

    use super::*;

    const FIVE: &str = include_str!("fixtures/setup/five_questions.md");
    const THIRTEEN: &str = include_str!("fixtures/setup/thirteen_questions.md");
    const SIXTEEN: &str = include_str!("fixtures/setup/sixteen_changes.md");
    const UNNUMBERED: &str = include_str!("fixtures/setup/unnumbered.md");
    const FOLLOW_UPS: &str = include_str!("fixtures/setup/follow_ups.md");
    const PATH_QUESTION: &str = "Welcome to the AI Job Search setup!\n\nThree ways to start:\n\n**Path A: Documents folder**\n\nWhich would you like?";
    const COMPLETE: &str = "**Setup complete!** Here's what was generated:\n\n- `CLAUDE.md` - Your full candidate profile\n\n**Try it out:**\n- Run `/scrape` to search for matching jobs right now";
    const PATIENCE: Duration = Duration::from_secs(10);

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    /// The local time the shell reads, so the draft's time and the run's start hold still.
    fn wall() -> NaiveDateTime {
        today().and_hms_opt(14, 32, 30).unwrap()
    }

    /// A file dropped on the window, by its path.
    #[derive(Debug)]
    struct Dropped(PathBuf);

    impl egui::DroppedFile for Dropped {
        fn path(&self) -> &Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            fs::read(&self.0).map_err(|error| error.to_string())
        }
    }

    /// A shell on the chapter, the gateway's sink and calls, the job search folder, and the folder documents come from.
    struct Scene {
        shell: Rc<RefCell<Shell>>,
        harness: Harness<'static>,
        sink: SharedSink,
        calls: SharedCalls,
        root: PathBuf,
        desktop: PathBuf,
        /// The fictional data folder that holds the progress record.
        data: PathBuf,
        opened: Rc<RefCell<Vec<String>>>,
    }

    fn scene(name: &str, theme: Theme, without_pdfs: bool) -> Scene {
        scene_sized(name, theme, without_pdfs, [1280.0, 820.0])
    }

    /// The scene in a window of `size`.
    fn scene_sized(name: &str, theme: Theme, without_pdfs: bool, size: [f32; 2]) -> Scene {
        let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = fs::remove_dir_all(&base);
        let root = base.join("ai-job-search");
        sample_workspace(&root);
        // One master CV, the one setup builds, and no other.
        fs::copy(root.join("cv/main_harbor.pdf"), root.join("cv/main_example.pdf")).unwrap();
        fs::remove_file(root.join("cv/main_harbor.pdf")).unwrap();
        fs::remove_file(root.join("cv/main_tessel_group_solution_architect.pdf")).unwrap();
        let desktop = base.join("Desktop");
        fs::create_dir_all(&desktop).unwrap();
        for file in ["Mira_Hale_CV.pdf", "Profile.pdf"] {
            fs::copy(root.join("cv/main_example.pdf"), desktop.join(file)).unwrap();
        }
        let data = base.join("data");
        fs::create_dir_all(&data).unwrap();
        let snapshot = Snapshot { usage_read: true, ..Snapshot::load(&root, today()) };
        let progress = Progress { folder: Some(root.clone()), done: [Step::Folder, Step::Computer].into(), ..Progress::default() };
        let run = jobdesk::onboarding::profile_run(&data, root.clone(), progress, Arc::new(|| {}), today());
        let (sink, calls) = (SharedSink::default(), SharedCalls::default());
        let mut shell = Shell::with_snapshot(Screen::InTray, jobdesk::workspace::open(&root).unwrap(), snapshot)
            .with_now(wall())
            .with_page_store(jobdesk::dossier::page_store(Arc::new(|| {})))
            .with_profile_setup(run, without_pdfs, "Documents/Job search");
        *shell.desk_slot() = Some(desk_on(RecordingGateway(Arc::clone(&sink), Arc::clone(&calls))));
        let shell = Rc::new(RefCell::new(shell));
        let (inner, opened) = (Rc::clone(&shell), Rc::new(RefCell::new(Vec::new())));
        let seen = Rc::clone(&opened);
        let harness = with_renderer(Harness::builder().with_size(size).with_pixels_per_point(1.5).with_theme(theme).with_context_setup(install_look)).build_ui(
            move |ui| {
                inner.borrow_mut().show(ui);
                let urls: Vec<String> = ui.ctx().output(|output| {
                    output
                        .commands
                        .iter()
                        .filter_map(|command| if let egui::OutputCommand::OpenUrl(open) = command { Some(open.url.clone()) } else { None })
                        .collect()
                });
                seen.borrow_mut().extend(urls);
            },
        );
        Scene { shell, harness, sink, calls, root, desktop, data, opened }
    }

    /// What eframe keeps between launches, in memory.
    #[derive(Default)]
    struct Kept(std::collections::HashMap<String, String>);

    impl eframe::Storage for Kept {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    impl Scene {
        /// The folder the shell would open on the next launch, as eframe saves it.
        fn remembered(&self) -> Option<String> {
            let mut kept = Kept::default();
            eframe::App::save(&mut *self.shell.borrow_mut(), &mut kept);
            kept.0.get("workspace").cloned()
        }

        /// The progress record as it stands in the data folder.
        fn record(&self) -> Option<String> {
            fs::read_to_string(self.data.join("onboarding.json")).ok()
        }
    }

    /// The record stays through the chapter and goes when the recap opens. The setup folder is remembered only from then.
    #[test]
    fn the_recap_clears_the_record_and_the_folder_is_remembered_from_then() {
        let mut scene = scene("recap_clears_record", Theme::Light, false);
        assert_eq!(scene.remembered(), None, "not while the profile is not built");
        scene.past_five();
        assert!(scene.record().is_some(), "the record stays while setup goes on");
        assert_eq!(scene.remembered(), None);
        scene.says(COMPLETE);
        scene.until_shown("Your job search is ready");
        assert_eq!(scene.record(), None, "the recap clears it");
        assert_eq!(scene.remembered(), Some(scene.root.display().to_string()));
    }

    /// Another folder opened during setup leaves the chapter, and that folder is remembered.
    #[test]
    fn another_folder_opened_during_setup_is_remembered() {
        let scene = scene("other_folder_during_setup", Theme::Light, false);
        let theirs = scene.root.parent().unwrap().join("Their search");
        sample_workspace(&theirs);
        let ctx = scene.harness.ctx.clone();
        scene.shell.borrow_mut().choose_folder_from(&ctx, theirs.clone());
        assert!(!scene.shell.borrow().in_profile_setup(), "the chapter is left");
        assert_eq!(scene.remembered(), Some(theirs.display().to_string()));
    }

    /// The setup folder opened again from Settings stays pending: it is not remembered, so the next open still offers
    /// Finish my profile, and its record stays.
    #[test]
    fn the_setup_folder_opened_again_stays_pending() {
        let mut scene = scene("setup_folder_again", Theme::Light, false);
        scene.at(FIVE);
        scene.until_shown("Claude has 5 questions");
        scene.click("Stop setup");
        assert!(!scene.shell.borrow().in_profile_setup(), "Stop setup opens the app as Finish later does");
        let ctx = scene.harness.ctx.clone();
        let root = scene.root.clone();
        scene.shell.borrow_mut().choose_folder_from(&ctx, root);
        assert_eq!(scene.remembered(), None, "the setup folder is not remembered");
        assert!(scene.record().is_some(), "and its record stays");
        // Another folder after it is remembered, so the opens above did happen.
        let theirs = scene.root.parent().unwrap().join("Their search");
        sample_workspace(&theirs);
        scene.shell.borrow_mut().choose_folder_from(&ctx, theirs.clone());
        assert_eq!(scene.remembered(), Some(theirs.display().to_string()));
    }

    /// A draft that waits for its next write is written as the window closes.
    #[test]
    fn closing_the_window_writes_a_waiting_draft() {
        let mut scene = scene("exit_writes_draft", Theme::Light, false);
        scene.at(FIVE);
        scene.until_shown("Claude has 5 questions");
        scene.type_into(0, "Design Lead");
        scene.type_into(1, "The whole studio");
        assert!(scene.shell.borrow_mut().profile_setup_mut().unwrap().is_unsaved(), "the second answer waits for its write");
        assert!(!scene.record().unwrap_or_default().contains("The whole studio"));
        eframe::App::on_exit(&mut *scene.shell.borrow_mut());
        assert!(scene.record().unwrap_or_default().contains("The whole studio"), "written as the window closed");
    }

    impl Scene {
        fn step(&mut self) {
            self.harness.run_steps(4);
        }

        fn until(&mut self, what: &str, seen: impl Fn(&Self) -> bool) {
            let deadline = Instant::now() + PATIENCE;
            while !seen(self) {
                assert!(Instant::now() < deadline, "{what} never happened");
                self.harness.run_steps(1);
                std::thread::sleep(Duration::from_millis(5));
            }
            self.step();
        }

        fn shown(&self, label: &str) -> bool {
            self.harness.query_all_by_label_contains(label).next().is_some()
        }

        fn until_shown(&mut self, label: &str) {
            let label = label.to_owned();
            self.until(&label.clone(), move |scene| scene.shown(&label));
        }

        fn click(&mut self, label: &str) {
            self.until(label, |scene| scene.harness.query_by_label(label).is_some_and(|node| !node.accesskit_node().is_disabled()));
            self.harness.get_by_label(label).click();
            self.step();
        }

        fn emit(&self, events: impl IntoIterator<Item = AgentEvent>) {
            emit_all(&self.sink, events);
        }

        fn clock(&mut self, seconds: f64) {
            clock_at(self.shell.borrow_mut().desk_slot().as_mut().unwrap(), seconds);
        }

        fn sent(&self) -> Vec<String> {
            self.calls.lock().unwrap().sent.clone()
        }

        /// Drops `files` from the fictional desktop on the window, as Finder does, and waits until each is looked at.
        fn drop_files(&mut self, files: &[&str]) {
            let dropped = files.iter().map(|file| Arc::new(Dropped(self.desktop.join(file))) as egui::DroppedFileHandle).collect();
            self.harness.input_mut().dropped_files = dropped;
            self.harness.run_steps(1);
            self.until("the documents looked at", |scene| {
                let mut shell = scene.shell.borrow_mut();
                let run = shell.profile_setup_mut().unwrap();
                !run.documents().is_empty() && !run.documents().is_looking()
            });
        }

        /// The two documents dropped, Read my documents, and the run started with its session.
        fn start(&mut self) {
            self.drop_files(&["Mira_Hale_CV.pdf", "Profile.pdf"]);
            self.click("Read my documents");
            self.until("the run starting", |scene| scene.calls.lock().unwrap().starts == 1);
            self.emit([AgentEvent::SessionStarted { session_id: "5e7a0c11-setup".into(), model: "claude-opus-5-5".into(), api_key_source: "none".into() }]);
            self.step();
        }

        /// Claude asks for the path, and Jobdesk answers it alone.
        fn path(&mut self) {
            self.emit([AgentEvent::AssistantText { text: PATH_QUESTION.into(), nested: false }, stop_point()]);
            self.until("the path sent", |scene| scene.sent() == [PATH_A_REPLY]);
        }

        /// Claude reads the documents.
        fn reads(&mut self) {
            let cv = self.root.join("documents/cv/Mira_Hale_CV.pdf").display().to_string();
            self.emit([tool("r1", "Read", &cv, false), done("r1")]);
            self.step();
        }

        /// Claude's turn ends on `message`.
        fn says(&mut self, message: &str) {
            self.emit([AgentEvent::AssistantText { text: message.into(), nested: false }, stop_point()]);
            self.step();
        }

        /// Started, the path sent, the documents read, and Claude's turn ended on `message`.
        fn at(&mut self, message: &str) {
            self.start();
            self.path();
            self.reads();
            // Claude's turn ends 18 minutes in.
            self.clock(1_090.0);
            self.says(message);
        }

        /// At the five questions, answered with the first field only.
        fn past_five(&mut self) {
            self.at(FIVE);
            self.until_shown("Claude has 5 questions");
            self.type_into(0, "Design Lead");
            self.click("Send answers");
            self.until("the answers sent", |scene| scene.sent().len() == 2);
        }

        /// Types `text` into the `at`th answer field.
        fn type_into(&mut self, at: usize, text: &str) {
            let field = self.harness.query_all_by_role(egui::accesskit::Role::MultilineTextInput).nth(at).expect("a field");
            field.focus();
            self.harness.run_steps(1);
            let field = self.harness.query_all_by_role(egui::accesskit::Role::MultilineTextInput).nth(at).expect("a field");
            field.type_text(text);
            self.step();
        }

        /// The `at`th change's checkbox, counting from 0.
        fn untick(&mut self, at: usize) {
            // By the keyboard, so a change scrolled out of view flips too.
            self.harness.query_all_by_role(egui::accesskit::Role::CheckBox).nth(at).expect("a change").focus();
            self.harness.run_steps(1);
            self.harness.key_press(Key::Space);
            self.step();
        }

        fn snapshot(&mut self, name: &str) {
            self.harness.input_mut().events.push(egui::Event::PointerGone);
            self.step();
            compare(&mut self.harness, name);
        }
    }

    fn render(name: &str, theme: Theme, play: impl FnOnce(&mut Scene), label: &str) {
        let mut scene = scene(name, theme, false);
        play(&mut scene);
        scene.until_shown(label);
        scene.snapshot(name);
    }

    #[test]
    fn onboarding_docs_light() {
        render("onboarding_docs_light", Theme::Light, |_| {}, "Drop your documents here");
    }

    #[test]
    fn onboarding_dropped_light() {
        render("onboarding_dropped_light", Theme::Light, |scene| scene.drop_files(&["Mira_Hale_CV.pdf", "Profile.pdf"]), "2 documents added");
    }

    #[test]
    fn onboarding_reading_light() {
        render(
            "onboarding_reading_light",
            Theme::Light,
            |scene| {
                scene.start();
                scene.path();
                scene.clock(126.0);
                scene.reads();
                scene.clock(130.0);
            },
            "Last update 4s ago",
        );
    }

    #[test]
    fn onboarding_working_light() {
        render(
            "onboarding_working_light",
            Theme::Light,
            |scene| {
                scene.past_five();
                scene.clock(1_260.0);
                let profile = scene.root.join("CLAUDE.md").display().to_string();
                scene.emit([tool("w1", "Write", &profile, false), done("w1")]);
                scene.step();
                scene.clock(1_308.0);
            },
            "No update for 48s",
        );
    }

    #[test]
    fn onboarding_sheet_light() {
        render("onboarding_sheet_light", Theme::Light, sheet, "Claude has 13 questions");
    }

    #[test]
    fn onboarding_sheet_dark() {
        render("onboarding_sheet_dark", Theme::Dark, sheet, "Claude has 13 questions");
    }

    /// The thirteen questions, with two answered and the draft saved.
    fn sheet(scene: &mut Scene) {
        scene.at(THIRTEEN);
        scene.until_shown("Claude has 13 questions");
        scene.type_into(0, "Design lead, principal designer");
        scene.type_into(1, "b, with a hand in the craft");
        saved(scene);
    }

    /// Waits until the draft's next write is due, so the slip says when it saved.
    fn saved(scene: &mut Scene) {
        std::thread::sleep(Duration::from_millis(1_100));
        scene.step();
    }

    #[test]
    fn onboarding_sheet_fallback_light() {
        render(
            "onboarding_sheet_fallback_light",
            Theme::Light,
            |scene| {
                scene.at(UNNUMBERED);
                scene.until_shown("Claude has a few questions");
                scene.type_into(0, "Design systems work in Lowmere, or remote. Leave out the 2021 studio closure.");
                saved(scene);
            },
            "Your answer",
        );
    }

    #[test]
    fn onboarding_review_light() {
        render("onboarding_review_light", Theme::Light, |scene| scene.at(SIXTEEN), "16 of 16 changes");
    }

    #[test]
    fn onboarding_review_skip_light() {
        render(
            "onboarding_review_skip_light",
            Theme::Light,
            |scene| {
                scene.at(SIXTEEN);
                scene.until_shown("16 of 16 changes");
                scene.untick(2);
                scene.untick(6);
            },
            "14 of 16 changes",
        );
    }

    #[test]
    fn onboarding_followup_light() {
        render(
            "onboarding_followup_light",
            Theme::Light,
            |scene| {
                scene.past_five();
                scene.clock(1_590.0);
                scene.says(FOLLOW_UPS);
                scene.until_shown("Two quick checks");
                scene.type_into(0, "Only the country, please.");
            },
            "Two quick checks",
        );
    }

    /// Claude built the CV this turn: its pages, once rendered.
    fn cv_check(scene: &mut Scene) {
        scene.past_five();
        scene.emit([tool("b1", "Bash", "cd cv && lualatex -interaction=nonstopmode main_example.tex", false), done("b1")]);
        scene.clock(1_870.0);
        scene.says("Your CV is built. Have a look at cv/main_example.pdf before we go on.");
        scene.until_shown("Check your CV");
        scene.until_shown("Page 1 of");
    }

    #[test]
    fn onboarding_cvcheck_light() {
        render("onboarding_cvcheck_light", Theme::Light, cv_check, "The CV looks good.");
    }

    #[test]
    fn onboarding_choice_light() {
        render(
            "onboarding_choice_light",
            Theme::Light,
            |scene| {
                scene.past_five();
                scene.clock(1_620.0);
                let option = |label: &str, description: &str| QuestionOption { label: label.into(), description: description.into() };
                scene.emit([AgentEvent::PromptRaised(Prompt {
                    request_id: "q1".into(),
                    kind: PromptKind::Questions(vec![Question {
                        text: "Which language should your CVs be in?".into(),
                        header: "Language".into(),
                        options: vec![
                            option("English", "Accepted in most countries. Cover letters still follow each job ad's language."),
                            option("German", "Matches most job ads where you live."),
                        ],
                        multi_select: false,
                    }]),
                    input_json: "{}".into(),
                    suggestions_json: None,
                })]);
                scene.step();
                scene.until_shown("Claude has a question");
                scene.click("English");
            },
            "Which language should your CVs be in?",
        );
    }

    #[test]
    fn onboarding_stopped_light() {
        render(
            "onboarding_stopped_light",
            Theme::Light,
            |scene| {
                scene.past_five();
                let profile = scene.root.join("CLAUDE.md").display().to_string();
                scene.emit([
                    tool("w1", "Write", &profile, false),
                    done("w1"),
                    AgentEvent::TurnEnded { failed: true, message: Some("The connection to Claude dropped.\u{202e}".into()), cost_usd: 1.0, duration_ms: 1 },
                ]);
                scene.step();
            },
            "Setup stopped partway",
        );
    }

    /// Setup complete after the five answers, 41 minutes in.
    fn complete(scene: &mut Scene) {
        scene.past_five();
        scene.clock(2_400.0);
        let profile = scene.root.join("CLAUDE.md").display().to_string();
        scene.emit([tool("w1", "Write", &profile, false), done("w1")]);
        scene.clock(2_460.0);
        scene.says(COMPLETE);
    }

    #[test]
    fn onboarding_recap_light() {
        render("onboarding_recap_light", Theme::Light, complete, "Your job search is ready");
    }

    #[test]
    fn onboarding_recap_dark() {
        render("onboarding_recap_dark", Theme::Dark, complete, "Your job search is ready");
    }

    #[test]
    fn onboarding_recap_nopdf_light() {
        let mut scene = scene("onboarding_recap_nopdf_light", Theme::Light, true);
        complete(&mut scene);
        scene.until_shown("Your job search is ready");
        assert!(scene.shown("This computer is almost ready"));
        scene.snapshot("onboarding_recap_nopdf_light");
    }

    /// Send joins the fields into one numbered reply, in Claude's order, with Skipped for each empty one.
    #[test]
    fn send_answers_sends_exactly_the_joined_reply() {
        let mut scene = scene("send_answers_joined", Theme::Light, false);
        scene.at(FIVE);
        scene.until_shown("Claude has 5 questions");
        scene.type_into(0, "Design Lead");
        scene.type_into(2, "Lowmere, remote preferred");
        assert_eq!(scene.sent(), [PATH_A_REPLY], "nothing goes while the user types");
        scene.harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::Enter);
        scene.step();
        assert_eq!(scene.sent().last().unwrap(), "1. Design Lead\n2. Skipped\n3. Lowmere, remote preferred\n4. Skipped\n5. Skipped", "Cmd Enter sends");
        assert_eq!(scene.sent().len(), 2, "once");
        scene.until_shown("Claude is working on your profile");
    }

    #[test]
    fn unticking_3_and_7_sends_apply_all_except_3_and_7() {
        let mut scene = scene("review_except", Theme::Light, false);
        scene.at(SIXTEEN);
        scene.until_shown("16 of 16 changes");
        assert!(scene.shown("Yes, apply these changes."), "all ticked");
        scene.untick(2);
        scene.untick(6);
        assert!(scene.shown("Apply all except 3 and 7."), "the slip shows the reply as it will go");
        scene.click("Apply these changes");
        assert_eq!(scene.sent().last().unwrap(), "Apply all except 3 and 7.");
    }

    #[test]
    fn every_change_unticked_disables_apply() {
        let mut scene = scene("review_none", Theme::Light, false);
        scene.at(SIXTEEN);
        scene.until_shown("16 of 16 changes");
        for at in 0..16 {
            scene.untick(at);
        }
        assert!(scene.harness.get_by_label("Apply these changes").accesskit_node().is_disabled());
        assert!(scene.shown("Tick a change, or use Change something."));
        scene.harness.get_by_label("Apply these changes").click();
        scene.step();
        assert_eq!(scene.sent(), [PATH_A_REPLY], "nothing is sent");
    }

    /// A permission prompt that can be remembered offers Allow for this run, as Tasks does, and that answers it.
    #[test]
    fn a_permission_that_can_be_remembered_offers_allow_for_this_run() {
        let mut scene = scene("setup_permission_remember", Theme::Light, false);
        scene.shell.borrow_mut().desk_slot().as_mut().unwrap().set_asks_permission(true);
        scene.past_five();
        scene.emit([AgentEvent::PromptRaised(Prompt {
            request_id: "p1".into(),
            kind: PromptKind::Permission { tool: "Write".into(), detail: "CLAUDE.md".into(), can_remember: true },
            input_json: "{}".into(),
            suggestions_json: Some("[]".into()),
        })]);
        scene.until_shown("Claude asks to use a tool");
        assert!(scene.shown("Allow once") && scene.shown("Deny"));
        scene.click("Allow for this run");
        scene.until("the prompt answered", |scene| !scene.shown("Claude asks to use a tool"));
    }

    /// A question with several answers says so, and never asks to pick one.
    #[test]
    fn a_question_with_several_answers_says_to_pick_the_ones_that_fit() {
        let mut scene = scene("setup_multi_select", Theme::Light, false);
        scene.past_five();
        let option = |label: &str| QuestionOption { label: label.into(), description: String::new() };
        scene.emit([AgentEvent::PromptRaised(Prompt {
            request_id: "q2".into(),
            kind: PromptKind::Questions(vec![Question {
                text: "Which job sites do you use?".into(),
                header: "Sites".into(),
                options: vec![option("LinkedIn"), option("Freehire")],
                multi_select: true,
            }]),
            input_json: "{}".into(),
            suggestions_json: None,
        })]);
        scene.until_shown("Which job sites do you use?");
        assert!(scene.shown("Pick the answers that fit"));
        assert!(scene.shown("Pick as many as fit."));
        assert!(!scene.shown("Pick one"), "several answers are allowed");
    }

    /// Every block of Claude's message shows on the sheet, the review and the CV check, and Show all Claude wrote is on each.
    #[test]
    fn claudes_lead_and_closing_words_show_with_the_items() {
        let mut scene = scene("setup_blocks_show", Theme::Light, false);
        scene.at(&format!("{FIVE}\nTake your time with these."));
        scene.until_shown("Claude has 5 questions");
        assert!(scene.shown("I read all three documents"), "the lead");
        assert!(scene.shown("Take your time with these."), "the closing words");
        assert!(scene.shown("Show all Claude wrote"));

        let mut review = self::scene("setup_blocks_review", Theme::Light, false);
        review.at(SIXTEEN);
        review.until_shown("16 of 16 changes");
        assert!(review.shown("I have what I need."), "the lead");
        assert!(review.shown("Here is what I will write."), "the lead");
        assert!(!review.shown("list the numbers to skip"), "the ask to type numbers is left out of the card");
        review.click("Show all Claude wrote");
        review.until_shown("All Claude wrote");
        assert!(review.shown("list the numbers to skip"), "Show all Claude wrote still holds it");

        let mut check = self::scene("setup_blocks_cv", Theme::Light, false);
        cv_check(&mut check);
        assert!(check.shown("Your CV is built."), "Claude's message shows with the pages");
        assert!(check.shown("Show all Claude wrote"));
    }

    /// A long message with the CV leaves the pages their reading size: the caption under them sits where it does for a
    /// short one.
    #[test]
    fn a_long_message_with_the_cv_keeps_the_pages_their_size() {
        let caption = |message: &str, name: &str| {
            let mut scene = scene(name, Theme::Light, false);
            scene.past_five();
            scene.emit([tool("b1", "Bash", "cd cv && lualatex main_example.tex", false), done("b1")]);
            scene.says(message);
            scene.until_shown("Page 1 of");
            scene.harness.get_by_label("Page 1 of 1").rect().top()
        };
        let long = format!("Your CV is built.\n\n{}", "Here is one more thing to look at before we go on.\n\n".repeat(30));
        assert_eq!(caption("Your CV is built.", "cv_short_message"), caption(&long, "cv_long_message"));
    }

    /// The CV check in a window of `size`, Claude's turn ending on `message`.
    fn cv_check_sized(name: &str, size: [f32; 2], message: &str) -> Scene {
        let mut scene = scene_sized(name, Theme::Light, false, size);
        scene.past_five();
        scene.emit([tool("b1", "Bash", "cd cv && lualatex main_example.tex", false), done("b1")]);
        scene.says(message);
        scene.until_shown("Page 1 of");
        scene
    }

    /// With a long message, the reply Looks good sends and Show all Claude wrote still show above the foot.
    #[test]
    fn a_long_cv_message_leaves_the_reply_and_show_all_in_view() {
        let long = format!("Your CV is built.\n\n{}", "Here is one more thing to look at before we go on.\n\n".repeat(30));
        let scene = cv_check_sized("cv_long_side", [1280.0, 820.0], &long);
        let foot = scene.harness.get_by_label("Looks good").rect().top();
        for label in ["Looks good sends Claude this reply:", "Show all Claude wrote"] {
            assert!(scene.harness.get_by_label(label).rect().bottom() < foot, "{label} shows above the foot");
        }
    }

    /// In the smallest window the side column scrolls rather than cut its last slips off.
    #[test]
    fn the_side_column_scrolls_in_the_smallest_window() {
        let mut scene = cv_check_sized("cv_side_scrolls", [1040.0, 640.0], "Your CV is built.");
        let before = scene.harness.get_by_label("Show all Claude wrote").rect().top();
        scene.harness.hover_at(scene.harness.get_by_label("Master CV").rect().center());
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -400.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        };
        scene.harness.event(wheel);
        scene.step();
        let after = scene.harness.get_by_label("Show all Claude wrote").rect().top();
        assert!(after < before, "the side column scrolled: {before} to {after}");
        let foot = scene.harness.get_by_label("Looks good").rect().top();
        assert!(scene.harness.get_by_label("Show all Claude wrote").rect().bottom() <= foot, "and its last slip comes into view");
    }

    /// A link in Claude's message shows its words, and a click on them opens nothing.
    #[test]
    fn a_link_in_claudes_message_never_opens() {
        let mut scene = scene("claude_link_never_opens", Theme::Light, false);
        scene.at("Have a look at [the harbor site](https://harbor.example/jobs) and tell me which roles fit.");
        scene.until_shown("Claude has a few questions");
        for link in scene.harness.query_all_by_label_contains("the harbor site") {
            link.click();
        }
        scene.step();
        assert!(scene.opened.borrow().is_empty(), "{:?}", scene.opened.borrow());
    }

    /// A file name's reordering mark is gone from the list and from the names a screen reader hears.
    #[test]
    fn a_file_name_with_a_bidi_character_shows_without_it() {
        let mut scene = scene("bidi_file_name", Theme::Light, false);
        let odd = "Mira\u{202E}fdp.CV.pdf";
        fs::copy(scene.desktop.join("Mira_Hale_CV.pdf"), scene.desktop.join(odd)).unwrap();
        scene.drop_files(&[odd]);
        assert!(scene.shown("Mirafdp.CV.pdf"));
        assert!(scene.harness.query_by_label("Kind of Mirafdp.CV.pdf").is_some());
        assert!(scene.harness.query_by_label_contains("\u{202E}").is_none(), "no label holds the mark");
    }

    /// Each file's kind is guessed from its name, and the kind picked decides the subfolder of `documents/` it goes to.
    #[test]
    fn the_kind_picked_decides_the_documents_subfolder() {
        let mut scene = scene("kind_decides_subfolder", Theme::Light, false);
        scene.drop_files(&["Mira_Hale_CV.pdf", "Profile.pdf"]);
        let menu = scene.harness.get_by_label("Kind of Profile.pdf");
        assert_eq!(menu.accesskit_node().role(), egui::accesskit::Role::ComboBox);
        assert_eq!(menu.accesskit_node().value().as_deref(), Some("LinkedIn profile"));
        scene.harness.get_by_label("Kind of Profile.pdf").click();
        scene.step();
        scene.harness.get_by_label("Reference letter").click();
        scene.step();
        assert_eq!(scene.harness.get_by_label("Kind of Profile.pdf").accesskit_node().value().as_deref(), Some("Reference letter"));
        scene.click("Read my documents");
        scene.until("the run starting", |scene| scene.calls.lock().unwrap().starts == 1);
        assert!(scene.root.join("documents/references/Profile.pdf").is_file());
        assert!(scene.root.join("documents/cv/Mira_Hale_CV.pdf").is_file());
        assert!(scene.desktop.join("Profile.pdf").is_file(), "the original stays where it was");
    }

    /// A copy that stops at a file shows where, as plain text, and the file already copied keeps its kind and its row.
    #[test]
    fn after_a_copy_a_files_kind_menu_is_disabled() {
        let mut scene = scene("kind_after_copy", Theme::Light, false);
        scene.drop_files(&["Mira_Hale_CV.pdf", "Profile.pdf"]);
        fs::remove_file(scene.desktop.join("Profile.pdf")).unwrap();
        scene.click("Read my documents");
        scene.until_shown("The copy stopped at Profile.pdf");
        assert_eq!(scene.calls.lock().unwrap().starts, 0, "no run starts");
        assert!(scene.harness.get_by_label("Kind of Mira_Hale_CV.pdf").accesskit_node().is_disabled(), "copied");
        assert!(scene.harness.get_by_label("Remove Mira_Hale_CV.pdf").accesskit_node().is_disabled());
        assert!(!scene.harness.get_by_label("Kind of Profile.pdf").accesskit_node().is_disabled());
    }

    /// When setup said it is complete but a change asked for afterwards left no clean end, Finish setup opens the recap,
    /// only once pressed, and the recap clears the record. Open Jobdesk opens Today.
    #[test]
    fn finish_setup_opens_the_recap_and_open_jobdesk_opens_today() {
        let mut scene = scene("finish_setup", Theme::Light, false);
        scene.past_five();
        scene.says(COMPLETE);
        scene.until_shown("Your job search is ready");
        scene.click("Open Jobdesk");
        assert!(!scene.shell.borrow().in_profile_setup());
        scene.until_shown("New task");

        let mut scene = self::scene("finish_setup_link", Theme::Light, false);
        scene.past_five();
        scene.emit([tool("b1", "Bash", "cd cv && lualatex main_example.tex", false), done("b1")]);
        scene.says(COMPLETE);
        scene.until_shown("Check your CV");
        scene.click("Ask for a change");
        scene.harness.get_by_role(egui::accesskit::Role::TextInput).focus();
        scene.harness.run_steps(1);
        scene.harness.get_by_role(egui::accesskit::Role::TextInput).type_text("Put my phone number first.");
        scene.step();
        scene.click("Send");
        scene.until("the change sent", |scene| scene.sent().last().is_some_and(|last| last == "Put my phone number first."));
        scene.says("I moved it in the source. Anything else?");
        scene.until_shown("Finish setup");
        assert!(scene.shell.borrow().in_profile_setup(), "setup never ends on its own");
        scene.click("Finish setup");
        scene.until_shown("Your job search is ready");
        let sent = scene.sent();
        assert_eq!(sent.last().unwrap(), "Put my phone number first.", "Finish setup sends nothing");
    }

    /// Add a job site you use… only opens the launcher on `/add-portal`. No run starts.
    #[test]
    fn add_a_job_site_only_opens_the_launcher() {
        let mut scene = scene("recap_add_portal", Theme::Light, false);
        complete(&mut scene);
        scene.until_shown("Your job search is ready");
        assert!(!scene.shown("$"), "no spend on the recap");
        scene.click("Add a job site you use…");
        scene.until_shown("Pick a command. Nothing runs until you press Run.");
        assert_eq!(scene.calls.lock().unwrap().starts, 1, "only setup's run ever started");
    }
}

// The Portals screen.

const PORTALS_SUBTITLE: &str = "The job boards Claude searches when you find new jobs. 4 of 6 on.";
const PORTALS_BUSY: &str = "A task is running. Turn portals on or off once it ends.";
const BUILT_IN_STAYS: &str = "Built-in portals stay installed. Turn one off to skip it.";

/// Where `shell(Screen::Portals, name)` copied the sample folder.
fn copied_root(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search")
}

/// A portal folder's `SKILL.md` in the copy at `root`.
fn manifest_in(root: &Path, folder: &str) -> String {
    fs::read_to_string(root.join(".agents/skills").join(folder).join("SKILL.md")).unwrap()
}

/// The sample folder on Portals, with `change` made to it before it is read.
fn portals_shell_with(name: &str, change: impl FnOnce(&Path)) -> Shell {
    let root = copied_root(name);
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    change(&root);
    let snapshot = Snapshot { usage_read: true, ..Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()) };
    Shell::with_snapshot(Screen::Portals, jobdesk::workspace::open(&root).unwrap(), snapshot)
}

/// Records each write it is asked for, and writes nothing.
#[derive(Default, Clone)]
struct FakePortals {
    calls: Arc<Mutex<Vec<String>>>,
}

impl jobdesk::portals::application::PortalStore for FakePortals {
    fn set_enabled(&self, folder: &str, on: bool) -> Result<(), jobdesk::portals::application::ChangeError> {
        self.calls.lock().unwrap().push(format!("set {folder} {on}"));
        Ok(())
    }

    fn trash(&self, folder: &str) -> Result<(), jobdesk::portals::application::ChangeError> {
        self.calls.lock().unwrap().push(format!("trash {folder}"));
        Ok(())
    }
}

/// The sample on Portals, writing through a fake store whose calls come back.
fn faked_portals(name: &str) -> (Shell, Arc<Mutex<Vec<String>>>) {
    let fake = FakePortals::default();
    let calls = Arc::clone(&fake.calls);
    let changes = jobdesk::portals::application::PortalChanges::new(Box::new(fake));
    (probed(shell(Screen::Portals, name), FixedTools).with_portal_changes(changes), calls)
}

/// A portal's row in the list, by what it says to a screen reader.
fn portal_row<'a>(harness: &'a Harness<'static>, spoken: &'a str) -> egui_kittest::Node<'a> {
    harness.get_by_role_and_label(egui::accesskit::Role::Button, spoken)
}

fn pick_portal(harness: &mut Harness<'static>, spoken: &str) {
    portal_row(harness, spoken).click();
    harness.run_steps(3);
}

/// The Portals tab sits between Profile and Setup, and every tab fits the smallest window.
#[test]
fn the_portals_tab_sits_between_profile_and_setup() {
    let mut shell = probed(shell(Screen::Portals, "portals_tab_order"), FixedTools);
    let mut harness = Harness::builder().with_size([1040.0, 640.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
    settle(&mut harness);
    // The menu's tabs come first, then the screen's own three.
    let tabs: Vec<(String, f32)> =
        harness.get_all_by_role(egui::accesskit::Role::Tab).map(|tab| (tab.accesskit_node().label().unwrap_or_default(), tab.rect().left())).take(8).collect();
    let names: Vec<&str> = tabs.iter().map(|(label, _)| label.split(',').next().unwrap_or_default()).collect();
    assert_eq!(names, ["Today", "Jobs", "Applications", "Insights", "Tasks", "Profile", "Portals", "Setup"]);
    assert!(tabs.windows(2).all(|pair| pair[0].1 < pair[1].1), "{tabs:?}");
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1040.0, 640.0));
    for tab in harness.get_all_by_role(egui::accesskit::Role::Tab) {
        assert!(window.contains_rect(tab.rect()), "{:?} at {:?}", tab.accesskit_node().label(), tab.rect());
    }
    assert_in_window(&harness, [1040.0, 640.0], &["New task", "Check all portals…"]);
}

/// A switch writes the portal's `SKILL.md` in the folder at once. Its row keeps its place, so nothing moves under the
/// pointer, and joins the Off group once the screen opens again.
#[test]
fn a_switch_writes_its_portal_and_the_row_moves_only_when_the_screen_opens_again() {
    let name = "portals_switch";
    let mut harness = tray_harness(probed(shell(Screen::Portals, name), FixedTools));
    let root = copied_root(name);
    assert!(is_shown(&harness, PORTALS_SUBTITLE));
    let off_group = |harness: &Harness| harness.get_by_label("Claude skips these").rect().top();
    let before = portal_row(&harness, "Northfield Jobs").rect().top();
    assert!(before < off_group(&harness), "Northfield Jobs is on");

    harness.get_by_label("Northfield Jobs, on").click();
    harness.run_steps(3);

    assert!(manifest_in(&root, "northfield-search").contains("enabled: false"), "the copy's SKILL.md is off");
    assert!(is_shown(&harness, "The job boards Claude searches when you find new jobs. 3 of 6 on."), "read back at once");
    assert!(harness.query_all_by_label("Northfield Jobs, off").count() > 0);
    assert_eq!(portal_row(&harness, "Northfield Jobs").rect().top(), before, "the row keeps its place");

    harness.get_by_role_and_label(egui::accesskit::Role::Tab, "Jobs").click();
    harness.run_steps(3);
    harness.get_by_role_and_label(egui::accesskit::Role::Tab, "Portals").click();
    harness.run_steps(3);
    assert!(portal_row(&harness, "Northfield Jobs").rect().top() > off_group(&harness), "it moves to Off once the screen opens again");

    harness.get_all_by_label("Northfield Jobs, off").next().unwrap().click();
    harness.run_steps(3);
    assert!(!manifest_in(&root, "northfield-search").contains("enabled: false"), "on again");
}

/// Remove… asks first, with Keep it as the safe choice: Keep it writes nothing, and Remove moves the folder through the
/// portal store.
#[test]
fn remove_asks_first_and_keep_it_writes_nothing() {
    let (shell, calls) = faked_portals("portals_remove");
    let mut harness = tray_harness(shell);
    pick_portal(&mut harness, "Northfield Jobs");
    assert!(!is_shown(&harness, "Remove Northfield Jobs?"));

    harness.get_by_label("Remove…").click();
    harness.run_steps(3);
    assert!(harness.query_all_by_label("Remove Northfield Jobs?").count() > 0, "the ask opens");
    let trash = if cfg!(target_os = "macos") { "the Trash" } else { "the Recycle Bin" };
    assert!(harness.query_by_label_contains(&format!(".agents/skills/northfield-search, to {trash}.")).is_some());
    harness.get_by_label("Keep it").click();
    harness.run_steps(3);
    assert!(!is_shown(&harness, "Remove Northfield Jobs?"));
    assert!(calls.lock().unwrap().is_empty(), "Keep it writes nothing");

    harness.get_by_label("Remove…").click();
    harness.run_steps(3);
    harness.get_by_label("Remove").click();
    harness.run_steps(3);
    assert_eq!(*calls.lock().unwrap(), ["trash northfield-search"]);
    assert!(!is_shown(&harness, "Remove Northfield Jobs?"));
}

/// A built-in portal is only switched off: it has no Remove…, and says why.
#[test]
fn a_built_in_portal_has_no_remove() {
    let (shell, calls) = faked_portals("portals_built_in");
    let mut harness = tray_harness(shell);
    for spoken in ["Freehire, Worldwide", "LinkedIn, Worldwide", "Jobnet, Denmark"] {
        pick_portal(&mut harness, spoken);
        assert!(harness.query_by_label("Remove…").is_none(), "{spoken} has no Remove…");
        assert!(is_shown(&harness, BUILT_IN_STAYS), "{spoken} says why");
    }
    pick_portal(&mut harness, "Sample Freelance Board");
    assert!(harness.query_by_label("Remove…").is_some());
    assert!(!is_shown(&harness, BUILT_IN_STAYS));
    assert!(calls.lock().unwrap().is_empty());
}

/// While a run works, every switch and action waits and says why, and a click writes nothing. The rows still pick.
#[test]
fn a_running_task_holds_every_switch_and_action() {
    let (mut shell, calls) = faked_portals("portals_busy");
    *shell.desk_slot() = Some(apply_run_ending([]));
    let mut harness = tray_harness(shell);
    pick_portal(&mut harness, "Northfield Jobs");
    for label in ["Check all portals…", "Check this portal…", "Change search terms…", "Remove…"] {
        assert!(harness.get_by_label(label).accesskit_node().is_disabled(), "{label} waits");
    }
    for switch in harness.query_all_by_role(egui::accesskit::Role::CheckBox) {
        assert!(switch.accesskit_node().is_disabled(), "{:?} waits", switch.accesskit_node().label());
    }
    let switch = harness.get_all_by_label("Northfield Jobs, on").next().unwrap();
    switch.click();
    harness.run_steps(3);
    harness.get_by_label("Remove…").click();
    harness.run_steps(3);
    assert!(!is_shown(&harness, "Remove Northfield Jobs?"));
    assert!(calls.lock().unwrap().is_empty(), "nothing is written while a run works");
    harness.get_all_by_label("Northfield Jobs, on").next().unwrap().hover();
    harness.run_steps(8);
    assert!(is_shown(&harness, PORTALS_BUSY), "the switch says why it waits");
}

/// Each action fills the launcher's line and starts nothing.
#[test]
fn the_portal_actions_fill_the_launcher_and_start_nothing() {
    for (label, line) in [
        ("Check all portals…", "/scrape health"),
        ("Check this portal…", "/scrape health freehire-search"),
        ("Change search terms…", "/setup --section search"),
    ] {
        let started = SharedSink::default();
        let mut shell = shell(Screen::Portals, "portals_actions");
        *shell.desk_slot() = Some(desk_on(ScriptedGateway(Arc::clone(&started))));
        let mut harness = tray_harness(shell);
        harness.get_by_label(label).click();
        harness.run_steps(3);
        assert!(is_shown(&harness, LAUNCHER), "{label} opens the launcher");
        assert!(harness.query_all_by_label(line).count() > 0, "{label} fills in {line}");
        assert!(started.lock().unwrap().is_none(), "{label} starts no run");
    }
}

/// The detail says how the picked portal reads the shared terms: in quotes for one that honours them, bare for one that
/// ignores them, and the Google lines that name its site.
#[test]
fn the_detail_shows_each_term_as_the_portal_receives_it() {
    let mut harness = tray_harness(shell(Screen::Portals, "portals_terms"));
    pick_portal(&mut harness, "Sample Freelance Board");
    assert!(is_shown(&harness, "\"Lead Backend Engineer\""));
    assert!(harness.query_all_by_label("These words together").count() > 0);
    assert!(is_shown(&harness, "7 searches"));
    pick_portal(&mut harness, "LinkedIn, Worldwide");
    assert!(is_shown(&harness, "Lead Backend Engineer"));
    assert!(!is_shown(&harness, "\"Lead Backend Engineer\""));
    assert!(is_shown(&harness, "site:linkedin.com/jobs \"Lead Backend Engineer\" Brussels"));
    assert!(harness.query_by_label("site:jobs.example.com \"Platform Lead\" Brussels").is_none(), "only lines that name its site");
    pick_portal(&mut harness, "Northfield Jobs");
    assert!(is_shown(&harness, "None of your Google lines name this board, so Claude only notes that it failed."));
}

/// Setup's Portals panel is one line, and Open Portals opens the screen. Runtime status shows the same line.
#[test]
fn setup_sums_up_the_portals_and_opens_them() {
    let mut harness = tray_harness(probed(shell(Screen::Workspace, "setup_portals_line"), FixedTools));
    settle(&mut harness);
    assert!(is_shown(&harness, "4 of 6 portals on"));
    assert!(is_shown(&harness, "10 jobs found"));
    // The dot between the count and the jobs has the same room on each side.
    let (count, dot, jobs) = (harness.get_by_label("4 of 6 portals on").rect(), harness.get_by_label("·").rect(), harness.get_by_label("10 jobs found").rect());
    assert!(((dot.left() - count.right()) - (jobs.left() - dot.right())).abs() < 1.0, "{count:?} {dot:?} {jobs:?}");
    for gone in ["Jobs seen", "linkedin-search", "Check portal health", "Add portal"] {
        assert!(harness.query_by_label(gone).is_none(), "Setup still shows {gone}");
    }
    harness.get_by_label("Open Portals").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, PORTALS_SUBTITLE), "Portals is on show");

    let mut shell = shell(Screen::InTray, "runtime_portals_line");
    shell.open_settings(Page::Runtime);
    let shell = Rc::new(RefCell::new(probed(shell, FixedTools)));
    let inner = Rc::clone(&shell);
    // Tall enough that the Portals section is in view.
    let mut harness = Harness::builder().with_size([1280.0, 1600.0]).with_context_setup(install_look).build_ui(move |ui| inner.borrow_mut().show(ui));
    settle(&mut harness);
    assert!(is_shown(&harness, "4 of 6 portals on"));
    harness.get_by_label("Open Portals").click();
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open(), "Settings closes first");
    assert!(is_shown(&harness, PORTALS_SUBTITLE));
}

/// With no portal installed, Setup says so and keeps Open Portals, and the screen offers to add one.
#[test]
fn no_portal_installed_says_so() {
    let remove_all = |root: &Path| fs::remove_dir_all(root.join(".agents/skills")).unwrap();
    let mut setup = portals_shell_with("portals_none_setup", remove_all);
    setup = probed(setup, FixedTools);
    let mut harness = tray_harness(setup);
    harness.get_by_role_and_label(egui::accesskit::Role::Tab, "Setup").click();
    settle(&mut harness);
    assert!(is_shown(&harness, "No portals installed."));
    assert!(is_shown(&harness, "Open Portals"));
    harness.get_by_label("Open Portals").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "No portals yet"));
    assert!(is_shown(&harness, "The job boards Claude searches when you find new jobs. None installed."));
}

/// The sample on Portals as the canvas draws it: the community list saved 12 minutes ago, so the portal installed from
/// it reads Community.
fn render_portals(shell: Shell, pick: Option<&str>, theme: Theme) -> Harness<'static> {
    let shell = listed(shell, Some(community_list(minutes_ago(12))), vec![]);
    let mut harness = probed_wgpu(probed(shell, FixedTools), theme);
    if let Some(spoken) = pick {
        pick_portal(&mut harness, spoken);
    }
    harness
}

#[test]
fn portals_yours_light() {
    let mut harness = render_portals(shell(Screen::Portals, "portals_yours_light"), None, Theme::Light);
    compare(&mut harness, "portals_yours_light");
}

#[test]
fn portals_yours_dark() {
    let mut harness = render_portals(shell(Screen::Portals, "portals_yours_dark"), None, Theme::Dark);
    compare(&mut harness, "portals_yours_dark");
}

#[test]
fn portals_madebyyou_light() {
    let name = "portals_madebyyou_light";
    let mut harness = render_portals(shell(Screen::Portals, name), Some("Northfield Jobs"), Theme::Light);
    compare(&mut harness, name);
}

#[test]
fn portals_remove_light() {
    let name = "portals_remove_light";
    let mut harness = render_portals(shell(Screen::Portals, name), Some("Northfield Jobs"), Theme::Light);
    harness.get_by_label("Remove…").click();
    harness.run_steps(3);
    compare(&mut harness, name);
}

#[test]
fn portals_busy_light() {
    let name = "portals_busy_light";
    let mut shell = shell(Screen::Portals, name);
    *shell.desk_slot() = Some(apply_run_ending([]));
    let mut harness = render_portals(shell, None, Theme::Light);
    harness.get_by_label("Northfield Jobs, on").hover();
    harness.run_steps(8);
    compare(&mut harness, name);
}

#[test]
fn portals_loading_light() {
    let name = "portals_loading_light";
    let mut harness = render_portals(loading_on(Screen::Portals, name), None, Theme::Light);
    compare(&mut harness, name);
}

#[test]
fn portals_none_light() {
    let name = "portals_none_light";
    let shell = portals_shell_with(name, |root| fs::remove_dir_all(root.join(".agents/skills")).unwrap());
    let mut harness = render_portals(shell, None, Theme::Light);
    compare(&mut harness, name);
}

#[test]
fn portals_failed_light() {
    let name = "portals_failed_light";
    let mut harness = render_portals(broken_shell(Screen::Portals, name, &[Source::Portals]), None, Theme::Light);
    compare(&mut harness, name);
}

/// A snapshot whose read began before a switch never undoes it on screen: the portals read back after the switch stay
/// until a snapshot read after it arrives.
#[test]
fn a_snapshot_read_before_a_switch_keeps_the_switch() {
    let name = "portals_stale_snapshot";
    let mut shell = probed(shell(Screen::Portals, name), FixedTools);
    let feed = shell.snapshot_feed();
    let root = copied_root(name);
    let day = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
    let stale = Snapshot { usage_read: true, ..Snapshot::load(&root, day) };
    let mut harness = tray_harness(shell);
    harness.get_by_label("Northfield Jobs, on").click();
    harness.run_steps(3);
    assert!(manifest_in(&root, "northfield-search").contains("enabled: false"));

    feed.send(stale).unwrap();
    harness.run_steps(3);
    assert!(is_shown(&harness, "The job boards Claude searches when you find new jobs. 3 of 6 on."), "the stale read leaves the switch");
    assert!(harness.query_all_by_label("Northfield Jobs, on").next().is_none());

    // A read that begins after the switch is taken as it is.
    let freehire = root.join(".agents/skills/freehire-search/SKILL.md");
    fs::write(&freehire, fs::read_to_string(&freehire).unwrap().replace("enabled: true", "enabled: false")).unwrap();
    feed.send(Snapshot { usage_read: true, ..Snapshot::load(&root, day) }).unwrap();
    harness.run_steps(3);
    assert!(is_shown(&harness, "The job boards Claude searches when you find new jobs. 2 of 6 on."));
}

/// The shell checks for a working run again at the write, so a request that gets past the waiting controls still
/// writes nothing.
#[test]
fn a_change_asked_for_while_a_run_works_writes_nothing() {
    use jobdesk::portals::application::Origin;
    use jobdesk::portals::ui::PortalsRequest;

    let (mut shell, calls) = faked_portals("portals_busy_request");
    *shell.desk_slot() = Some(apply_run_ending([]));
    let (mut harness, shell) = shared_harness(shell);
    pick_portal(&mut harness, "Northfield Jobs");
    shell.borrow_mut().change_portals(PortalsRequest::Switch { folder: "northfield-search".into(), on: false });
    shell.borrow_mut().change_portals(PortalsRequest::Remove { folder: "northfield-search".into(), origin: Origin::MadeByYou });
    harness.run_steps(3);
    assert!(calls.lock().unwrap().is_empty(), "nothing is written while a run works");
    assert!(harness.query_all_by_label(PORTALS_BUSY).count() > 0, "the detail says why");
}

/// While Jobdesk updates the job search folder, a portal is neither switched nor moved to the Trash, and the detail says
/// why in the update's words.
#[test]
fn a_change_asked_for_while_the_folder_updates_writes_nothing() {
    use jobdesk::portals::application::Origin;
    use jobdesk::portals::ui::{FOLDER_UPDATING, PortalsRequest};

    let (shell, calls) = faked_portals("portals_folder_update");
    let (mut harness, shell) = shared_harness(shell.with_folder_update(working_folder_update()));
    pick_portal(&mut harness, "Northfield Jobs");
    shell.borrow_mut().change_portals(PortalsRequest::Switch { folder: "northfield-search".into(), on: false });
    shell.borrow_mut().change_portals(PortalsRequest::Remove { folder: "northfield-search".into(), origin: Origin::MadeByYou });
    harness.run_steps(3);
    assert!(calls.lock().unwrap().is_empty(), "nothing is written while the folder updates");
    assert!(harness.query_all_by_label(FOLDER_UPDATING).count() > 0, "the detail says why");
}

// Browse community and Add a portal.

use jobdesk::portals::application::{CommunityFailure, CommunityList, CountryCode, FolderName, GitSha, ListedPortal, parse_codeowners};

/// The wall clock Browse counts ages to.
fn browse_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(15, 20, 0).unwrap()
}

/// `browse_now` in UTC, as the shell takes it, so every age and time on screen is the same wherever the tests run.
fn browse_utc() -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Local.from_local_datetime(&browse_now()).earliest().unwrap().to_utc()
}

fn minutes_ago(minutes: i64) -> chrono::DateTime<chrono::Utc> {
    browse_utc() - chrono::TimeDelta::minutes(minutes)
}

fn listed_portal(country: &str, folder: &str, name: &str, sentence: &str) -> ListedPortal {
    let sha = GitSha::parse(&"0".repeat(40)).unwrap();
    ListedPortal::from_parts(CountryCode::parse(country).unwrap(), FolderName::parse(folder).unwrap(), sha, name, Some(sentence)).unwrap()
}

/// A fictional community list with made-up owners. Three of its portals are installed in the sample, and the sample's
/// Location is in Belgium.
fn community_list(saved_at: chrono::DateTime<chrono::Utc>) -> CommunityList {
    CommunityList {
        saved_at,
        portals: vec![
            listed_portal("be", "samplefreelance-search", "Sample Freelance Board", "Freelance missions posted by Belgian agencies."),
            listed_portal("global", "freehire-search", "Freehire", "Gathers postings from many job sites into one search."),
            listed_portal("global", "linkedin-search", "LinkedIn", "Public job listings on LinkedIn, in any country."),
            listed_portal("global", "sampleremote-search", "Sample Remote Board", "Remote jobs from small companies, worldwide."),
            listed_portal("dk", "jobindex-search", "Jobindex", "The largest Danish job board, every sector."),
            listed_portal("dk", "jobnet-search", "Jobnet", "Listings from the Danish public job service."),
            listed_portal("dk", "sampletechdk-search", "Sample Tech Jobs DK", "IT and engineering roles in Copenhagen and Aarhus."),
        ],
        codeowners: parse_codeowners(
            "* @board-crew @portal-keeper\n/portals/be/samplefreelance-search/ @portal-keeper\n/portals/global/freehire-search/ @remote-scout\n/portals/global/sampleremote-search/ @remote-scout # unmaintained since 2026-05\n/portals/dk/sampletechdk-search/ @board-crew\n",
        ),
        codeowners_sha: None,
    }
}

/// `shell` on the Browse clock, reading the community list from `saved`, then from `answers` one read at a time.
fn listed(shell: Shell, saved: Option<CommunityList>, answers: Vec<Result<CommunityList, CommunityFailure>>) -> Shell {
    shell.with_now(browse_now()).with_community(jobdesk::portals::scripted_community(saved, answers))
}

/// The sample on Portals with `saved` and `answers`, its tools checked.
fn browse_shell(name: &str, saved: Option<CommunityList>, answers: Vec<Result<CommunityList, CommunityFailure>>) -> Shell {
    listed(probed(shell(Screen::Portals, name), FixedTools), saved, answers)
}

fn open_segment(harness: &mut Harness, label: &str) {
    harness.get_by_role_and_label(egui::accesskit::Role::Tab, label).click();
    harness.run_steps(3);
}

const INSTALL_PROMPT: &str = "Copy https://github.com/ai-job-search-dev/community-portals/tree/main/portals/global/sampleremote-search into `.agents/skills/`. Read its code and tell me what it does. Then ask me before you run `python3 tools/install_portal_clis.py`.";
const NO_LIST: &str = "The community list could not be read";
const READING_LIST: &str = "Reading the list from GitHub…";

/// A portal the community list holds reads Community, with the list's country. Without the list it reads Made by you.
#[test]
fn a_portal_from_the_community_list_reads_community() {
    let mut harness = tray_harness(shell(Screen::Portals, "portals_community_tag_none"));
    pick_portal(&mut harness, "Sample Freelance Board");
    assert!(harness.query_all_by_label("Made by you").count() > 0);
    assert!(harness.query_by_label("Community").is_none());

    let mut harness = tray_harness(browse_shell("portals_community_tag", Some(community_list(minutes_ago(12))), vec![]));
    pick_portal(&mut harness, "Sample Freelance Board, Belgium");
    assert!(harness.query_all_by_label("Community").count() > 0);
    assert!(harness.query_all_by_label("Made by you").count() > 0, "Northfield Jobs is not on the list");
}

/// The list groups by country, the user's market first, then Worldwide, then by name, and says what is installed.
#[test]
fn browse_puts_your_market_first_and_marks_what_is_installed() {
    let mut harness = tray_harness(browse_shell("portals_browse_order", Some(community_list(minutes_ago(12))), vec![]));
    open_segment(&mut harness, "Browse community");
    let top = |label: &str| harness.get_by_label(label).rect().top();
    assert!(top("Belgium") < top("Worldwide") && top("Worldwide") < top("Denmark"));
    assert!(is_shown(&harness, "your market"));
    assert!(is_shown(&harness, "From GitHub, updated 12 min ago"));
    assert_eq!(harness.query_all_by_label("Installed").count(), 5);
    assert_eq!(harness.query_all_by_label("Install…").count(), 2);
    assert!(is_shown(&harness, "Unmaintained since 2026-05"));
    assert!(harness.query_all_by_label("@board-crew, @portal-keeper").count() > 0, "owners are words");
}

/// The search keeps the portals whose name, country or sentence holds it.
#[test]
fn the_search_filters_the_community_list() {
    let mut harness = tray_harness(browse_shell("portals_browse_search", Some(community_list(minutes_ago(12))), vec![]));
    open_segment(&mut harness, "Browse community");
    let field = harness.get_by_role(egui::accesskit::Role::TextInput);
    field.focus();
    field.type_text("remote");
    harness.run_steps(3);
    assert!(is_shown(&harness, "Sample Remote Board"));
    for gone in ["Jobnet", "Sample Freelance Board", "Denmark"] {
        assert!(harness.query_by_label(gone).is_none(), "{gone} is filtered out");
    }
}

/// Install… opens the launcher on the task in plain words, built from the folder alone, and starts nothing.
#[test]
fn install_opens_the_plain_words_task_and_starts_nothing() {
    let calls = SharedCalls::default();
    let mut shell = browse_shell("portals_install", Some(community_list(minutes_ago(12))), vec![]);
    *shell.desk_slot() = Some(desk_on(RecordingGateway(SharedSink::default(), Arc::clone(&calls))));
    let mut harness = tray_harness(shell);
    open_segment(&mut harness, "Browse community");
    harness.get_all_by_label("Install…").next().unwrap().click();
    harness.run_steps(3);
    assert!(is_shown(&harness, WORDS_LAUNCHER), "the launcher opens on a task in plain words");
    assert!(is_shown(&harness, INSTALL_PROMPT));
    assert_eq!(calls.lock().unwrap().starts, 0, "nothing runs until Run");
}

/// View code ↗ opens only on click, and only the address Jobdesk builds from the portal's country and folder, whatever
/// the list's words say.
#[test]
fn view_code_opens_the_address_built_from_the_folder() {
    let mut list = community_list(minutes_ago(12));
    list.portals = vec![listed_portal("be", "samplefreelance-search", "Board https://tracker.example/x", "See https://tracker.example/y for more.")];
    let mut shell = browse_shell("portals_view_code", Some(list), vec![]);
    let opened = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&opened);
    let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(install_look).build_ui(move |ui| {
        shell.show(ui);
        let urls: Vec<String> = ui.ctx().output(|output| {
            output.commands.iter().filter_map(|command| if let egui::OutputCommand::OpenUrl(open) = command { Some(open.url.clone()) } else { None }).collect()
        });
        seen.borrow_mut().extend(urls);
    });
    harness.run_steps(3);
    open_segment(&mut harness, "Browse community");
    harness.get_by_label("View code ↗").hover();
    harness.run_steps(3);
    assert!(opened.borrow().is_empty(), "nothing opens before a click");
    harness.get_by_label("View code ↗").click();
    harness.run_steps(3);
    assert_eq!(*opened.borrow(), ["https://github.com/ai-job-search-dev/community-portals/tree/main/portals/be/samplefreelance-search"]);
}

/// Browse asks for an old list once each time it comes on show, never on every frame. Here GitHub answers with a list
/// already over an hour old, so each showing finds it old.
#[test]
fn browse_reads_the_list_once_each_time_it_shows() {
    let mut harness = tray_harness(browse_shell("portals_browse_once", None, vec![Ok(community_list(minutes_ago(61)))]));
    open_segment(&mut harness, "Browse community");
    harness.run_steps(5);
    assert!(is_shown(&harness, "Sample Remote Board"), "one read, and no other since");
    assert!(!is_shown(&harness, READING_LIST));
    open_segment(&mut harness, "Your portals  6");
    open_segment(&mut harness, "Browse community");
    assert!(is_shown(&harness, READING_LIST), "shown again, it reads again");
}

/// After a failed read, showing Browse again reads nothing for a minute, so a read that keeps failing cannot spend
/// GitHub's limit. Try again still reads at once.
#[test]
fn a_failed_read_is_not_tried_again_by_showing_browse() {
    let answers = vec![Err(CommunityFailure::Unreachable), Ok(community_list(browse_utc()))];
    let mut harness = tray_harness(browse_shell("portals_browse_failed_once", None, answers));
    open_segment(&mut harness, "Browse community");
    assert!(is_shown(&harness, NO_LIST));
    for _ in 0..3 {
        open_segment(&mut harness, "Your portals  6");
        open_segment(&mut harness, "Browse community");
        assert!(is_shown(&harness, NO_LIST), "no read when Browse shows again");
    }
    harness.get_all_by_label("Try again").next().unwrap().click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Sample Remote Board"), "Try again reads at once");
}

/// GitHub's rate limit keeps the saved list, and Try again waits until its reset.
#[test]
fn a_rate_limit_holds_try_again_until_its_reset() {
    let until = browse_utc() + chrono::TimeDelta::minutes(20);
    let answers = vec![Err(CommunityFailure::Limited { until })];
    let mut harness = tray_harness(browse_shell("portals_browse_limited", Some(community_list(minutes_ago(130))), answers));
    open_segment(&mut harness, "Browse community");
    assert!(is_shown(&harness, "GitHub asks Jobdesk to wait before it looks again."));
    assert!(harness.query_by_label_contains("Try again after 15:40.").is_some());
    assert!(harness.get_by_label("Try again").accesskit_node().is_disabled());
    assert!(is_shown(&harness, "Sample Remote Board"), "the saved list stays");

    let past = vec![Err(CommunityFailure::Limited { until: browse_utc() - chrono::TimeDelta::minutes(1) })];
    let mut harness = tray_harness(browse_shell("portals_browse_limit_over", Some(community_list(minutes_ago(130))), past));
    open_segment(&mut harness, "Browse community");
    assert!(!harness.get_by_label("Try again").accesskit_node().is_disabled(), "the reset has passed");
}

/// A 403 or 429 without GitHub's limit still asks Jobdesk to wait, so Try again waits a minute.
#[test]
fn a_bare_refusal_holds_try_again_for_a_minute() {
    let answers = vec![Err(CommunityFailure::Refused(429))];
    let mut harness = tray_harness(browse_shell("portals_browse_refused", Some(community_list(minutes_ago(130))), answers));
    open_segment(&mut harness, "Browse community");
    assert!(harness.get_by_label("Try again").accesskit_node().is_disabled());
    assert!(harness.query_by_label_contains("Try again after 15:21.").is_some());
}

/// With no saved list and none read, the screen says so, with Try again and the list on GitHub.
#[test]
fn no_list_offers_try_again_and_github() {
    let mut harness = tray_harness(browse_shell("portals_browse_none", None, vec![Err(CommunityFailure::TimedOut), Ok(community_list(browse_utc()))]));
    open_segment(&mut harness, "Browse community");
    assert!(is_shown(&harness, NO_LIST));
    assert!(is_shown(&harness, "Open the list on GitHub ↗"));
    harness.get_all_by_label("Try again").next().unwrap().click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "From GitHub, updated just now"));
    assert!(is_shown(&harness, "Sample Remote Board"));
}

/// With no portal installed, Browse community and Add a portal open their views.
#[test]
fn no_portals_yet_leads_to_browse_and_add() {
    let shell = portals_shell_with("portals_none_leads", |root| fs::remove_dir_all(root.join(".agents/skills")).unwrap());
    let mut harness = tray_harness(listed(probed(shell, FixedTools), Some(community_list(minutes_ago(12))), vec![]));
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Browse community").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Sample Remote Board"));
    assert!(harness.query_by_label("Installed").is_none(), "none is installed");
    open_segment(&mut harness, "Your portals  0");
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Add a portal").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "A job board Claude should search"));
}

/// At a large text size the actions stay whole inside the paper: the owners and then the tag give up their width first.
#[test]
fn a_large_text_size_keeps_install_in_reach() {
    let calls = SharedCalls::default();
    let mut shell = browse_shell("portals_browse_large_text", Some(community_list(minutes_ago(12))), vec![]);
    *shell.desk_slot() = Some(desk_on(RecordingGateway(SharedSink::default(), Arc::clone(&calls))));
    let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(install_look).build_ui(move |ui| shell.show(ui));
    harness.ctx.set_zoom_factor(1.5);
    harness.run_steps(3);
    open_segment(&mut harness, "Browse community");
    let edge = harness.get_by_label("Refresh").rect().right();
    let install = harness.get_all_by_label("Install…").next().unwrap().rect();
    assert!(install.right() <= edge + 0.5, "Install… at {install:?} ends past the paper's edge at {edge}");
    for code in harness.get_all_by_label("View code ↗") {
        assert!(code.rect().right() <= edge + 0.5, "View code ↗ at {:?} ends past {edge}", code.rect());
    }
    // Through accesskit, because a pointer click lands where the node would be at the harness's first scale.
    let install = harness.get_all_by_label("Install…").next().unwrap();
    assert!(!install.accesskit_node().is_disabled());
    install.click_accesskit();
    harness.run_steps(3);
    assert!(is_shown(&harness, WORDS_LAUNCHER), "Install… is clickable");
    assert_eq!(calls.lock().unwrap().starts, 0);
}

/// The four fields of Add a portal, in order: website, country, language and test title.
fn type_answers(harness: &mut Harness, answers: [&str; 4]) {
    for (at, answer) in answers.into_iter().enumerate() {
        if answer.is_empty() {
            continue;
        }
        let field = harness.get_all_by_role(egui::accesskit::Role::TextInput).nth(at).unwrap();
        field.focus();
        field.type_text(answer);
        harness.run_steps(2);
    }
    harness.run_steps(3);
}

const FILLED_LINE: &str = "/add-portal https://jobs.sample-board.example country: Belgium, language: French, test title: Lead Backend Engineer";

/// Build it with Claude… waits for a web address, and says why beside it.
#[test]
fn build_it_waits_for_a_web_address() {
    let mut harness = tray_harness(browse_shell("portals_add_waits", None, vec![]));
    open_segment(&mut harness, "Add a portal");
    let build = |harness: &Harness| harness.get_by_label("Build it with Claude…").accesskit_node().is_disabled();
    assert!(build(&harness));
    assert!(is_shown(&harness, "Type the board’s website first."));
    type_answers(&mut harness, ["jobs sample board", "", "", ""]);
    assert!(build(&harness));
    assert!(is_shown(&harness, "Type a web address first."));
    assert!(is_shown(&harness, "This is not a web address. Type it as it shows in your browser, such as https://jobs.example.be."));
}

/// A screen reader hears each field of Add a portal by the words above it, Optional included.
#[test]
fn add_a_portal_names_its_fields() {
    let mut harness = tray_harness(browse_shell("portals_add_names", None, vec![]));
    open_segment(&mut harness, "Add a portal");
    let names: Vec<String> =
        harness.get_all_by_role(egui::accesskit::Role::TextInput).map(|field| field.accesskit_node().label().unwrap_or_default()).collect();
    assert_eq!(names, ["The job board’s website", "Country Optional", "Language of its job ads Optional", "One job title to test with Optional"]);
}

/// A website with a space makes no line, so nothing after the space reaches `/add-portal`, and the form says why.
#[test]
fn build_it_refuses_a_website_with_a_space() {
    let mut harness = tray_harness(browse_shell("portals_add_space", None, vec![]));
    open_segment(&mut harness, "Add a portal");
    type_answers(&mut harness, ["jobs.example.be/x --list", "", "", ""]);
    assert!(harness.get_by_label("Build it with Claude…").accesskit_node().is_disabled());
    assert!(is_shown(&harness, "Take the spaces out of the address first."));
    assert!(is_shown(&harness, "A web address has no spaces. Type it as it shows in your browser, such as https://jobs.example.be/jobs."));
}

/// A field holding a character that shows nothing makes no line, and says which field.
#[test]
fn build_it_refuses_hidden_characters() {
    let mut harness = tray_harness(browse_shell("portals_add_hidden", None, vec![]));
    open_segment(&mut harness, "Add a portal");
    type_answers(&mut harness, ["jobs.sample-board.example", "Bel\u{200B}gium", "", ""]);
    assert!(harness.get_by_label("Build it with Claude…").accesskit_node().is_disabled());
    assert!(harness.query_all_by_label("The country holds characters that do not show. Type it again by hand.").count() > 0);
}

/// Build it with Claude… opens the launcher on `/add-portal` with the answers on its line, and starts nothing.
#[test]
fn build_it_opens_add_portal_with_the_answers() {
    let calls = SharedCalls::default();
    let mut shell = browse_shell("portals_add_build", None, vec![]);
    *shell.desk_slot() = Some(desk_on(RecordingGateway(SharedSink::default(), Arc::clone(&calls))));
    let mut harness = tray_harness(shell);
    open_segment(&mut harness, "Add a portal");
    type_answers(&mut harness, ["jobs.sample-board.example", "Belgium", "French", "Lead Backend Engineer"]);
    assert!(is_shown(&harness, FILLED_LINE), "the command shows");
    harness.get_by_label("Build it with Claude…").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, LAUNCHER), "the launcher opens");
    assert!(harness.query_all_by_label(FILLED_LINE).count() > 0, "on the line the task runs");
    assert_eq!(calls.lock().unwrap().starts, 0, "nothing runs until Run");
}

fn render_browse(name: &str, saved: Option<CommunityList>, answers: Vec<Result<CommunityList, CommunityFailure>>, theme: Theme) -> Harness<'static> {
    let mut harness = probed_wgpu(browse_shell(name, saved, answers), theme);
    open_segment(&mut harness, "Browse community");
    harness
}

#[test]
fn portals_browse_light() {
    let name = "portals_browse_light";
    compare(&mut render_browse(name, Some(community_list(minutes_ago(12))), vec![], Theme::Light), name);
}

#[test]
fn portals_browse_dark() {
    let name = "portals_browse_dark";
    compare(&mut render_browse(name, Some(community_list(minutes_ago(12))), vec![], Theme::Dark), name);
}

/// At 150% text size the actions stay whole, with the room the owners and the tag do not need given to the name. The
/// search frames the rows that show Install… and an Unmaintained tag.
#[test]
fn portals_browse_large_light() {
    let name = "portals_browse_large_light";
    let mut harness = probed_wgpu(browse_shell(name, Some(community_list(minutes_ago(12))), vec![]), Theme::Light);
    harness.ctx.set_zoom_factor(1.5);
    harness.run_steps(3);
    harness.get_by_role_and_label(egui::accesskit::Role::Tab, "Browse community").click_accesskit();
    harness.run_steps(3);
    let field = harness.get_by_role(egui::accesskit::Role::TextInput);
    field.focus();
    field.type_text("remote");
    harness.run_steps(3);
    assert!(is_shown(&harness, "Unmaintained since 2026-05"));
    assert_eq!(harness.query_all_by_label("Install…").count(), 1);
    compare(&mut harness, name);
}

#[test]
fn portals_browse_loading_light() {
    let name = "portals_browse_loading_light";
    compare(&mut render_browse(name, None, vec![], Theme::Light), name);
}

#[test]
fn portals_browse_offline_light() {
    let name = "portals_browse_offline_light";
    compare(&mut render_browse(name, Some(community_list(minutes_ago(25 * 60 + 18))), vec![Err(CommunityFailure::Unreachable)], Theme::Light), name);
}

#[test]
fn portals_browse_limited_light() {
    let name = "portals_browse_limited_light";
    let answers = vec![Err(CommunityFailure::Limited { until: browse_utc() + chrono::TimeDelta::minutes(20) })];
    let mut harness = render_browse(name, Some(community_list(minutes_ago(12))), answers, Theme::Light);
    harness.get_by_label("Refresh").click();
    harness.run_steps(3);
    compare(&mut harness, name);
}

#[test]
fn portals_browse_nocache_light() {
    let name = "portals_browse_nocache_light";
    compare(&mut render_browse(name, None, vec![Err(CommunityFailure::Unreachable)], Theme::Light), name);
}

fn render_add(name: &str, answers: [&str; 4]) {
    let mut harness = probed_wgpu(browse_shell(name, None, vec![]), Theme::Light);
    open_segment(&mut harness, "Add a portal");
    type_answers(&mut harness, answers);
    compare(&mut harness, name);
}

#[test]
fn portals_add_empty_light() {
    render_add("portals_add_empty_light", ["", "", "", ""]);
}

#[test]
fn portals_add_filled_light() {
    render_add("portals_add_filled_light", ["https://jobs.sample-board.example", "Belgium", "French", "Lead Backend Engineer"]);
}

#[test]
fn portals_add_partial_light() {
    render_add("portals_add_partial_light", ["https://jobs.sample-board.example", "Belgium", "", ""]);
}

#[test]
fn portals_add_invalid_light() {
    render_add("portals_add_invalid_light", ["jobs sample board", "Belgium", "French", "Lead Backend Engineer"]);
}
