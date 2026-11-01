use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use chrono::{Local, NaiveDate};
use egui::{Align, Frame, Label, Layout, Margin, Panel, Rect, ThemePreference, Ui, UiBuilder, pos2, vec2};

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
use crate::shared_kernel::LoadError;
use crate::shell::{Snapshot, Source, live_snapshot};
use crate::tracking::ui::TrackerScreen;
use crate::ui_kit::tokens::{
    HEAD_GAP, KNOT_GAP, KNOT_SIZE, NAV_TAB_GAP, NAV_TAB_HEIGHT, SHELL_GAP, STATUS_GAP, WELCOME_PADDING, WELCOME_STEP_PADDING, WELCOME_TOP, WELCOME_WIDTH, WINDOW_SHADOW,
};
use crate::ui_kit::{
    Ink, KnotState, TabMark, Tone, Type, alert, brand, button, copyable, desk, divider, enabled_when, folder_tab, install_fonts, install_theme, knot, muted, page_body, paper, primary_button, prompt_box_tone, rich,
    sheet_frame, shell_top_frame, shortcut_button, status_frame, status_line, status_note, status_segmented, strong, take_command_request,
};
use crate::workspace;
use crate::workspace::application::{AuthMode, EnvironmentChecks, WorkspaceRoot, auth_mode};
use crate::workspace::ui::WorkspaceAction;

const WORKSPACE_KEY: &str = "workspace";

/// The bundled fonts have no command key glyph, so macOS spells it out.
/// What the tabs and New task say while they wait for a folder.
const CHOOSE_FOLDER: &str = "Choose your job search folder first.";
const ASK_KEYS:[&str; 2] = if cfg!(target_os = "macos") { ["Cmd", "K"] } else { ["Ctrl", "K"] };

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
    dossier_screen: DossierScreen,
    pages: Option<PageStore>,
    open_dossier: Option<OpenDossier>,
    environment: Option<EnvironmentChecks>,
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
            dossier_screen: DossierScreen::default(),
            pages: None,
            open_dossier: None,
            environment: None,
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
    fn start(ctx: &egui::Context, path: Option<PathBuf>, data: Option<PathBuf>) -> Self {
        let repaint = ctx.clone();
        let mut environment = workspace::environment_checks(Arc::new(move || repaint.request_repaint()));
        environment.ensure_started();
        let mut shell = Self { data, environment: Some(environment), ..Self::empty(Screen::InTray, Local::now().date_naive()) };
        if let Some(path) = path {
            shell.open(ctx, &path);
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
        self.handle_shortcuts_and_quit(ui.ctx());
        let queue = self.queue();
        // The desk goes down first, under every panel, so the grid holds still behind the menu and the screens.
        desk(ui, |ui| {
            Panel::top("desk_top").show_separator_line(false).frame(shell_top_frame()).show(ui, |ui| self.desk_top(ui, queue.cards.len()));
            // Its separator is the strip's top hairline, in `line`.
            Panel::bottom("status_bar").show_separator_line(true).frame(status_frame(ui)).show(ui, |ui| self.status_bar(ui));
            egui::CentralPanel::default().frame(sheet_frame()).show(ui, |ui| {
                if self.workspace.is_some() {
                    self.content(ui, &queue);
                } else {
                    self.welcome(ui);
                }
            });
        });
        if let Some(line) = take_command_request(ui.ctx()) {
            self.launcher.prefill(&line);
            self.screen = Screen::Launcher;
        }
        if self.confirm_quit {
            self.quit_dialog(ui.ctx());
        }
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
                self.screen = Screen::Launcher;
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

    fn handle_shortcuts_and_quit(&mut self, ctx: &egui::Context) {
        // Taken before any widget runs, even while typing: a text field reads Ctrl K as delete to the end of the line.
        let ask = ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::K));
        if ask && self.workspace.is_some() {
            self.screen = Screen::Launcher;
        }
        let active = self.desk.as_ref().is_some_and(RunDesk::has_live_process);
        if ctx.input(|input| input.viewport().close_requested()) && active {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_quit = true;
        }
    }

    fn quit_dialog(&mut self, ctx: &egui::Context) {
        egui::Window::new("Quit Jobdesk?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("A Claude session is still open. Quitting ends it. You can resume it later from Tasks.");
            ui.horizontal(|ui| {
                if primary_button(ui, "Keep working").clicked() {
                    self.confirm_quit = false;
                }
                if button(ui, "End session and quit").clicked() {
                    self.desk = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }

    /// The menu on the desk, in one row: brand, the tabs, and New task at the right. The open tab is highlighted.
    /// Before a folder is open, only the lit Today tab answers: the rest have nothing to show yet.
    fn desk_top(&mut self, ui: &mut Ui, pending: usize) {
        ui.spacing_mut().item_spacing.y = 0.0;
        let open = self.workspace.is_some();
        let waiting = self.desk.as_ref().map_or(0, RunDesk::waiting_count);
        let lit = self.screen.tab();
        let mut picked = None;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), NAV_TAB_HEIGHT), Layout::left_to_right(Align::Center), |ui| {
            brand(ui);
            ui.add_space(SHELL_GAP.0);
            ui.spacing_mut().item_spacing.x = NAV_TAB_GAP;
            for screen in Screen::NAV {
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
                    self.screen = Screen::Launcher;
                }
            });
        });
        if let Some(screen) = picked {
            self.screen = screen;
        }
    }

    /// What the status bar says about the screen on show.
    fn screen_note(&self) -> Option<String> {
        match self.screen {
            Screen::Dossier => Some("Source: tracker row and application archive".to_owned()),
            Screen::Workspace => Some(match auth_mode(self.desk.as_ref().and_then(RunDesk::latest_auth_source)) {
                AuthMode::ClaudeLogin => "Tasks use your Claude login. Jobdesk stores no credentials.".to_owned(),
                AuthMode::ApiKey => "Tasks use an API key or provider. Jobdesk stores no credentials.".to_owned(),
                AuthMode::NoRunYet => "You will see which account Claude uses after the first task. Jobdesk stores no credentials.".to_owned(),
            }),
            _ => None,
        }
    }

    /// The status strip at the desk's bottom edge. What waits on the user shows on the tabs, not here.
    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = STATUS_GAP.0;
            // Nothing while the probe runs. Once it has, a missing or broken `claude` shows as such.
            if let Some(environment) = self.environment.as_ref().filter(|environment| environment.results().is_some()) {
                let ready = environment.version_of("claude").is_some();
                // Before a folder is open, the line also says how Jobdesk signs in: through Claude Code alone.
                let text = match (ready, self.workspace.is_some()) {
                    (true, true) => "Claude is ready",
                    (true, false) => "Claude is installed. Jobdesk uses its sign-in and stores no passwords.",
                    (false, _) => "Claude Code is not installed",
                };
                status_line(ui, ready, text);
            }
            // Setup always says how tasks sign in, as docs/auth-review.md promises.
            if let Some(note) = self.screen_note() {
                status_note(ui, &note, false);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let current = ui.ctx().options(|options| options.theme_preference);
                let options = [
                    (ThemePreference::Dark, "Dark".to_owned()),
                    (ThemePreference::Light, "Light".to_owned()),
                    (ThemePreference::System, "System".to_owned()),
                ];
                if let Some(theme) = status_segmented(ui, current, &options) {
                    ui.ctx().set_theme(theme);
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

    /// A source that could not be read frames an alert over every screen. Today draws it under its head.
    fn content(&mut self, ui: &mut Ui, queue: &Queue) {
        // The tray reports its own file. It is copied only while it fails, so the tray stays free to take decisions.
        let decisions = self.tray.as_ref().and_then(InTray::issue).cloned();
        let failures: Vec<(Source, &LoadError)> =
            self.snapshot.issues.iter().map(|issue| (issue.source, &issue.error)).chain(decisions.iter().map(|error| (Source::Decisions, error))).collect();
        let open_setup = Cell::new(false);
        let on_setup = self.screen == Screen::Workspace;
        let alerts = |ui: &mut Ui| {
            if !failures.is_empty() && source_alert(ui, &failures, on_setup) {
                open_setup.set(true);
            }
        };
        if !failures.is_empty() && self.screen != Screen::InTray {
            page_body(ui, alerts);
        }
        let snapshot = &self.snapshot;
        match self.screen {
            Screen::InTray if !self.loaded => intray::ui::loading(ui, snapshot.today),
            Screen::InTray => {
                let keys = !self.confirm_quit;
                let failed = |source| failures.iter().any(|(failed, _)| *failed == source);
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
                self.jobs.show(ui, &snapshot.board, &queue.waiting().jobs, &kept);
            }
            Screen::Tracker => {
                if let Some(folder) = self.tracker.show(ui, &snapshot.pipeline, &queue.waiting().applications) {
                    self.show_dossier(folder);
                }
            }
            Screen::Dossier => {
                let today = snapshot.today;
                let view = self.refreshed_dossier();
                match (view, &mut self.pages) {
                    (Some(view), Some(pages)) => {
                        self.dossier_screen.show(ui, &view, pages, today);
                    }
                    _ => {
                        page_body(ui, |ui| muted(ui, "This application is no longer in the tracker."));
                    }
                }
            }
            Screen::Insights => insights::ui::show(ui, &snapshot.insights, &snapshot.band, snapshot.skill_gaps.as_ref()),
            Screen::Runs => {
                // The workspace folder as text, so the story can shorten paths without reading the disk.
                let root = self.workspace.as_ref().map(|workspace| workspace.path().display().to_string()).unwrap_or_default();
                if let Some(desk) = &mut self.desk && self.run_screen.show(ui, desk, &root) == Some(RunScreenRequest::NewTask) {
                    self.screen = Screen::Launcher;
                }
            }
            Screen::Workspace => {
                let name = self.workspace.as_ref().map(WorkspaceRoot::name).unwrap_or_default();
                let action = self.environment.as_mut().and_then(|environment| workspace::ui::show(ui, &name, &snapshot.workspace, environment));
                if action == Some(WorkspaceAction::ChangeFolder) {
                    let ctx = ui.ctx().clone();
                    self.choose_folder(&ctx);
                }
            }
            Screen::Launcher => {
                if let Some(desk) = &mut self.desk && let Some(id) = self.launcher.show(ui, desk) {
                    self.run_screen.select(id);
                    self.screen = Screen::Runs;
                }
            }
        }
        if open_setup.get() {
            self.screen = Screen::Workspace;
        }
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
    welcome_step(ui, 2, "Check your profile", "Setup shows which parts are filled in and what is missing.", false);
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
/// A reason quotes a file, so it shows as plain text only. Returns whether Open Setup was pressed. Setup itself has no button.
fn source_alert(ui: &mut Ui, failures: &[(Source, &LoadError)], on_setup: bool) -> bool {
    prompt_box_tone(ui, Tone::Closed, |ui| {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                let open = !on_setup && button(ui, "Open Setup").clicked();
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
