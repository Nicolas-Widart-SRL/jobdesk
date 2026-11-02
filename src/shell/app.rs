use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use chrono::{Local, NaiveDate};
use egui::{Align, Frame, Label, Layout, Margin, Panel, Rect, Ui, UiBuilder, pos2, vec2};

use crate::discovery::ui::JobsScreen;
use crate::dossier;
use crate::dossier::application::{Dossier, PageStore, RunTranscript, compose_dossier};
use crate::dossier::ui::DossierScreen;
use crate::insights;
use crate::intray;
use crate::intray::application::{InTray, Queue, Request, Sources};
use crate::intray::ui::{TodayContext, TodayScreen};
use crate::runs;
use crate::runs::application::RunDesk;
use crate::runs::ui::{Launcher, RunScreen, RunScreenRequest};
use crate::settings;
use crate::settings::application::{Page, RuntimeWord, Settings, StartScreen};
use crate::settings::ui::{SettingsRequest, SettingsScreen, SettingsView};
use crate::shared_kernel::LoadError;
use crate::shell::{Snapshot, Source, live_snapshot};
use crate::tracking::ui::TrackerScreen;
use crate::ui_kit::tokens::{
    HEAD_GAP, KNOT_GAP, KNOT_SIZE, NAV_TAB_GAP, NAV_TAB_HEIGHT, SCREEN_PADDING, SHELL_GAP, SHELL_PADDING, STATUS_GAP, WELCOME_PADDING, WELCOME_STEP_PADDING, WELCOME_TOP, WELCOME_WIDTH, WINDOW_SHADOW,
};
use crate::ui_kit::{
    Ink, KnotState, Load, ScreenState, TabMark, Tone, Type, alert, back_pressed, brand, button, command_chord, copyable, desk, divider, enabled_when, folder_tab, hidden_note, install_fonts, install_theme, knot, muted, note_dialog, note_focus, page_body, paper,
    primary_button, prompt_box_tone, rich,
    sheet_frame, shell_top_frame, shortcut_button, status_button, status_frame, status_line, status_note, status_waits, strong, take_command_request,
};
use crate::workspace;
use crate::workspace::application::{AuthMode, EnvironmentChecks, Readiness, WorkspaceRoot, auth_mode, readiness};
use crate::workspace::ui::WorkspaceAction;

const WORKSPACE_KEY: &str = "workspace";

/// What the tabs and New task say while they wait for a folder.
const CHOOSE_FOLDER: &str = "Choose your job search folder first.";
/// The keys that open the launcher.
const ASK_KEYS: [&str; 2] = command_chord("K");
/// The keys that open Settings.
const SETTINGS_KEYS: [&str; 2] = command_chord(",");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    InTray,
    Jobs,
    Tracker,
    Insights,
    Runs,
    Workspace,
    Launcher,
    Dossier,
}

impl Screen {
    const NAV: [Self; 6] = [Self::InTray, Self::Jobs, Self::Tracker, Self::Insights, Self::Runs, Self::Workspace];

    fn title(self) -> &'static str {
        match self {
            Self::InTray => "Today",
            Self::Jobs => "Jobs",
            Self::Tracker => "Applications",
            Self::Insights => "Insights",
            Self::Runs => "Tasks",
            Self::Workspace => "Setup",
            Self::Launcher => "New task",
            Self::Dossier => "Application dossier",
        }
    }

    /// The folder tab a screen lights: a dossier sits in the tracker, the launcher with the runs.
    fn tab(self) -> Self {
        match self {
            Self::Dossier => Self::Tracker,
            Self::Launcher => Self::Runs,
            screen => screen,
        }
    }
}

/// Window state. It only reads snapshots.
pub struct Shell {
    screen: Screen,
    workspace: Option<WorkspaceRoot>,
    snapshot: Snapshot,
    /// False from opening a folder until its first snapshot arrives. Today shows that it is reading.
    loaded: bool,
    updates: Option<Receiver<Snapshot>>,
    jobs: JobsScreen,
    tracker: TrackerScreen,
    folder_error: Option<String>,
    desk: Option<RunDesk>,
    launcher: Launcher,
    run_screen: RunScreen,
    data: Option<PathBuf>,
    tray: Option<InTray>,
    today: TodayScreen,
    confirm_quit: bool,
    /// Keep working takes the focus once, on the frame the quit dialog opens.
    quit_focus: bool,
    /// Where Esc leaves the launcher for: the screen it was opened from.
    launcher_from: Screen,
    dossier_screen: DossierScreen,
    pages: Option<PageStore>,
    open_dossier: Option<OpenDossier>,
    environment: Option<EnvironmentChecks>,
    settings: Settings,
    settings_screen: SettingsScreen,
    /// Settings covers the menu and the screen, which stays as it was underneath.
    settings_open: bool,
    /// The folder as Settings shows it, in place of its path. Snapshots set it, so no temporary path shows.
    folder_label: Option<String>,
}

/// The dossier on screen. It is rebuilt when its files or saved runs change.
struct OpenDossier {
    folder: String,
    view: Option<Dossier>,
    run_activity: usize,
}

impl Shell {
    fn empty(screen: Screen, today: NaiveDate) -> Self {
        Self {
            screen,
            workspace: None,
            snapshot: Snapshot { today, ..Snapshot::default() },
            loaded: false,
            updates: None,
            jobs: JobsScreen::default(),
            tracker: TrackerScreen::default(),
            folder_error: None,
            desk: None,
            launcher: Launcher::default(),
            run_screen: RunScreen::default(),
            data: None,
            tray: None,
            today: TodayScreen::default(),
            confirm_quit: false,
            quit_focus: false,
            launcher_from: screen.tab(),
            dossier_screen: DossierScreen::default(),
            pages: None,
            open_dossier: None,
            environment: None,
            settings: Settings::unsaved(),
            settings_screen: SettingsScreen::default(),
            settings_open: false,
            folder_label: None,
        }
    }

    /// A shell with fixed data, for tests and previews.
    pub fn with_snapshot(screen: Screen, workspace: WorkspaceRoot, snapshot: Snapshot) -> Self {
        let today = snapshot.today;
        Self { workspace: Some(workspace), snapshot, loaded: true, tray: Some(InTray::unsaved()), ..Self::empty(screen, today) }
    }

    /// A shell whose folder is open but not read yet, for tests and previews.
    pub fn loading(screen: Screen, workspace: WorkspaceRoot, today: NaiveDate) -> Self {
        Self { workspace: Some(workspace), tray: Some(InTray::unsaved()), ..Self::empty(screen, today) }
    }

    /// The first run: no folder is open yet.
    pub fn without_workspace(today: NaiveDate) -> Self {
        Self::empty(Screen::InTray, today)
    }

    pub fn desk_slot(&mut self) -> &mut Option<RunDesk> {
        &mut self.desk
    }

    pub fn tray_slot(&mut self) -> &mut Option<InTray> {
        &mut self.tray
    }

    pub fn environment_slot(&mut self) -> &mut Option<EnvironmentChecks> {
        &mut self.environment
    }

    /// Opens Settings on `page`, as Cmd , does once that page was last open.
    pub fn open_settings(&mut self, page: Page) {
        self.settings_screen.open_on(page);
        self.settings_open = true;
    }

    pub fn settings_open(&self) -> bool {
        self.settings_open
    }

    /// Shows `label` as the folder in Settings, for snapshots.
    pub fn with_folder_label(self, label: &str) -> Self {
        Self { folder_label: Some(label.to_owned()), ..self }
    }

    /// Hands the shell snapshots as the folder watcher does, for tests.
    pub fn snapshot_feed(&mut self) -> std::sync::mpsc::Sender<Snapshot> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.updates = Some(receiver);
        sender
    }

    pub fn open_dossier(&mut self, folder: &str, pages: PageStore) {
        self.pages = Some(pages);
        self.show_dossier(folder.to_owned());
    }

    fn show_dossier(&mut self, folder: String) {
        self.open_dossier = Some(OpenDossier { folder, view: None, run_activity: 0 });
        self.dossier_screen.reset();
        self.screen = Screen::Dossier;
    }

    fn refreshed_dossier(&mut self) -> Option<Dossier> {
        let runs = self.desk.as_ref().map_or(&[][..], RunDesk::runs);
        let run_activity = runs.iter().map(|run| run.activity.len()).sum();
        let open = self.open_dossier.as_mut()?;
        if open.view.is_none() || open.run_activity != run_activity {
            let card = self.snapshot.pipeline.card_in_folder(&open.folder)?;
            let contents = self.snapshot.archives.get(&open.folder).cloned().unwrap_or_default();
            let transcripts: Vec<RunTranscript> = runs.iter().map(RunTranscript::from).collect();
            open.view = Some(compose_dossier(card, &contents, &transcripts));
            open.run_activity = run_activity;
        }
        open.view.clone()
    }

    /// The tool check starts at launch, so the first run can say whether Claude Code is installed.
    /// It only asks each tool for its version, on a worker thread.
    /// The preferences reach the window here, before the first frame, and after that only when one changes.
    fn start(ctx: &egui::Context, path: Option<PathBuf>, data: Option<PathBuf>) -> Self {
        let repaint = ctx.clone();
        let environment = workspace::environment_checks(Arc::new(move || repaint.request_repaint()));
        Self::launch(ctx, path, data, environment)
    }

    /// `start` with the tool check given, so tests can launch without asking real tools.
    /// The first launch with no settings file keeps the theme and zoom eframe restored, and saves them.
    fn launch(ctx: &egui::Context, path: Option<PathBuf>, data: Option<PathBuf>, mut environment: EnvironmentChecks) -> Self {
        environment.ensure_started();
        let restored = settings::ui::restored(ctx);
        let settings = data.as_deref().map_or_else(Settings::unsaved, |data| settings::open_settings(data, restored));
        settings::ui::apply(ctx, settings.preferences());
        let first = start_screen(settings.preferences().open_on);
        let mut shell = Self { data, environment: Some(environment), settings, ..Self::empty(first, Local::now().date_naive()) };
        if let Some(path) = path {
            shell.open(ctx, &path);
        }
        // Without a folder only Today answers, and the welcome shows there.
        if shell.workspace.is_none() {
            shell.screen = Screen::InTray;
            shell.launcher_from = Screen::InTray;
        }
        shell
    }

    fn open(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        match workspace::open(path) {
            Some(root) => {
                let repaint = ctx.clone();
                self.updates = Some(live_snapshot::spawn(root.clone(), move || repaint.request_repaint()));
                if let Some(data) = &self.data {
                    let repaint = ctx.clone();
                    self.desk = Some(runs::open_desk(root.path(), data.join("runs"), Arc::new(move || repaint.request_repaint())));
                }
                self.tray = Some(self.data.as_ref().map_or_else(InTray::unsaved, |data| intray::open_tray(data.join("intray"), root.path())));
                let repaint = ctx.clone();
                self.pages = Some(dossier::page_store(Arc::new(move || repaint.request_repaint())));
                self.open_dossier = None;
                // The launcher no longer leads back into the last folder's screens.
                self.launcher_from = self.screen.tab();
                // The last folder's data stays off screen while this one is read.
                self.snapshot = Snapshot { today: Local::now().date_naive(), ..Snapshot::default() };
                self.loaded = false;
                self.workspace = Some(root);
                self.folder_error = None;
            }
            None => self.folder_error = Some(format!("{} is not an ai-job-search checkout.", path.display())),
        }
    }

    fn choose_folder(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new().set_title("Choose your ai-job-search folder").pick_folder() {
            self.open(ctx, &path);
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        if let Some(latest) = self.updates.as_ref().and_then(|updates| updates.try_iter().last()) {
            self.snapshot = latest;
            self.loaded = true;
            if let Some(open) = &mut self.open_dossier {
                open.view = None;
            }
        }
        if let Some(desk) = &mut self.desk {
            desk.poll();
        }
        if let Some(pages) = &mut self.pages {
            pages.poll();
        }
        if let Some(environment) = &mut self.environment {
            environment.poll();
        }
        // Cmd + and Cmd − move the zoom on any screen, and the text size follows.
        settings::ui::follow_zoom(ui.ctx(), &mut self.settings);
        self.handle_shortcuts_and_quit(ui.ctx());
        let queue = self.queue();
        // The desk goes down first, under every panel, so the grid holds still behind the menu and the screens.
        desk(ui, |ui| {
            // Settings takes the menu's place too, so its page keeps the most height.
            if !self.settings_open {
                Panel::top("desk_top").show_separator_line(false).frame(shell_top_frame()).show(ui, |ui| self.desk_top(ui, queue.cards.len()));
            }
            // Its separator is the strip's top hairline, in `line`.
            Panel::bottom("status_bar").show_separator_line(true).frame(status_frame(ui)).show(ui, |ui| self.status_bar(ui));
            egui::CentralPanel::default().frame(sheet_frame()).show(ui, |ui| {
                if self.settings_open {
                    self.settings(ui);
                } else if self.workspace.is_some() {
                    self.content(ui, &queue);
                } else {
                    self.welcome(ui);
                }
            });
        });
        if let Some(line) = take_command_request(ui.ctx()) {
            self.launcher.prefill(&line);
            self.open_launcher();
        }
        if self.confirm_quit {
            self.quit_dialog(ui.ctx());
        }
        note_focus(ui.ctx());
    }

    /// Runs change between snapshots, so the queue is dealt again each frame.
    fn queue(&mut self) -> Queue {
        let Some(tray) = &mut self.tray else { return Queue::default() };
        if self.screen != Screen::InTray {
            tray.stop_reading();
        }
        let snapshot = &self.snapshot;
        let runs = self.desk.as_ref().map_or(&[][..], RunDesk::runs);
        tray.queue(&Sources { board: &snapshot.board, pipeline: &snapshot.pipeline, archives: &snapshot.archives, runs, today: snapshot.today })
    }

    fn follow(&mut self, ctx: &egui::Context, request: Request) {
        match request {
            Request::Command(line) => {
                self.launcher.prefill(&line);
                self.open_launcher();
            }
            Request::Dossier(folder) => self.show_dossier(folder),
            Request::Run(id) => {
                self.run_screen.select(id);
                self.screen = Screen::Runs;
            }
            Request::Posting(url) => ctx.open_url(egui::OpenUrl::new_tab(url.as_str())),
            Request::Copy(text) => ctx.copy_text(text),
        }
    }

    /// Opens the launcher and keeps where it came from, for Esc. It closes Settings, which keeps its page for next time.
    fn open_launcher(&mut self) {
        self.settings_open = false;
        if self.screen != Screen::Launcher {
            self.launcher_from = self.screen;
        }
        self.screen = Screen::Launcher;
    }

    fn handle_shortcuts_and_quit(&mut self, ctx: &egui::Context) {
        // While the quit dialog asks, the first Esc means Keep working, whatever is open or focused behind it,
        // and the launcher's Cmd Enter starts nothing.
        if self.confirm_quit {
            let keep = ctx.input_mut(|input| {
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter);
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
            });
            if keep {
                self.confirm_quit = false;
            }
        }
        let active = self.desk.as_ref().is_some_and(RunDesk::has_live_process);
        if ctx.input(|input| input.viewport().close_requested()) && active {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_focus |= !self.confirm_quit;
            self.confirm_quit = true;
        }
        // Taken before any widget runs, even while typing: a text field reads Ctrl K as delete to the end of the line.
        // Cmd , is taken the same way, so Settings opens from a field too.
        let (ask, open_settings) =
            ctx.input_mut(|input| (input.consume_key(egui::Modifiers::COMMAND, egui::Key::K), input.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)));
        // While the quit dialog asks, no shortcut reaches the screens behind it.
        if self.confirm_quit {
            note_dialog(ctx);
            return;
        }
        // Settings opens on the page last open, with or without a folder.
        if open_settings {
            self.settings_open = true;
        }
        if ask && self.workspace.is_some() {
            self.open_launcher();
        }
    }

    /// Keep working is the default, and Esc keeps working too, as on End session's popover.
    fn quit_dialog(&mut self, ctx: &egui::Context) {
        egui::Window::new("Quit Jobdesk?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("A Claude session is still open. Quitting ends it. You can resume it later from Tasks.");
            ui.horizontal(|ui| {
                let keep = primary_button(ui, "Keep working");
                if self.quit_focus && !ui.is_sizing_pass() {
                    keep.request_focus();
                    self.quit_focus = false;
                }
                if keep.clicked() {
                    self.confirm_quit = false;
                }
                if button(ui, "End session and quit").clicked() {
                    self.desk = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.label(rich(ui, "Esc keeps working.", Type::NOTE, Ink::Faint));
        });
    }

    /// The menu on the desk, in one row: brand, the tabs, and New task at the right. The open tab is highlighted.
    /// Before a folder is open, only the lit Today tab answers: the rest have nothing to show yet.
    fn desk_top(&mut self, ui: &mut Ui, pending: usize) {
        ui.spacing_mut().item_spacing.y = 0.0;
        let open = self.workspace.is_some();
        let waiting = self.desk.as_ref().map_or(0, RunDesk::waiting_count);
        let lit = self.screen.tab();
        let setup_tab = self.setup_tab();
        let mut picked = None;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), NAV_TAB_HEIGHT), Layout::left_to_right(Align::Center), |ui| {
            brand(ui);
            ui.add_space(SHELL_GAP.0);
            ui.spacing_mut().item_spacing.x = NAV_TAB_GAP;
            for screen in Screen::NAV.into_iter().filter(|&screen| screen != Screen::Workspace || setup_tab) {
                let failed = self.snapshot.issues.iter().find(|issue| source_place(issue.source).0 == Some(screen)).map(|issue| issue.source);
                let mark = match screen {
                    _ if failed.is_some() => TabMark::Failed,
                    Screen::InTray => TabMark::Count(pending),
                    Screen::Runs => TabMark::Count(waiting),
                    _ => TabMark::None,
                };
                let tab = enabled_when(ui, open || screen == lit, CHOOSE_FOLDER, |ui| folder_tab(ui, screen.title(), mark, screen == lit));
                // The mark is a bare square, so the tab says what failed in the alert's words.
                let tab = match failed {
                    Some(source) => tab.on_hover_text(failed_words(source).0),
                    None => tab,
                };
                if tab.clicked() {
                    picked = Some(screen);
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if enabled_when(ui, open, CHOOSE_FOLDER, |ui| shortcut_button(ui, "New task", &ASK_KEYS)).clicked() {
                    self.open_launcher();
                }
            });
        });
        if let Some(screen) = picked {
            self.screen = screen;
        }
    }

    /// What still stands between the user and a working setup. Tools not checked yet count as nothing.
    fn readiness(&self) -> Readiness {
        readiness(&self.snapshot.workspace, self.environment.as_ref().and_then(EnvironmentChecks::results))
    }

    /// The Setup tab shows until a folder is chosen and every required tool answers, so through first onboarding.
    /// Profile files never keep it. While Setup is open it stays, so the screen on show is never pulled away.
    fn setup_tab(&self) -> bool {
        self.workspace.is_none() || !self.readiness().ready() || self.screen == Screen::Workspace
    }

    /// What the status bar says about the screen on show. In Settings only Runtime status has a note.
    fn screen_note(&self) -> Option<String> {
        let sign_in = || match auth_mode(self.desk.as_ref().and_then(RunDesk::latest_auth_source)) {
            AuthMode::ClaudeLogin => "Tasks use your Claude login. Jobdesk stores no credentials.".to_owned(),
            AuthMode::ApiKey => "Tasks use an API key or provider. Jobdesk stores no credentials.".to_owned(),
            AuthMode::NoRunYet => "You will see which account Claude uses after the first task. Jobdesk stores no credentials.".to_owned(),
        };
        if self.settings_open {
            return (self.settings_screen.page() == Page::Runtime).then(sign_in);
        }
        match self.screen {
            Screen::Dossier => Some("Source: tracker row and application archive".to_owned()),
            Screen::Workspace => Some(sign_in()),
            _ => None,
        }
    }

    /// The status strip at the desk's bottom edge. What waits on the user shows on the tabs, not here.
    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = STATUS_GAP.0;
            let waiting = self.desk.as_ref().map_or(0, RunDesk::waiting_count);
            // Settings hides the tabs and their counts, so a task that waits says so here instead.
            // Otherwise nothing while the probe runs. Once it has, a missing or broken `claude` shows as such.
            if self.settings_open && waiting > 0 {
                status_waits(ui, "A task waits for you");
            } else if let Some(environment) = self.environment.as_ref().filter(|environment| environment.results().is_some()) {
                let ready = environment.version_of("claude").is_some();
                // Before a folder is open, the line also says how Jobdesk signs in: through Claude Code alone.
                let text = match (ready, self.workspace.is_some()) {
                    (true, true) => "Claude is ready",
                    (true, false) => "Claude is installed. Jobdesk uses its sign-in and stores no passwords.",
                    (false, _) => "Claude Code is not installed",
                };
                status_line(ui, ready, text);
            }
            // Setup and Runtime status always say how tasks sign in, as docs/auth-review.md promises.
            if let Some(note) = self.screen_note() {
                status_note(ui, &note, false);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if status_button(ui, "Settings", &SETTINGS_KEYS, self.settings_open).clicked() {
                    self.settings_open = true;
                }
            });
        });
    }

    /// The first run: a card centred on the desk with the three steps to a first card. Only the folder can be chosen here.
    fn welcome(&mut self, ui: &mut Ui) {
        ui.add_space(WELCOME_TOP);
        let width = (WELCOME_WIDTH + f32::from(WINDOW_SHADOW)).min(ui.available_width());
        let rect = Rect::from_min_size(pos2(ui.max_rect().center().x - width / 2.0, ui.cursor().top()), vec2(width, ui.available_height()));
        let today = self.snapshot.today;
        let error = self.folder_error.as_deref();
        let choose = ui
            .scope_builder(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)), |ui| {
                paper(ui, true, |ui| {
                    let (x, y) = WELCOME_PADDING;
                    Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| welcome_card(ui, today, error)).inner
                })
            })
            .inner;
        if choose {
            let ctx = ui.ctx().clone();
            self.choose_folder(&ctx);
        }
    }

    /// A source that could not be read frames an alert on every screen. The screens with a head of their own draw it under the head,
    /// and a screen whose source failed hides its content instead of showing it empty.
    fn content(&mut self, ui: &mut Ui, queue: &Queue) {
        let failures = self.failures();
        let open_setup = Cell::new(false);
        // The alert leads to the tools: Setup while its tab shows, else Settings' Runtime status. Setup has no button to itself.
        let setup_tab = self.setup_tab();
        let leads_to = match self.screen {
            Screen::Workspace => None,
            _ if setup_tab => Some("Open Setup"),
            _ => Some("Open Runtime status"),
        };
        let alerts = |ui: &mut Ui| {
            if !failures.is_empty() && source_alert(ui, &failures, leads_to) {
                open_setup.set(true);
            }
        };
        let loaded = self.loaded;
        let unread: Vec<Source> = failures.iter().map(|(source, _)| *source).collect();
        let failed = |source: Source| unread.contains(&source);
        let load = |source: Source| {
            if failed(source) {
                Load::Failed
            } else if loaded {
                Load::Ready
            } else {
                Load::Reading
            }
        };
        let whole = if loaded { Load::Ready } else { Load::Reading };
        let state = |load: Load| ScreenState { load, unread: &unread, alerts: &alerts };
        // Each screen says where its alert goes, so a new one must choose.
        let alert_over_head = match self.screen {
            Screen::InTray | Screen::Jobs | Screen::Tracker | Screen::Insights | Screen::Workspace => false,
            // These keep it over their head until they take a `ScreenState` too.
            Screen::Runs | Screen::Launcher | Screen::Dossier => true,
        };
        if !failures.is_empty() && alert_over_head {
            page_body(ui, alerts);
        }
        let snapshot = &self.snapshot;
        match self.screen {
            Screen::InTray if !self.loaded => intray::ui::loading(ui, snapshot.today),
            Screen::InTray => {
                let keys = !self.confirm_quit;
                let (jobs, applications) = (failed(Source::Jobs), failed(Source::Applications));
                // In the alert's order, so the subtitle names them as the alert does.
                let missing: Vec<&str> =
                    failures.iter().filter_map(|(source, _)| match source_place(*source) { (Some(screen), true) => Some(screen.title()), _ => None }).collect();
                let missing_dates = match (jobs, applications) {
                    (true, true) => Some("Job and application dates are missing until the files are fixed."),
                    (true, false) => Some("Job closing dates are missing until the file is fixed."),
                    (false, true) => Some("Application dates are missing until the file is fixed."),
                    (false, false) => None,
                };
                let context = TodayContext { missing: &missing, missing_dates, alerts: &alerts };
                if let Some(request) = self.tray.as_mut().and_then(|tray| intray::ui::show(ui, &mut self.today, tray, queue, &context, keys)) {
                    self.follow(ui.ctx(), request);
                }
            }
            Screen::Jobs => {
                let kept = self.tray.as_ref().map(InTray::kept_jobs).unwrap_or_default();
                self.jobs.show(ui, &state(load(Source::Jobs)), &snapshot.board, &queue.waiting().jobs, &kept);
            }
            Screen::Tracker => {
                if let Some(folder) = self.tracker.show(ui, &state(load(Source::Applications)), &snapshot.pipeline, &queue.waiting().applications) {
                    self.show_dossier(folder);
                }
            }
            Screen::Dossier => {
                let today = snapshot.today;
                let tracker_failed = failed(Source::Applications);
                let view = self.refreshed_dossier();
                match (view, &mut self.pages) {
                    (Some(view), Some(pages)) => {
                        self.dossier_screen.show(ui, &view, pages, today);
                    }
                    // A failed tracker loads with no rows, so the application is only hidden, and comes back with the file.
                    _ if tracker_failed => {
                        page_body(ui, |ui| hidden_note(ui, insights::ui::TRACKER_HIDDEN));
                    }
                    _ if !self.loaded => {
                        page_body(ui, |ui| muted(ui, "Reading your applications…"));
                    }
                    // The one place that decides a dossier has gone, whichever way it was reached: back to Applications.
                    _ => {
                        self.screen = Screen::Dossier.tab();
                        ui.ctx().request_repaint();
                    }
                }
            }
            Screen::Insights => {
                // Each failed source hides only the parts it feeds, so the screen as a whole never fails.
                insights::ui::show(ui, &state(whole), &snapshot.insights, &snapshot.band, snapshot.skill_gaps.as_ref());
            }
            Screen::Runs => {
                // The workspace folder as text, so the story can shorten paths without reading the disk.
                let root = self.workspace.as_ref().map(|workspace| workspace.path().display().to_string()).unwrap_or_default();
                if let Some(desk) = &mut self.desk && self.run_screen.show(ui, desk, &root) == Some(RunScreenRequest::NewTask) {
                    self.open_launcher();
                }
            }
            Screen::Workspace => {
                let name = self.workspace.as_ref().map(WorkspaceRoot::name).unwrap_or_default();
                // Only previews lack the tool check. They draw no Setup, but still the alert.
                let action = match self.environment.as_mut() {
                    Some(environment) => workspace::ui::show(ui, &state(whole), &name, &snapshot.workspace, environment),
                    None => {
                        page_body(ui, alerts);
                        None
                    }
                };
                if action == Some(WorkspaceAction::ChangeFolder) {
                    let ctx = ui.ctx().clone();
                    self.choose_folder(&ctx);
                }
            }
            Screen::Launcher => {
                if back_pressed(ui.ctx()) {
                    self.screen = self.launcher_from;
                    ui.ctx().request_repaint();
                } else if let Some(desk) = &mut self.desk && let Some(id) = self.launcher.show(ui, desk) {
                    self.run_screen.select(id);
                    self.screen = Screen::Runs;
                }
            }
        }
        if open_setup.get() && setup_tab {
            self.screen = Screen::Workspace;
        } else if open_setup.get() {
            self.open_settings(Page::Runtime);
        }
    }

    /// Every source that could not be read. The tray reports its own file.
    /// It is copied only while it fails, so the tray stays free to take decisions.
    fn failures(&self) -> Vec<(Source, LoadError)> {
        let decisions = self.tray.as_ref().and_then(InTray::issue).cloned();
        self.snapshot.issues.iter().map(|issue| (issue.source, issue.error.clone())).chain(decisions.map(|error| (Source::Decisions, error))).collect()
    }

    /// Settings in place of the menu and the screen, which waits underneath. A running task keeps running.
    /// Runtime status is the workspace's page, and the only one that shows the alert.
    fn settings(&mut self, ui: &mut Ui) {
        let failures = self.failures();
        let unread: Vec<Source> = failures.iter().map(|(source, _)| *source).collect();
        let alerts = |ui: &mut Ui| {
            if !failures.is_empty() {
                source_alert(ui, &failures, None);
            }
        };
        let state = ScreenState { load: if self.loaded { Load::Ready } else { Load::Reading }, unread: &unread, alerts: &alerts };
        let found = self.readiness();
        let runtime = match found {
            Readiness { missing, .. } if missing > 0 => RuntimeWord::Missing(missing),
            Readiness { to_fill, .. } if to_fill > 0 => RuntimeWord::ToFill(to_fill),
            _ => RuntimeWord::None,
        };
        let folder_open = self.workspace.is_some();
        // Runtime status draws the folder's sections only once it is read, and a search matches only what it draws.
        let folder_read = folder_open && self.loaded;
        let folder = self.folder_label.clone().or_else(|| self.workspace.as_ref().map(|workspace| workspace.path().display().to_string()));
        let sign_in = auth_mode(self.desk.as_ref().and_then(RunDesk::latest_auth_source));
        let (environment, view) = (&mut self.environment, &self.snapshot.workspace);
        let mut change_folder = false;
        let (x, top, _) = SHELL_PADDING;
        let margin = Margin { left: x as i8, right: SCREEN_PADDING.0 as i8, top: top as i8, bottom: top as i8 };
        let asked = Frame::new()
            .inner_margin(margin)
            .show(ui, |ui| {
                let page_view = SettingsView { settings: &mut self.settings, folder: folder.as_deref(), folder_read, runtime };
                settings::ui::show(ui, &mut self.settings_screen, page_view, &mut |ui, page, query| match (page, environment.as_mut()) {
                    (Page::Runtime, Some(environment)) => {
                        change_folder |= workspace::ui::runtime_status(ui, &state, view, environment, sign_in, folder_open, query) == Some(WorkspaceAction::ChangeFolder);
                    }
                    // Only previews lack the tool check. They draw no tools, but still the alert.
                    (Page::Runtime, None) => alerts(ui),
                    _ => {}
                })
            })
            .inner;
        let ctx = ui.ctx().clone();
        match asked {
            Some(SettingsRequest::Back) => {
                self.settings_open = false;
                ctx.request_repaint();
            }
            Some(SettingsRequest::ChangeFolder) => change_folder = true,
            Some(SettingsRequest::ShowFolder) => {
                if let Some(workspace) = &self.workspace {
                    settings::reveal_folder(workspace.path());
                }
            }
            None => {}
        }
        if change_folder {
            self.choose_folder(&ctx);
        }
    }
}

/// The screen Jobdesk opens on, as General's Open on names it.
fn start_screen(open_on: StartScreen) -> Screen {
    match open_on {
        StartScreen::Today => Screen::InTray,
        StartScreen::Jobs => Screen::Jobs,
        StartScreen::Applications => Screen::Tracker,
        StartScreen::Insights => Screen::Insights,
        StartScreen::Tasks => Screen::Runs,
    }
}

/// The welcome's stamp and title, its steps, then the folder promise. Returns whether Choose folder was pressed.
fn welcome_card(ui: &mut Ui, today: NaiveDate, error: Option<&str>) -> bool {
    ui.set_width(ui.available_width());
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = HEAD_GAP.0;
        intray::ui::dated_stamp(ui, today, 0, 0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(rich(ui, "Welcome to Jobdesk", Type::CLEAR, Ink::Text));
            let intro = "Your job search as a short list of decisions, one at a time. Claude does the legwork, and nothing runs until you say so.";
            ui.add(egui::Label::new(rich(ui, intro, Type::TAKE, Ink::Muted)).wrap());
        });
    });
    ui.add_space(WELCOME_STEP_PADDING);
    let choose = welcome_step(ui, 1, "Choose your job search folder", "The ai-job-search folder you set up with Claude Code.", true);
    // Setup can leave the menu once the folder is chosen, so the step names the place that always stays.
    let profile = format!("Settings › Runtime status shows which parts are filled in and what is missing. {} opens Settings.", SETTINGS_KEYS.join(" "));
    welcome_step(ui, 2, "Check your profile", &profile, false);
    welcome_step(ui, 3, "Find your first jobs", "Opens the search task. You start it, and Today fills in as it works.", false);
    ui.add_space(WELCOME_STEP_PADDING / 2.0);
    if let Some(error) = error {
        alert(ui, error);
        ui.add_space(WELCOME_STEP_PADDING / 2.0);
    }
    let promise = "Jobdesk reads this folder and never changes it. Only a task you start can change it, through Claude Code on this computer.";
    ui.add(egui::Label::new(rich(ui, promise, Type::NOTE, Ink::Muted)).wrap());
    choose
}

/// One first-run step under a hairline. Only the step to do next has a button, and it says whether it was pressed.
fn welcome_step(ui: &mut Ui, number: usize, title: &str, detail: &str, next: bool) -> bool {
    divider(ui);
    ui.add_space(WELCOME_STEP_PADDING);
    let pressed = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = KNOT_GAP;
            knot(ui, &number.to_string(), if next { KnotState::Next } else { KnotState::Ahead }, KNOT_SIZE);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                strong(ui, title);
                ui.label(rich(ui, detail, Type::NOTE, Ink::Muted));
            });
            next && ui.with_layout(Layout::right_to_left(Align::Center), |ui| primary_button(ui, "Choose folder…")).inner.clicked()
        })
        .inner;
    ui.add_space(WELCOME_STEP_PADDING);
    pressed
}

/// The tab a source's failure marks, if any, and whether Today deals cards from it.
/// Decisions mark no tab: Today's count stays, and the alert says what failed.
fn source_place(source: Source) -> (Option<Screen>, bool) {
    match source {
        Source::Jobs => (Some(Screen::Jobs), true),
        Source::Applications => (Some(Screen::Tracker), true),
        Source::SkillGaps => (Some(Screen::Insights), false),
        Source::Decisions => (None, false),
    }
}

/// What an alert says when a source could not be read: what failed, then what still works when it fails alone.
fn failed_words(source: Source) -> (&'static str, &'static str) {
    match source {
        Source::Applications => (
            "Your applications could not be read.",
            "Today shows new matches and tasks until it is fixed. Jobs, Insights and Tasks still work.",
        ),
        Source::Jobs => (
            "Your saved jobs could not be read.",
            "Today shows applications and tasks until it is fixed. Applications, Insights and Tasks still work.",
        ),
        Source::SkillGaps => ("Your skill gap reports could not be read.", "Everything else still works."),
        Source::Decisions => ("Your decisions could not be saved or read.", "Today still works, but what you decide may be gone after Jobdesk closes."),
    }
}

/// Every source that could not be read, in one frame: what failed and why, then what still works.
/// Each sentence on what still works assumes its source failed alone, so with more than one failure none shows.
/// A reason quotes a file, so it shows as plain text only. `leads_to` is the button to the tools, if any, and the return says whether it was pressed.
fn source_alert(ui: &mut Ui, failures: &[(Source, LoadError)], leads_to: Option<&str>) -> bool {
    prompt_box_tone(ui, Tone::Closed, |ui| {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                let open = leads_to.is_some_and(|text| button(ui, text).clicked());
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for (source, error) in failures {
                            ui.label(rich(ui, failed_words(*source).0, Type::STRONG, Tone::Closed));
                            copyable(ui, |ui| ui.add(Label::new(error.to_string()).wrap()));
                        }
                        if let [(only, _)] = failures {
                            ui.add(Label::new(failed_words(*only).1).wrap());
                        }
                    });
                });
                open
            })
            .inner
        })
        .inner
    })
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(workspace) = &self.workspace {
            storage.set_string(WORKSPACE_KEY, workspace.path().display().to_string());
        }
    }
}

pub fn install_look(ctx: &egui::Context) {
    install_fonts(ctx);
    install_theme(ctx);
}

/// `--workspace <path>` wins over `JOBDESK_WORKSPACE`, which wins over the last folder used.
fn requested_workspace() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    args.find(|arg| arg == "--workspace").and_then(|_| args.next()).or_else(|| std::env::var("JOBDESK_WORKSPACE").ok()).map(PathBuf::from)
}

pub fn run() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/brand/jobdesk-icon-1024.png")).unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 820.0]).with_min_inner_size([1040.0, 640.0]).with_title("Jobdesk").with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "Jobdesk",
        options,
        Box::new(|creation| {
            install_look(&creation.egui_ctx);
            let saved = creation.storage.and_then(|storage| storage.get_string(WORKSPACE_KEY)).map(PathBuf::from);
            let data = eframe::storage_dir("Jobdesk");
            Ok(Box::new(Shell::start(&creation.egui_ctx, requested_workspace().or(saved), data)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::settings::application::{Preferences, ThemeChoice};
    use crate::workspace::application::{ToolProber, ToolSpec};

    /// Every tool answers, so launching asks no real program.
    struct AllTools;

    impl ToolProber for AllTools {
        fn probe(&self, _: &ToolSpec) -> Option<String> {
            Some("1.0".to_owned())
        }
    }

    fn launch(ctx: &egui::Context, path: Option<PathBuf>, data: &Path) -> Shell {
        Shell::launch(ctx, path, Some(data.to_owned()), EnvironmentChecks::new(Arc::new(AllTools), Arc::new(|| {})))
    }

    /// An empty folder that opens as a checkout.
    fn checkout(folder: &tempfile::TempDir) -> PathBuf {
        let root = folder.path().join("ai-job-search");
        fs::create_dir_all(root.join(".claude/commands")).unwrap();
        fs::write(root.join(".claude/commands/apply.md"), "").unwrap();
        root
    }

    /// The first launch after the upgrade has no settings file: the theme and zoom eframe restored stay, and are saved.
    #[test]
    fn a_first_launch_keeps_the_restored_theme_and_zoom() {
        let data = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.options_mut(|options| {
            options.theme_preference = egui::ThemePreference::Dark;
            options.zoom_factor = 1.25;
        });
        let shell = launch(&ctx, None, data.path());
        assert_eq!((shell.settings.preferences().theme, shell.settings.preferences().text_size.percent()), (ThemeChoice::Dark, 125));
        assert_eq!(ctx.options(|options| options.theme_preference), egui::ThemePreference::Dark);
        assert_eq!(ctx.zoom_factor(), 1.25);
        let saved = settings::open_settings(data.path(), Preferences::default());
        assert_eq!((saved.preferences().theme, saved.preferences().text_size.percent()), (ThemeChoice::Dark, 125), "the file holds them");
    }

    /// A file that cannot be read is not a missing one: the defaults apply, and the file is left for the next change.
    #[test]
    fn an_unreadable_file_takes_nothing_from_the_window() {
        let data = tempfile::tempdir().unwrap();
        fs::write(data.path().join("settings.json"), "{not json").unwrap();
        let ctx = egui::Context::default();
        ctx.options_mut(|options| options.theme_preference = egui::ThemePreference::Dark);
        let shell = launch(&ctx, None, data.path());
        assert_eq!(shell.settings.preferences().theme, ThemeChoice::System);
        assert_eq!(fs::read_to_string(data.path().join("settings.json")).unwrap(), "{not json");
    }

    #[test]
    fn open_on_chooses_the_launch_screen() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        settings::open_settings(data.path(), Preferences::default()).set_open_on(StartScreen::Jobs);
        let ctx = egui::Context::default();
        let shell = launch(&ctx, Some(checkout(&folder)), data.path());
        assert_eq!(shell.screen, Screen::Jobs);
    }

    /// Without a folder only Today answers, so the welcome shows there whatever Open on says.
    #[test]
    fn with_no_folder_the_launch_screen_is_today() {
        let data = tempfile::tempdir().unwrap();
        settings::open_settings(data.path(), Preferences::default()).set_open_on(StartScreen::Insights);
        let ctx = egui::Context::default();
        let shell = launch(&ctx, None, data.path());
        assert_eq!((shell.screen, shell.launcher_from), (Screen::InTray, Screen::InTray));
    }
}
