//! Renders every screen in both themes from a sample workspace.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use egui::Theme;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use jobdesk::runs::application::{
    AgentEvent, AgentGateway, AgentSession, Decision, EventSink, Prompt, PromptKind, Question, QuestionOption, RunDesk, RunRequest, StartOptions, Usage,
};
use jobdesk::shell::{Screen, Shell, Snapshot, install_look};

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
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("ai-job-search");
    let _ = fs::remove_dir_all(&root);
    sample_workspace(&root);
    let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
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

fn tool(id: &str, name: &str, summary: &str, nested: bool) -> AgentEvent {
    AgentEvent::ToolStarted { id: id.into(), name: name.into(), summary: summary.into(), nested }
}

fn done(id: &str) -> AgentEvent {
    AgentEvent::ToolFinished { id: id.into(), failed: false }
}

fn apply_run(final_event: AgentEvent) -> RunDesk {
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    desk.start("/apply https://kestrel.example/jobs/2", &StartOptions { model: None, budget_usd: 5.0 }).unwrap();
    let events = [
        AgentEvent::SessionStarted { session_id: "a5ce0bcc-22f9".into(), model: "claude-opus-5-5".into() },
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
        tool("7", "Bash", "cd cv && lualatex -interaction=nonstopmode main_kestrel_solution_architect.tex", false),
        final_event,
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

fn render(screen: Screen, theme: Theme, name: &str) {
    render_with(shell(screen, name), theme, name);
}

fn render_run(final_event: AgentEvent, theme: Theme, name: &str) {
    let mut shell = shell(Screen::Runs, name);
    *shell.desk_slot() = Some(apply_run(final_event));
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
        .with_context_setup(|ctx| install_look(ctx))
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

/// Overview button, launcher form, run, then the Runs screen.
#[test]
fn a_command_button_leads_to_a_started_run() {
    let mut shell = shell(Screen::Overview, "click_through");
    let gateway = ScriptedGateway(SharedSink::default());
    *shell.desk_slot() = Some(RunDesk::new(Box::new(gateway), Arc::new(|| {})));
    let mut harness = Harness::builder()
        .with_size([1280.0, 820.0])
        .with_context_setup(|ctx| install_look(ctx))
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(2);

    harness.get_by_label("/setup").click();
    harness.run_steps(3);
    harness.get_by_label("Run a command");

    harness.get_by_label("Run").click();
    harness.run_steps(3);

    harness.get_by_label("Every command Jobdesk ran in this workspace");
    assert_eq!(harness.get_all_by_label("Starting").count(), 2, "run list and run header");
}

const FINDINGS: &str = "## Verification Checklist\n- [x] All claims match the profile\n- [x] CV is exactly 2 pages\n- [ ] Addressed to a named person\n\n| Keyword | Priority | Status | Note |\n|---|---|---|---|\n| event sourcing | required | covered | Skills |\n| Azure | preferred | missing (gap) | Named in letter |\n";
const EVALUATION: &str = "**Overall fit: 78, Strong Fit.**\n\n**Skills match:** Java, Kafka and event sourcing match.\n\n**Experience match:** Public sector work maps well.";

fn dossier_shell(name: &str) -> Shell {
    let mut shell = shell(Screen::Tracker, name);
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    desk.start("/apply https://tessel.example/1", &StartOptions { model: None, budget_usd: 5.0 }).unwrap();
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
        .with_context_setup(|ctx| install_look(ctx))
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
    render_dossier(Some("Outcome and follow-ups"), Theme::Dark, "dossier_outcome_dark");
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
fn launcher_light() {
    render_launcher(Theme::Light, "launcher_light");
}

#[test]
fn launcher_dark() {
    render_launcher(Theme::Dark, "launcher_dark");
}

screens! {
    overview_dark: Screen::Overview, Theme::Dark;
    overview_light: Screen::Overview, Theme::Light;
    jobs_dark: Screen::Jobs, Theme::Dark;
    jobs_light: Screen::Jobs, Theme::Light;
    tracker_dark: Screen::Tracker, Theme::Dark;
    tracker_light: Screen::Tracker, Theme::Light;
    insights_dark: Screen::Insights, Theme::Dark;
    insights_light: Screen::Insights, Theme::Light;
}
