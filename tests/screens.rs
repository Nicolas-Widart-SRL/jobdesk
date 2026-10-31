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

fn tool(id: &str, name: &str, summary: &str, nested: bool) -> AgentEvent {
    AgentEvent::ToolStarted { id: id.into(), name: name.into(), summary: summary.into(), nested }
}

fn done(id: &str) -> AgentEvent {
    AgentEvent::ToolFinished { id: id.into(), failed: false }
}

/// Start times show on screen, so scripted runs start at a fixed moment.
fn run_started() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(9, 30, 0).unwrap()
}

fn apply_run(final_event: AgentEvent) -> RunDesk {
    let sink = SharedSink::default();
    let mut desk = RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&sink))), Arc::new(|| {}));
    // Permission prompts, as the coming setting will show them.
    desk.ask_permission();
    desk.start("/apply https://kestrel.example/jobs/2", &StartOptions { model: None, budget_usd: 5.0 }, run_started()).unwrap();
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

    harness.get_by_label("Calibrate").click();
    harness.run_steps(3);
    harness.get_by_label("All commands");

    harness.get_by_label("Run").click();
    harness.run_steps(3);

    harness.get_by_label("Recent runs");
    assert_eq!(harness.get_all_by_label("Starting").count(), 2, "run head and recent runs");
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
fn smallest_window(final_event: AgentEvent, name: &str) -> Harness<'static> {
    let mut shell = shell(Screen::Runs, name);
    *shell.desk_slot() = Some(apply_run(final_event));
    let mut harness = Harness::builder()
        .with_size([1040.0, 640.0])
        .with_context_setup(install_look)
        .build_ui(move |ui| shell.show(ui));
    harness.run_steps(3);
    harness
}

#[test]
fn a_permission_prompt_fits_the_smallest_window() {
    let harness = smallest_window(permission_prompt(), "smallest_permission");
    assert_in_window(&harness, [1040.0, 640.0], &["Allow once", "Allow for this run", "Deny"]);
}

#[test]
fn a_question_with_many_choices_fits_the_smallest_window() {
    let harness = smallest_window(many_choices("q"), "smallest_question");
    assert_in_window(&harness, [1040.0, 640.0], &["Send answer", "Decline"]);
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
    *shell.desk_slot() = Some(apply_run(stop_point()));
    let mut harness = tray_harness(shell);
    let field = harness.get_by_role(egui::accesskit::Role::TextInput);
    field.focus();
    field.type_text("keep this");
    harness.run_steps(2);
    harness.key_press(Key::Home);
    harness.key_press_modifiers(egui::Modifiers { ctrl: true, command: true, ..Default::default() }, Key::K);
    harness.run_steps(3);
    harness.get_by_label("All commands");
    harness.get_by_label("Runs").click();
    harness.run_steps(3);
    assert_eq!(harness.get_by_role(egui::accesskit::Role::TextInput).value().as_deref(), Some("keep this"));
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

fn press(harness: &mut Harness, key: Key) {
    harness.key_press(key);
    harness.run_steps(3);
}

fn is_shown(harness: &Harness, label: &str) -> bool {
    harness.query_by_label(label).is_some()
}

const TRAY_CARDS: usize = 10;
const HARBOR: &str = "Harbor Bank, Lead Java Engineer";
const MERIDIAN: &str = "Meridian Rail Systems, Tech Lead Java, Real-Time Traffic";

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
    let row = "Last decision: Set aside, Harbor Bank. Press U to undo.";
    assert!(is_shown(&harness, row));
    assert!(is_shown(&harness, "Set aside until tomorrow"));
    assert!(!is_shown(&harness, "Nothing yet."), "the undo row heads the rail");

    press(&mut harness, Key::U);
    assert!(is_shown(&harness, HARBOR));
    assert!(!is_shown(&harness, row));
    assert!(is_shown(&harness, "Nothing yet."), "setting aside clears nothing");
}

#[test]
fn a_timeline_mark_pulls_a_set_aside_card_back() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_mark"));
    press(&mut harness, Key::L);
    assert!(!is_shown(&harness, HARBOR));

    harness.get_by_label("Harbor deadline").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, HARBOR));
}

#[test]
fn bring_them_back_refills_the_tray() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_bring_back"));
    for _ in 0..TRAY_CARDS {
        press(&mut harness, Key::L);
    }
    assert!(is_shown(&harness, &format!("{TRAY_CARDS} set aside")));
    assert!(!is_shown(&harness, "Nothing needs you"));

    harness.get_by_label("Bring them back").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, HARBOR));
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
    assert!(is_shown(&harness, "Skipped, domain"));
    assert!(is_shown(&harness, "Last decision: Skipped, Meridian Rail Systems. Press U to undo."));
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

    harness.get_by_label("Open run").click();
    harness.run_steps(3);
    harness.get_by_label("Recent runs");
}

#[test]
fn a_guard_card_has_no_default_and_a_kept_posting_reaches_triage() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_guard"));
    for _ in 0..4 {
        press(&mut harness, Key::L);
    }
    let guarded = "Halden Logistics, Lead Developer Kafka";
    assert!(is_shown(&harness, guarded));
    press(&mut harness, Key::Y);
    assert!(is_shown(&harness, guarded));

    harness.get_by_label("Harmless, keep posting").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Last decision: Kept, Halden Logistics. Press U to undo."));
}

#[test]
fn the_horizon_counts_what_is_left_today() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_count"));
    assert!(is_shown(&harness, &format!("{TRAY_CARDS} to decide")));

    press(&mut harness, Key::L);
    assert!(is_shown(&harness, &format!("{} to decide", TRAY_CARDS - 1)));
}

#[test]
fn the_return_chip_brings_set_aside_cards_back_without_a_run() {
    let mut shell = shell(Screen::InTray, "tray_return_chip");
    let started = SharedSink::default();
    *shell.desk_slot() = Some(RunDesk::new(Box::new(ScriptedGateway(Arc::clone(&started))), Arc::new(|| {})));
    let mut harness = tray_harness(shell);
    press(&mut harness, Key::L);

    harness.get_by_label_contains("1 set aside, back tomorrow: Harbor Bank").click();
    harness.run_steps(3);

    assert!(is_shown(&harness, HARBOR), "the card brought back is on top again");
    assert!(is_shown(&harness, &format!("{TRAY_CARDS} to decide")));
    assert!(harness.query_by_label_contains("set aside, back tomorrow").is_none());
    assert!(!is_shown(&harness, "All commands"), "the chip opens no launcher");
    assert!(started.lock().unwrap().is_none(), "the chip starts no run");
}

/// The top of the dealt card's tab, which moves only when the tray scrolls.
fn tab_top(harness: &Harness) -> f32 {
    let kinds = ["Permission", "Question", "Send", "Triage", "Prep", "Offer", "Guard flag", "Follow-up", "Close"];
    kinds.iter().find_map(|kind| harness.query_by_label(kind)).expect("a dealt card").rect().top()
}

#[test]
fn later_and_keep_leave_the_scroll_alone() {
    let mut harness = tray_harness(shell(Screen::InTray, "tray_no_scroll"));
    let top = tab_top(&harness);
    for _ in 0..4 {
        press(&mut harness, Key::L);
        assert_eq!(tab_top(&harness), top);
    }
    harness.get_by_label("Harmless, keep posting").click();
    harness.run_steps(3);
    assert!(is_shown(&harness, "Last decision: Kept, Halden Logistics. Press U to undo."));
    assert_eq!(tab_top(&harness), top);
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
    assert!(is_shown(&harness, "Last decision: Skipped, Federal Data Agency. Press U to undo."));
    assert!(is_shown(&harness, "Skipped, rate"), "Y skips with the suggested reason");
    assert_eq!(tab_top(&harness), resting);
}

#[test]
fn overdue_cards_show_in_todays_cell() {
    let harness = tray_harness(shell_on(Screen::InTray, "tray_overdue", overdue_day()));
    assert!(harness.query_by_label_contains(" overdue").is_some());
    assert!(is_shown(&harness, "No other deadlines in the next 14 days"));
    assert!(!is_shown(&harness, "Harbor deadline"));
}

#[test]
fn an_empty_tray_says_nothing_is_due() {
    let harness = tray_harness(empty_shell("tray_empty"));
    assert!(is_shown(&harness, "0 to decide"));
    assert!(is_shown(&harness, "No deadlines in the next 14 days"));
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
    let mut shell = shell(Screen::Workspace, name);
    *shell.environment_slot() = Some(EnvironmentChecks::new(Arc::new(FixedTools), Arc::new(|| {})));
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
    intray_dark: Screen::InTray, Theme::Dark;
    intray_light: Screen::InTray, Theme::Light;
    jobs_dark: Screen::Jobs, Theme::Dark;
    jobs_light: Screen::Jobs, Theme::Light;
    tracker_dark: Screen::Tracker, Theme::Dark;
    tracker_light: Screen::Tracker, Theme::Light;
    insights_dark: Screen::Insights, Theme::Dark;
    insights_light: Screen::Insights, Theme::Light;
}
