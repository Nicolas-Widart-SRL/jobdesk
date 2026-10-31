use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use chrono::Local;
use egui::{Frame, Margin, Panel, RichText, ThemePreference, Ui};

use crate::discovery::ui::JobsScreen;
use crate::dossier;
use crate::dossier::application::{Dossier, PageStore, RunTranscript, compose_dossier};
use crate::dossier::ui::{DossierAction, DossierScreen};
use crate::insights;
use crate::intray;
use crate::intray::application::{InTray, Queue, Request, Sources};
use crate::runs;
use crate::runs::application::RunDesk;
use crate::runs::ui::{Launcher, RunScreen};
use crate::shell::{Snapshot, live_snapshot};
use crate::tracking::ui::TrackerScreen;
use crate::ui_kit::{Palette, faint_mono, install_fonts, install_theme, muted, panel, panel_body, primary_button, segmented, semibold, strong, take_command_request};
use crate::workspace;
use crate::workspace::application::{EnvironmentChecks, WorkspaceRoot, auth_mode};

const WORKSPACE_KEY: &str = "workspace";

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
            Self::InTray => "In-tray",
            Self::Jobs => "Jobs",
            Self::Tracker => "Tracker",
            Self::Insights => "Insights",
            Self::Runs => "Runs",
            Self::Workspace => "Workspace",
            Self::Launcher => "Run a command",
            Self::Dossier => "Application dossier",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::InTray => "Claude prepares, you decide",
            Self::Jobs => "Triage scores from posting text only. /apply re-evaluates with company research.",
            Self::Tracker => "Status changes go through /outcome",
            Self::Insights => "Search health and your latest skill gap report",
            Self::Runs => "Every command Jobdesk ran in this workspace",
            Self::Workspace => "Whether your checkout and tools are ready",
            Self::Launcher => "Every action is the unchanged slash command. Forms only build its arguments.",
            Self::Dossier => "Documents, evaluation, prep and outcome for one application",
        }
    }
}

/// Window state. It only reads snapshots.
pub struct Shell {
    screen: Screen,
    workspace: Option<WorkspaceRoot>,
    snapshot: Snapshot,
    updates: Option<Receiver<Snapshot>>,
    jobs: JobsScreen,
    tracker: TrackerScreen,
    folder_error: Option<String>,
    desk: Option<RunDesk>,
    launcher: Launcher,
    run_screen: RunScreen,
    data: Option<PathBuf>,
    tray: Option<InTray>,
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
    fn empty(screen: Screen) -> Self {
        Self {
            screen,
            workspace: None,
            snapshot: Snapshot { today: Local::now().date_naive(), ..Snapshot::default() },
            updates: None,
            jobs: JobsScreen::default(),
            tracker: TrackerScreen::default(),
            folder_error: None,
            desk: None,
            launcher: Launcher::default(),
            run_screen: RunScreen::default(),
            data: None,
            tray: None,
            confirm_quit: false,
            dossier_screen: DossierScreen::default(),
            pages: None,
            open_dossier: None,
            environment: None,
        }
    }

    /// A shell with fixed data, for tests and previews.
    pub fn with_snapshot(screen: Screen, workspace: WorkspaceRoot, snapshot: Snapshot) -> Self {
        Self { workspace: Some(workspace), snapshot, tray: Some(InTray::unsaved()), ..Self::empty(screen) }
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

    fn start(ctx: &egui::Context, path: Option<PathBuf>, data: Option<PathBuf>) -> Self {
        let mut shell = Self { data, ..Self::empty(Screen::InTray) };
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
                let repaint = ctx.clone();
                self.environment = Some(workspace::environment_checks(Arc::new(move || repaint.request_repaint())));
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
        let p = Palette::of(ui);
        Panel::left("sidebar")
            .exact_size(216.0)
            .resizable(false)
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(12, 14)))
            .show(ui, |ui| self.sidebar(ui, queue.cards.len()));
        Panel::bottom("status_bar").frame(Frame::new().fill(p.bg).inner_margin(Margin::symmetric(24, 5))).show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| {
            if self.workspace.is_some() {
                self.content(ui, &queue);
            } else {
                self.welcome(ui);
            }
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
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) && self.workspace.is_some() {
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
            ui.label("A Claude session is still open. Quitting ends it. You can resume it later from Runs.");
            ui.horizontal(|ui| {
                if primary_button(ui, "Keep working").clicked() {
                    self.confirm_quit = false;
                }
                if ui.button("End session and quit").clicked() {
                    self.desk = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }

    fn sidebar(&mut self, ui: &mut Ui, pending: usize) {
        let p = Palette::of(ui);
        ui.label(RichText::new("Jobdesk").size(14.0).family(semibold()));
        ui.add_space(10.0);
        if self.workspace.is_some() && primary_button(ui, "Run command").on_hover_text("Cmd+K or Ctrl+K").clicked() {
            self.screen = Screen::Launcher;
        }
        ui.add_space(10.0);
        let waiting = self.desk.as_ref().map_or(0, RunDesk::waiting_count);
        for screen in Screen::NAV {
            let on = self.screen == screen || (self.screen == Screen::Dossier && screen == Screen::Tracker);
            let title = match screen {
                Screen::Runs if waiting > 0 => format!("Runs  {waiting} waiting"),
                Screen::InTray if pending > 0 => format!("In-tray  {pending}"),
                _ => screen.title().to_owned(),
            };
            let text = RichText::new(title).color(if on { p.text } else { p.muted });
            let button = egui::Button::new(text).fill(if on { p.raised } else { egui::Color32::TRANSPARENT }).stroke(egui::Stroke::NONE).min_size(egui::vec2(ui.available_width(), 32.0)).wrap_mode(egui::TextWrapMode::Truncate);
            if ui.add(button).clicked() {
                self.screen = screen;
            }
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            if ui.button("Change folder").clicked() {
                let ctx = ui.ctx().clone();
                self.choose_folder(&ctx);
            }
            if let Some(workspace) = &self.workspace {
                faint_mono(ui, workspace.name());
            }
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let issues = self.snapshot.issues.len();
            if issues > 0 {
                ui.label(RichText::new(format!("{issues} source could not be read")).color(Palette::of(ui).closed));
            } else {
                muted(ui, "Slash commands stay the only writers of your files.");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let current = ui.ctx().options(|options| options.theme_preference);
                let options = [
                    (ThemePreference::Dark, "Dark".to_owned()),
                    (ThemePreference::Light, "Light".to_owned()),
                    (ThemePreference::System, "System".to_owned()),
                ];
                if let Some(theme) = segmented(ui, current, &options) {
                    ui.ctx().set_theme(theme);
                }
            });
        });
    }

    fn welcome(&mut self, ui: &mut Ui) {
        ui.add_space(64.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(420.0);
            panel(ui, |ui| {
                panel_body(ui, |ui| {
                    strong(ui, "Choose your ai-job-search folder");
                    muted(ui, "Jobdesk reads your checkout and never changes it.");
                    if let Some(error) = &self.folder_error {
                        ui.label(RichText::new(error).color(Palette::of(ui).closed));
                    }
                    if primary_button(ui, "Choose folder").clicked() {
                        let ctx = ui.ctx().clone();
                        self.choose_folder(&ctx);
                    }
                });
            });
        });
    }

    fn content(&mut self, ui: &mut Ui, queue: &Queue) {
        Frame::new().inner_margin(Margin::symmetric(24, 16)).show(ui, |ui| {
            if self.screen != Screen::InTray {
                ui.heading(self.screen.title());
                muted(ui, self.screen.subtitle());
                ui.add_space(12.0);
            }
            for issue in &self.snapshot.issues {
                ui.label(RichText::new(issue.to_string()).color(Palette::of(ui).closed));
            }
            let snapshot = &self.snapshot;
            match self.screen {
                Screen::InTray => {
                    let keys = !self.confirm_quit;
                    if let Some(request) = self.tray.as_mut().and_then(|tray| intray::ui::show(ui, tray, queue, keys)) {
                        self.follow(ui.ctx(), request);
                    }
                }
                Screen::Jobs => self.jobs.show(ui, &snapshot.board),
                Screen::Tracker => {
                    if let Some(folder) = self.tracker.show(ui, &snapshot.pipeline) {
                        self.show_dossier(folder);
                    }
                }
                Screen::Dossier => {
                    let view = self.refreshed_dossier();
                    match (view, &mut self.pages) {
                        (Some(view), Some(pages)) => {
                            if self.dossier_screen.show(ui, &view, pages) == Some(DossierAction::Back) {
                                self.screen = Screen::Tracker;
                            }
                        }
                        _ => {
                            muted(ui, "This application is no longer in the tracker.");
                        }
                    }
                }
                Screen::Insights => insights::ui::show(ui, &snapshot.insights, snapshot.skill_gaps.as_ref()),
                Screen::Runs => {
                    if let Some(desk) = &mut self.desk {
                        self.run_screen.show(ui, desk);
                    }
                }
                Screen::Workspace => {
                    let auth = auth_mode(self.desk.as_ref().and_then(RunDesk::latest_auth_source));
                    if let Some(environment) = &mut self.environment {
                        workspace::ui::show(ui, &snapshot.workspace, environment, auth);
                    }
                }
                Screen::Launcher => {
                    if let Some(desk) = &mut self.desk {
                        if let Some(id) = self.launcher.show(ui, desk) {
                            self.run_screen.select(id);
                            self.screen = Screen::Runs;
                        }
                    }
                }
            }
        });
    }
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
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png")).unwrap_or_default();
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
