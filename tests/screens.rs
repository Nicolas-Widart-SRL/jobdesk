//! Renders every screen in both themes from a sample workspace.

use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use egui::{CursorIcon, Key, Theme};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, SnapshotOptions};
use jobdesk::runs::application::{
    AgentEvent, AgentGateway, AgentSession, Decision, EventSink, ModelCost, Prompt, PromptKind, Question, QuestionOption, RunDesk, RunRequest, StartOptions,
    TaskCost, TaskOutcome, Usage, UsageReport,
};
use jobdesk::settings::application::{Page, drawn_index, index};
use jobdesk::shell::{Screen, Shell, Snapshot, Source, install_look};
use jobdesk::ui_kit::tokens::{BODY_PADDING, HAIRLINE, HEADER_PADDING, HEAT_LABELS, SCREEN_PADDING, STATUS_DOT, STATUS_PADDING, TALLY_GAP_Y};
use jobdesk::workspace::application::{EnvironmentChecks, ToolProber, ToolSpec};

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
    emit_all(
        &sink,
        [
            tool("6", "Read", ".claude/skills/job-application-assistant/05-cv-tailoring.md", false),
            done("6"),
            tool("7", "Write", "cv/main_kestrel_solution_architect.tex", false),
            done("7"),
            tool("8", "Write", "cover_letters/cover_kestrel_solution_architect.tex", false),
        ],
    );
    (desk, sink)
}

const SUMMARY: &str = "# Application ready: Kestrel Payments, Solution Architect\n\nThe CV (2 pages) and the cover letter (1 page) are built, and both checks passed.\n\n## What changed after review\n\n- 6 reviewer edits applied. 1 was skipped because your profile does not back it.\n- The letter now opens with the payments migration.\n\n## Next\n\n- Read both PDFs in the dossier.\n- Send it yourself. Jobdesk never sends applications.";

/// /apply through Finish, its closing summary, and the process gone.
fn apply_finished() -> RunDesk {
    let (desk, sink) = apply_drafting();
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
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
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
    *shell.desk_slot() = Some(RunDesk::new(Box::new(RecordingGateway(SharedSink::default(), Arc::clone(calls))), Arc::new(|| {})));
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
    for screen in [Screen::InTray, Screen::Jobs, Screen::Tracker, Screen::Insights, Screen::Runs, Screen::Workspace] {
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
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(SharedSink::default())), Arc::new(|| {})));
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
    harness.get_by_label(LAUNCHER);
    harness.get_by_label("Tasks, 1 need you").click();
    harness.run_steps(3);
    assert_eq!(harness.get_by_role(egui::accesskit::Role::TextInput).value().as_deref(), Some("keep this"));
}

#[test]
fn the_tabs_use_plain_names() {
    let mut harness = tray_harness(probed(shell(Screen::InTray, "plain_tabs"), FixedTools));
    settle(&mut harness);
    for name in ["Today", "Jobs", "Applications", "Insights", "Tasks", "Setup"] {
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
    assert!(is_shown(&harness, "4 settings match. Esc clears the search."));
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
fn unbuilt_sections_are_disabled() {
    let mut shell = shell(Screen::InTray, "settings_unbuilt");
    shell.open_settings(Page::General);
    let harness = tray_harness(shell);
    for page in Page::ALL {
        let row = harness.query_all_by_label(page.title()).next().unwrap_or_else(|| panic!("{} is listed", page.title()));
        assert_eq!(row.accesskit_node().is_disabled(), !page.built(), "{}", page.title());
    }
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
    for line in drawn_index(Page::Runtime, true) {
        assert!(harness.query_all_by_label(line).next().is_some(), "{line}");
    }
    assert_eq!(drawn_index(Page::Runtime, true), index(Page::Runtime), "with the folder read, every line is drawn");

    // With no folder, the lines a search can match are the ones drawn, and nothing more.
    let mut shell = probed(Shell::without_workspace(NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()), FixedTools);
    shell.open_settings(Page::Runtime);
    let mut harness = tray_harness(shell);
    settle(&mut harness);
    for &line in index(Page::Runtime) {
        let drawn = harness.query_all_by_label(line).next().is_some();
        assert_eq!(drawn, drawn_index(Page::Runtime, false).contains(&line), "{line}");
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

/// The welcome's second step names a place that stays once the folder is chosen and every tool answers,
/// when the Setup tab has left the menu.
#[test]
fn the_welcome_points_at_a_place_that_stays() {
    let harness = tray_harness(probed(Shell::without_workspace(NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()), AllTools));
    let step = harness.get_by_label_contains("shows which parts are filled in and what is missing.");
    let words = step.accesskit_node().value().unwrap_or_default();
    assert!(words.starts_with("Settings › Runtime status"), "{words}");
    assert!(!words.contains("Setup"), "{words}");

    let mut harness = tray_harness(probed(shell(Screen::InTray, "welcome_step_reachable"), AllTools));
    settle(&mut harness);
    assert!(harness.query_by_role_and_label(egui::accesskit::Role::Tab, "Setup").is_none(), "the Setup tab has gone");
    settings_button(&harness).click();
    harness.run_steps(3);
    harness.get_by_label("Runtime status").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Whether Claude Code, your profile, tools, portals, templates and sync are ready."));
    assert!(harness.query_all_by_label("Profile").next().is_some(), "and the profile shows there");
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
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&started))), Arc::new(|| {})));
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
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(SharedSink::default())), Arc::new(|| {})));
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
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&started))), Arc::new(|| {})));
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
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&started))), Arc::new(|| {})));
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
    *shell.desk_slot() = Some(RunDesk::new(Box::new(gateway), Arc::new(|| {})));
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
        *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&started))), Arc::new(|| {})));
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
        *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&started))), Arc::new(|| {})));
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
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(SharedSink::default())), Arc::new(|| {})));
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
            Source::Decisions => unreachable!("the tray reports its own file"),
        }
    }
    let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    assert_eq!(snapshot.issues.len(), broken.len(), "{:?}", snapshot.issues);
    Shell::with_snapshot(screen, jobdesk::workspace::open(&root).unwrap(), snapshot)
}

const TRACKER_HIDDEN: &str = "Hidden while the tracker cannot be read.";
const JOBS_HIDDEN: &str = "Hidden while your saved jobs cannot be read.";

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
            shown: &["Jobs seen"],
            not_shown: &["Jobs seen are hidden while your saved jobs cannot be read."],
        },
        Case {
            screen: Screen::Workspace,
            broken: &[Source::Jobs],
            subtitle: "ai-job-search",
            alert: "Your saved jobs could not be read.",
            shown: &["Jobs seen are hidden while your saved jobs cannot be read."],
            not_shown: &["Jobs seen"],
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
    assert!(is_shown(&harness, "You will see which account Claude uses after the first task. Jobdesk stores no credentials."), "Runtime status is on show");
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
fn render_probed(shell: Shell, theme: Theme, name: &str) {
    probed_wgpu(probed(shell, FixedTools), theme).snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
}

/// A rendering harness on `shell`, stepped until its tool check is back.
fn probed_wgpu(mut shell: Shell, theme: Theme) -> Harness<'static> {
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_pixels_per_point(1.5)
        .with_theme(theme)
        .with_context_setup(install_look)
        .wgpu()
        .build_ui(move |ui| shell.show(ui));
    settle(&mut harness);
    harness
}

/// Settings over Today, on `page`, with bun missing so Runtime status says "1 missing". `query` is typed into the search.
fn render_settings(page: Page, query: Option<&str>, theme: Theme, name: &str) {
    let mut shell = shell(Screen::InTray, name).with_folder_label("~/Documents/ai-job-search");
    shell.open_settings(page);
    let mut harness = probed_wgpu(probed(shell, FixedTools), theme);
    if let Some(query) = query {
        search(&harness).focus();
        search(&harness).type_text(query);
        harness.run_steps(3);
    }
    harness.snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
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
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    Shell::with_snapshot(Screen::InTray, jobdesk::workspace::open(&root).unwrap(), usage_snapshot(&root, tasks)).with_folder_label("~/Documents/ai-job-search")
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
    harness.snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
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
    let shell = loading_on(Screen::InTray, "settings_loading_light").with_folder_label("~/Documents/ai-job-search");
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
    *launcher.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(SharedSink::default())), Arc::new(|| {})));
    launcher.open_settings(Page::Usage);
    let (mut harness, shell) = shared_harness(launcher);
    harness.get_by_label("New task…").click();
    harness.run_steps(3);
    assert!(!shell.borrow().settings_open());
    assert!(is_shown(&harness, LAUNCHER), "the launcher is open");
}

/// Every required tool answers, so the menu has no Setup tab.
#[test]
fn menu_without_setup_light() {
    let mut harness = probed_wgpu(probed(shell(Screen::InTray, "menu_without_setup_light"), AllTools), Theme::Light);
    harness.snapshot_options("menu_without_setup_light", &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
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
    let empty =
        ["No jobs wait here.", "No jobs wait here", "No applications yet", "No applications sent yet.", "No skill gap report yet", "No portals installed."];
    let cases: [(Screen, &str, &[&str]); 4] = [
        (Screen::Jobs, "Reading your saved jobs…", &["Find new jobs…", "Score", "Worth knowing"]),
        (Screen::Tracker, "Reading your applications…", &["Record a result…", "Check mail for replies…", "Not sent yet", "Applied", "Closed"]),
        (
            Screen::Insights,
            "Reading your jobs, applications and reports…",
            &["Find skill gaps…", "Rebuild dashboard…", "Profile", "Result", "Funnel", "By sector"],
        ),
        (Screen::Workspace, "Reading your profile, tools and portals…", &["Change folder", "Profile", "Environment", "Portals"]),
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
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(SharedSink::default())), Arc::new(|| {})));
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

/// The run logs sit outside the checkout, so the shell asks for a reload when a run stops working.
fn reload_harness(name: &str) -> (Harness<'static>, SharedSink, std::sync::mpsc::Receiver<()>) {
    let mut shell = shell(Screen::Runs, name);
    let reloads = shell.reload_feed();
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
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
