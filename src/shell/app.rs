use std::cell::Cell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use egui::{Align, Frame, Label, Layout, Margin, Panel, Rect, Ui, UiBuilder, pos2, vec2};

use crate::discovery::application::hidden_total;
use crate::discovery::ui::JobsScreen;
use crate::dossier;
use crate::dossier::application::{Dossier, PageStore, RunTranscript, compose_dossier};
use crate::dossier::ui::DossierScreen;
use crate::folder_update;
use crate::folder_update::application::{FOLDER_WAITS, FolderUpdate, HOLD_RETRY, RESTART_WAITS};
use crate::folder_update::ui::{NoticeRequest, NoticeScreen};
use crate::insights;
use crate::intray;
use crate::intray::application::{InTray, Queue, Request, Sources};
use crate::intray::ui::{TodayContext, TodayScreen};
use crate::onboarding::application::{Onboarding, Stage as SetupStage};
use crate::onboarding::ui::{FirstRunRequest, FirstRunScreen};
use crate::portals;
use crate::portals::application::{Community, PortalChanges};
use crate::portals::ui::{BrowseContext, PortalsRequest, PortalsScreen};
use crate::profile;
use crate::profile::application::{ProfileEditor, UpdatedMarks};
use crate::profile::ui::{Away, Moment, ProfileRequest, ProfileScreen};
use crate::runs;
use crate::runs::application::RunDesk;
use crate::runs::ui::{LaunchRules, Launcher, LauncherRequest, RunScreen, RunScreenRequest};
use crate::settings;
use crate::settings::application::{
    CapCheck, ClaudeVersion, DataSizes, Page, PermissionMode, Providers, Retention, RuntimeWord, Saved, Settings, SignIn, StartScreen, TermsState, TestGate,
    UsageState, cap_check,
};
use crate::settings::ui::{LastTask, SettingsRequest, SettingsScreen, SettingsView};
use crate::shared_kernel::data_folder::DATA_DIR_VARIABLE;
use crate::shared_kernel::folder_wait::UNLOCKABLE;
use crate::shared_kernel::{FolderWait, LoadError};
use crate::shell::live_snapshot::{self, Reload};
use crate::shell::{Issue, Snapshot, Source};
use crate::tracking::ui::TrackerScreen;
use crate::ui_kit::tokens::{
    HEAD_GAP, KNOT_GAP, KNOT_SIZE, NAV_TAB_GAP, NAV_TAB_HEIGHT, SCREEN_PADDING, SHELL_GAP, SHELL_PADDING, STATUS_GAP, WELCOME_PADDING, WELCOME_STEP_PADDING,
    WELCOME_TOP, WELCOME_WIDTH, WINDOW_SHADOW,
};
use crate::ui_kit::{
    Ink, KnotState, Load, ScreenState, TabMark, Tone, Type, alert, back_pressed, brand, button, command_chord, copyable, desk, divider, enabled_when,
    folder_tab, hidden_note, install_fonts, install_theme, knot, muted, note_dialog, note_focus, page_body, paper, primary_button, prompt_box_tone, rich,
    sheet_frame, shell_top_frame, shortcut_button, status_button, status_frame, status_line, status_note, status_waits, strong, take_command_request,
};
use crate::updates;
use crate::updates::application::{BarAction, RestartPlan, SessionState, Updates, relaunch_args, restart_wait};
use crate::updates::ui::{UpdateRequest, UpdateScreen};
use crate::workspace;
use crate::workspace::application::{EnvironmentChecks, Readiness, WorkspaceRoot, auth_mode, readiness};
use crate::workspace::ui::WorkspaceAction;

const WORKSPACE_KEY: &str = "workspace";

/// What the tabs and New task say while they wait for a folder.
const CHOOSE_FOLDER: &str = "Choose your job search folder first.";
/// Why Change folder waits: a session stays open after its turn ends, until its process exits or the user ends it.
const FOLDER_BUSY: &str = "A Claude session is open. Stop it, then end it, in Tasks to change the folder.";
/// The keys that open the launcher.
const ASK_KEYS: [&str; 2] = command_chord("K");
/// The keys that open Settings.
const SETTINGS_KEYS: [&str; 2] = command_chord(",");
/// How long Save waits after a run for the snapshot worker's next load. Past it, Jobdesk reads the profile itself, so a
/// worker that died cannot hold Save off.
const RUN_READ_WAIT: Duration = Duration::from_secs(5);
/// How often Providers looks again while a task's start reads the keychain, perhaps behind its prompt.
const KEYCHAIN_LOOK_AGAIN: Duration = Duration::from_millis(250);
/// The window's size, and the smallest it may be, once the app shows.
const WINDOW: egui::Vec2 = vec2(1280.0, 820.0);
const SMALLEST_WINDOW: egui::Vec2 = vec2(1040.0, 640.0);
/// The small first window, for the welcome and the Folder and Computer steps of first-run setup. It reads as an
/// installer, not an app to learn.
const FIRST_WINDOW: egui::Vec2 = vec2(720.0, 560.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    InTray,
    Jobs,
    Tracker,
    Insights,
    Runs,
    Profile,
    Portals,
    Workspace,
    Launcher,
    Dossier,
}

impl Screen {
    const NAV: [Self; 8] = [Self::InTray, Self::Jobs, Self::Tracker, Self::Insights, Self::Runs, Self::Profile, Self::Portals, Self::Workspace];

    fn title(self) -> &'static str {
        match self {
            Self::InTray => "Today",
            Self::Jobs => "Jobs",
            Self::Tracker => "Applications",
            Self::Insights => "Insights",
            Self::Runs => "Tasks",
            Self::Profile => "Profile",
            Self::Portals => "Portals",
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

/// Window state. It reads the checkout through snapshots, except the profile and the portals: it writes one profile block
/// at a time through `ProfileEditor`, switches portals and moves them to the Trash through `PortalChanges`, and reads
/// what it wrote back at once, as it reads the profile after a run that no snapshot follows in time.
pub struct Shell {
    screen: Screen,
    workspace: Option<WorkspaceRoot>,
    snapshot: Snapshot,
    /// False from opening a folder until its first snapshot arrives. Today shows that it is reading.
    loaded: bool,
    snapshots: Option<Receiver<Snapshot>>,
    /// Asks the snapshot worker to read again, because run logs change outside the checkout.
    reload: Option<Reload>,
    /// The reload the last task's end asked for. Until a snapshot carries it, the month's spend misses that task's cost.
    usage_due: u64,
    /// Whether a run was working on the last frame, so its start and end can be seen. A run that stops asks for a reload.
    was_busy: bool,
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
    /// The Permissions setting the desk last took. A change reaches the desk once, on the frame after it is made.
    permissions: PermissionMode,
    /// How tasks sign in, the saved key and Test connection. The keychain is read once per launch, until a read succeeds, and the key then held in memory.
    providers: Providers,
    /// The sign-in the desk last took. A change reaches the desk once, as Permissions does.
    sign_in: SignIn,
    /// A fixed clock for the ages Providers shows, for snapshots. None reads the wall clock.
    now: Option<NaiveDateTime>,
    settings_screen: SettingsScreen,
    /// Settings covers the menu and the screen, which stays as it was underneath.
    settings_open: bool,
    /// The folder as Settings shows it, in place of its path. Snapshots set it, so no temporary path shows.
    folder_label: Option<String>,
    /// The data folder as Settings shows it, in place of its path, for snapshots.
    data_label: Option<String>,
    /// What the data folder holds: measured at launch, when a task ends and after a clear, never every frame.
    data_sizes: Option<DataSizes>,
    /// This copy's version as About shows it. Snapshots set their own, so a release moves none.
    version: String,
    profile_screen: ProfileScreen,
    /// The topics the last run changed, kept in memory only.
    marks: UpdatedMarks,
    /// When the last run ended, until a snapshot read after it shows what the run changed.
    run_ended: Option<Instant>,
    /// Saves one profile block at a time into the open folder.
    profile_editor: Option<ProfileEditor>,
    /// The tab the user picked while Profile asks whether to drop their draft.
    leave_to: Option<Screen>,
    /// When the profile was last read back, after a save or a run, so a snapshot read before it cannot show the old text again.
    profile_saved: Option<Instant>,
    portals_screen: PortalsScreen,
    /// Switches the open folder's portals and moves them to the Trash.
    portal_changes: Option<PortalChanges>,
    /// When the portals were last read back, after a switch or a removal, so a snapshot read before it cannot undo it on screen.
    portals_changed: Option<Instant>,
    /// The community portal list from GitHub, with the copy saved in the data folder. Reading it is not a run: no Claude,
    /// no run log, nothing counted in Usage. It names the installed portals that came from it, on every snapshot.
    community: Community,
    /// What waits, over every screen, on the ask to drop unsaved profile text, and whether Keep editing still has to
    /// take the focus.
    away: Option<(Pending, bool)>,
    /// The window was asked to close while the folder ask showed, so the close comes back once that ask is answered.
    quit_after: bool,
    /// The user agreed to drop their profile draft to quit. The draft stays until the window really closes, so a quit
    /// cancelled later, at the session ask, keeps it.
    quit_agreed: bool,
    /// Jobdesk's own updates: the check, the download, and what Restart to update applies. Not a run: no Claude, no
    /// run log, nothing counted in Usage.
    updates: Updates,
    /// Whether the release notes are open over the desk.
    update_screen: UpdateScreen,
    /// The switch for the automatic check as `updates` last took it. A change reaches it once, as Permissions does.
    auto_check: bool,
    /// The user chose Restart to update, and the window is closing for it. Any answer that keeps the window open clears it.
    restart_pending: bool,
    /// A withdrawn release stopped the restart's close this frame, so the close request is not asked about again.
    close_dropped: bool,
    /// Where the window leaves what Restart to update applies once it has closed, with the arguments to open again with.
    restart_slot: RestartSlot,
    /// The arguments Jobdesk started with, after the program's name.
    launch_args: Vec<OsString>,
    /// First-run setup, while it shows in place of the welcome: on macOS, when no folder was asked for or remembered
    /// and setup has not got past the Computer step.
    first_run: Option<FirstRun>,
    /// First-run setup's worker once setup has handed over, until it has seen the cancel and finished. The window
    /// never waits on it: it is joined on the first frame after it has finished. Quitting does not wait on it either,
    /// as for every other worker thread.
    setup_stopping: Option<JoinHandle<()>>,
    /// The size the window was last asked to take. None until first-run setup's first frame asks for the small one.
    window: Option<WindowSize>,
    /// The update of the open job search folder that comes with each Jobdesk update, its rollback at every open, and
    /// its Undo. Not a run: no Claude, no run log, nothing counted in Usage. While it works no run starts, Restart to
    /// update waits and the folder stays.
    folder_update: FolderUpdate,
    /// Whether the folder update's details are open over the desk.
    notice_screen: NoticeScreen,
}

/// What Restart to update applies once the window has closed, and the arguments the new copy opens with. `run` reads it.
pub type RestartSlot = Arc<Mutex<Option<(RestartPlan, Vec<OsString>)>>>;

/// What else writes the job search folder, so the folder's other writers wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderWriter {
    /// A task works, and Claude may write any file.
    Task,
    /// Jobdesk updates the folder, or undoes its update, or cannot lock it: which, for each writer's words.
    FolderUpdate(FolderWait),
}

/// What would drop unsaved profile text from outside Profile, once the user agrees.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    /// Opening the folder the user picked.
    Folder(PathBuf),
    Quit,
}

/// The dossier on screen. It is rebuilt when its files or saved runs change.
struct OpenDossier {
    folder: String,
    view: Option<Dossier>,
    run_activity: usize,
}

/// First-run setup and its screens.
struct FirstRun {
    onboarding: Onboarding,
    screen: FirstRunScreen,
}

/// A size the window is asked to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowSize {
    /// `FIRST_WINDOW`, with the smallest size lowered to it, whatever size was saved.
    FirstRun,
    /// `WINDOW`, or the screen when it is smaller but never below `SMALLEST_WINDOW`, which is the smallest again.
    App,
}

impl Shell {
    fn empty(screen: Screen, today: NaiveDate) -> Self {
        Self {
            screen,
            workspace: None,
            snapshot: Snapshot { today, ..Snapshot::default() },
            loaded: false,
            snapshots: None,
            reload: None,
            usage_due: 0,
            was_busy: false,
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
            permissions: PermissionMode::default(),
            // A key kept in memory and a test that never answers, so a preview or a test never reaches the system keychain or Claude.
            providers: settings::open_providers(settings::memory_keychain(None), settings::idle_tester()),
            sign_in: SignIn::ClaudeSignIn,
            now: None,
            settings_screen: SettingsScreen::default(),
            settings_open: false,
            folder_label: None,
            data_label: None,
            data_sizes: None,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            profile_screen: ProfileScreen::default(),
            marks: UpdatedMarks::default(),
            run_ended: None,
            profile_editor: None,
            leave_to: None,
            profile_saved: None,
            portals_screen: PortalsScreen::default(),
            portal_changes: None,
            portals_changed: None,
            // A list that never answers, so a preview or a test never reaches the network.
            community: portals::scripted_community(None, Vec::new()),
            away: None,
            quit_after: false,
            quit_agreed: false,
            // A check that never answers, so a preview or a test never reaches the network.
            updates: updates::idle_updates(env!("CARGO_PKG_VERSION"), true),
            update_screen: UpdateScreen::default(),
            auto_check: true,
            restart_pending: false,
            close_dropped: false,
            restart_slot: RestartSlot::default(),
            launch_args: Vec::new(),
            first_run: None,
            setup_stopping: None,
            // The window opens at the app's own size, so nothing is asked of it until first-run setup shows.
            window: Some(WindowSize::App),
            // Folders that are never there, so a preview or a test never writes a folder or reaches the network.
            folder_update: folder_update::idle_folder_update(env!("CARGO_PKG_VERSION")),
            notice_screen: NoticeScreen::default(),
        }
    }

    /// A shell with fixed data, for tests and previews.
    pub fn with_snapshot(screen: Screen, workspace: WorkspaceRoot, snapshot: Snapshot) -> Self {
        let today = snapshot.today;
        let (profile_editor, portal_changes) = (Some(profile::open_editor(workspace.path())), Some(portals::open_changes(workspace.path())));
        Self { workspace: Some(workspace), snapshot, loaded: true, tray: Some(InTray::unsaved()), profile_editor, portal_changes, ..Self::empty(screen, today) }
    }

    /// A shell whose folder is open but not read yet, for tests and previews.
    pub fn loading(screen: Screen, workspace: WorkspaceRoot, today: NaiveDate) -> Self {
        let (profile_editor, portal_changes) = (Some(profile::open_editor(workspace.path())), Some(portals::open_changes(workspace.path())));
        Self { workspace: Some(workspace), tray: Some(InTray::unsaved()), profile_editor, portal_changes, ..Self::empty(screen, today) }
    }

    /// Switches portals and moves them to the Trash through `changes`, such as a fake store, for tests.
    pub fn with_portal_changes(self, changes: PortalChanges) -> Self {
        Self { portal_changes: Some(changes), ..self }
    }

    /// Reads the community list through `community`, such as a scripted one, for tests and snapshots. The installed
    /// portals it holds read Community at once.
    pub fn with_community(self, community: Community) -> Self {
        let mut shell = Self { community, ..self };
        shell.name_community_portals();
        shell
    }

    /// Names the installed portals that came from the community list, with their country, on the snapshot shown.
    fn name_community_portals(&mut self) {
        let hints = self.community.installed_hints();
        self.snapshot.portals.set_community(&hints);
    }

    /// Now in UTC, as the community list keeps its times: the system's clock, or a test's clock read as local time. The
    /// wall clock is never turned back into UTC, because in the hour repeated when clocks go back it would read an hour
    /// early, and a list just read would look as if it came from the future.
    fn now_utc(&self) -> DateTime<Utc> {
        match self.now {
            Some(now) => Local.from_local_datetime(&now).earliest().map_or_else(|| now.and_utc(), |local| local.to_utc()),
            None => Utc::now(),
        }
    }

    /// The first run: no folder is open yet.
    pub fn without_workspace(today: NaiveDate) -> Self {
        Self::empty(Screen::InTray, today)
    }

    /// Shows first-run setup through `onboarding` in place of the welcome, in the small first window. Paths under
    /// `home` show from it.
    pub fn with_first_run(self, onboarding: Onboarding, home: PathBuf) -> Self {
        Self { first_run: Some(FirstRun { onboarding, screen: FirstRunScreen::new(home) }), window: None, ..self }
    }

    /// First-run setup while it shows, for tests that act as its folder picker would.
    pub fn first_run_mut(&mut self) -> Option<&mut Onboarding> {
        self.first_run.as_mut().map(|first| &mut first.onboarding)
    }

    /// Whether first-run setup's worker is still ending after setup handed over, for tests.
    pub fn setup_stopping(&self) -> bool {
        self.setup_stopping.is_some()
    }

    /// Opens `path` as Choose folder and Settings › Change folder do once the picker gives it, for tests that act as
    /// the picker would.
    pub fn choose_folder_from(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.choose_folder_with(ctx, || Some(path));
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

    /// The preferences, for tests that set one as Settings would.
    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// Shows `label` as the folder in Settings, for snapshots.
    pub fn with_folder_label(self, label: &str) -> Self {
        Self { folder_label: Some(label.to_owned()), ..self }
    }

    /// Shows `label` as the data folder in Settings, holding `sizes`, for snapshots.
    pub fn with_data_label(self, label: &str, sizes: DataSizes) -> Self {
        Self { data_label: Some(label.to_owned()), data_sizes: Some(sizes), ..self }
    }

    /// Shows `label` as this copy's version in Settings, for snapshots. The updates count it as the version in use, with
    /// a check that never answers: give scripted ones with `with_updates` after this.
    pub fn with_version_label(self, label: &str) -> Self {
        let updates = updates::idle_updates(label, true);
        Self { version: label.to_owned(), ..self }.with_updates(updates)
    }

    /// Takes `updates`, such as scripted ones, for tests and snapshots. They follow the switch as the settings hold it.
    pub fn with_updates(self, mut updates: Updates) -> Self {
        let auto = self.settings.preferences().check_updates.is_on();
        updates.set_auto(auto);
        Self { updates, auto_check: auto, ..self }
    }

    /// Signs tasks in through `providers`, such as a key kept in memory and a scripted Test connection, for tests.
    pub fn with_providers(self, providers: Providers) -> Self {
        Self { providers, ..self }
    }

    /// Takes `folder_update`, such as an idle one showing a stage, for tests and snapshots. It keeps what it shows, and
    /// updates no folder until one opens.
    pub fn with_folder_update(self, folder_update: FolderUpdate) -> Self {
        Self { folder_update, ..self }
    }

    /// The folder update, for tests that swap it as a worker's end would.
    pub fn folder_update_slot(&mut self) -> &mut FolderUpdate {
        &mut self.folder_update
    }

    /// Reads `now` as the time for every age Providers and Profile show, so snapshots never move with the wall clock.
    pub fn with_now(self, now: NaiveDateTime) -> Self {
        Self { now: Some(now), ..self }
    }

    /// The time ages are counted to.
    fn now(&self) -> NaiveDateTime {
        self.now.unwrap_or_else(|| Local::now().naive_local())
    }

    /// Measures what the data folder holds, if there is one. A shell without one keeps what it was given.
    fn measure_data(&mut self) {
        if let Some(data) = &self.data {
            self.data_sizes = Some(settings::measure_data(data));
        }
    }

    /// Hands the shell snapshots as the folder watcher does, for tests.
    pub fn snapshot_feed(&mut self) -> std::sync::mpsc::Sender<Snapshot> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.snapshots = Some(receiver);
        sender
    }

    /// Takes the reload requests the shell would send to the snapshot worker, for tests.
    pub fn reload_feed(&mut self) -> Receiver<()> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.reload = Some(Reload::new(sender));
        receiver
    }

    /// Renders PDF pages, for Profile's CV documents and the dossier, through `pages`, for tests.
    pub fn with_page_store(self, pages: PageStore) -> Self {
        Self { pages: Some(pages), ..self }
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
    /// The system keychain is reached from here alone, and nothing reads it until Providers shows, a test runs or a task starts on the key.
    /// The updates read GitHub from here alone, and only while the switch is on or the user asks. What Restart to update
    /// applies goes into `restart_slot` as the window closes, with `args` as the new copy's.
    fn start(ctx: &egui::Context, path: Option<PathBuf>, data: Option<PathBuf>, restart_slot: RestartSlot, args: Vec<OsString>) -> Self {
        let repaint = ctx.clone();
        let environment = workspace::environment_checks(Arc::new(move || repaint.request_repaint()));
        let repaint = ctx.clone();
        let providers = settings::open_providers(settings::system_keychain(), settings::claude_tester(Arc::new(move || repaint.request_repaint())));
        let repaint = ctx.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || repaint.request_repaint());
        let folder = data.clone();
        let make_updates = move |auto| match folder {
            Some(folder) => updates::github(&folder, env!("CARGO_PKG_VERSION"), updates::install_block(), auto, wake),
            None => updates::idle_updates(env!("CARGO_PKG_VERSION"), auto),
        };
        let repaint = ctx.clone();
        let make_community = move |data: Option<&Path>| portals::community(data, env!("CARGO_PKG_VERSION"), Arc::new(move || repaint.request_repaint()));
        let repaint = ctx.clone();
        // The release downloads into the data folder, so a launch with none updates no folder.
        let folder_update = match &data {
            Some(data) => folder_update::folder_update(data, env!("CARGO_PKG_VERSION"), Arc::new(move || repaint.request_repaint())),
            None => folder_update::idle_folder_update(env!("CARGO_PKG_VERSION")),
        };
        // Only a launch with no folder asked for or remembered may show first-run setup.
        let first_run = if path.is_none() { data.as_deref().and_then(|data| first_run(ctx, data)) } else { None };
        let launched = Self::launch(ctx, path, data, environment, providers, make_updates, make_community, folder_update);
        let shell = Self { restart_slot, launch_args: args, ..launched };
        match first_run {
            Some((onboarding, home)) => shell.with_first_run(onboarding, home),
            None => shell,
        }
    }

    /// `start` with the tool check, the providers, the updates and the community list given, so tests can launch without
    /// asking real tools, the keychain or the network. `make_updates` takes the switch for the automatic check as saved.
    /// `make_community` takes the data folder, where the list read last is saved, and is asked with none too.
    /// `folder_update` updates the folder that opens, and every folder opened after it.
    /// The first launch with no settings file keeps the theme and zoom eframe restored, and saves them.
    #[allow(clippy::too_many_arguments)]
    fn launch(
        ctx: &egui::Context,
        path: Option<PathBuf>,
        data: Option<PathBuf>,
        mut environment: EnvironmentChecks,
        providers: Providers,
        make_updates: impl FnOnce(bool) -> Updates,
        make_community: impl FnOnce(Option<&Path>) -> Community,
        folder_update: FolderUpdate,
    ) -> Self {
        environment.ensure_started();
        let restored = settings::ui::restored(ctx);
        let settings = data.as_deref().map_or_else(Settings::unsaved, |data| settings::open_settings(data, restored));
        settings::ui::apply(ctx, settings.preferences());
        // Before any desk reads the logs, so a deleted one never shows in Tasks or counts in Usage.
        if let Some(data) = &data {
            prune_run_logs(data, settings.preferences().retention, Local::now().naive_local());
        }
        let auto_check = settings.preferences().check_updates.is_on();
        let mut updates = make_updates(auto_check);
        // What a past session downloaded is never applied by this one. A Restart to update that did not install says why.
        if let Some(data) = &data {
            updates::clear_downloads(data);
            if let Some(failed) = updates::last_apply_failure(data) {
                updates.apply_failed(failed);
            }
        }
        let first = start_screen(settings.preferences().open_on);
        let permissions = settings.preferences().permissions;
        // With no data folder too, so Browse reads the list rather than wait on one that never comes.
        let community = make_community(data.as_deref());
        let mut shell = Self {
            community,
            data,
            environment: Some(environment),
            settings,
            permissions,
            providers,
            updates,
            auto_check,
            folder_update,
            ..Self::empty(first, Local::now().date_naive())
        };
        // After the pruning, so the size is what stays.
        shell.measure_data();
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

    /// A running task keeps its folder: a new one would replace the desk and drop the task's live session. A folder
    /// update keeps it too, so its notice and Undo stay with the folder it changed, and so does another Jobdesk's that
    /// holds the folder alone. A folder Jobdesk cannot lock keeps nothing: this Jobdesk holds nothing there, and another
    /// folder is the way out.
    fn folder_locked(&self) -> bool {
        self.desk.as_ref().is_some_and(RunDesk::has_live_process) || self.folder_update.folder_wait() == Some(FolderWait::Updating)
    }

    fn open(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        if self.folder_locked() {
            return;
        }
        match workspace::open(path) {
            Some(root) => {
                let repaint = ctx.clone();
                let usage = self.data.as_ref().map(|data| runs::usage_logs(data.join("runs"), root.path()));
                let (snapshots, reload) = live_snapshot::spawn(root.clone(), usage, move || repaint.request_repaint());
                (self.snapshots, self.reload) = (Some(snapshots), Some(reload));
                // The new worker numbers its own asks, and reads every log first.
                self.usage_due = 0;
                if let Some(data) = &self.data {
                    let repaint = ctx.clone();
                    // Each task's process gets the key through this as it starts: the one held in memory, or the keychain's on the first read.
                    let mut desk = runs::open_desk(root.path(), data.join("runs"), Arc::new(move || repaint.request_repaint()), self.providers.key_reader());
                    self.permissions = self.settings.preferences().permissions;
                    desk.set_asks_permission(self.permissions.asks());
                    // A new desk starts on the Claude sign-in, so it takes the method in use, whatever the last desk had.
                    self.sign_in = self.providers.task_sign_in(self.settings.preferences());
                    desk.set_sign_in(self.sign_in);
                    self.desk = Some(desk);
                }
                self.tray = Some(self.data.as_ref().map_or_else(InTray::unsaved, |data| intray::open_tray(data.join("intray"), root.path())));
                let repaint = ctx.clone();
                self.pages = Some(dossier::page_store(Arc::new(move || repaint.request_repaint())));
                self.open_dossier = None;
                // The last folder's marks, picked topic and edit mean nothing here.
                (self.marks, self.profile_screen, self.was_busy, self.run_ended) = (UpdatedMarks::default(), ProfileScreen::default(), false, None);
                (self.profile_editor, self.leave_to, self.profile_saved, self.away) = (Some(profile::open_editor(root.path())), None, None, None);
                (self.portals_screen, self.portal_changes, self.portals_changed) = (PortalsScreen::default(), Some(portals::open_changes(root.path())), None);
                // The launcher no longer leads back into the last folder's screens.
                self.launcher_from = self.screen.tab();
                // A task in plain words was written for the last folder.
                self.launcher.clear_words();
                // The last folder's data stays off screen while this one is read.
                self.snapshot = Snapshot { today: Local::now().date_naive(), ..Snapshot::default() };
                self.loaded = false;
                // Its rollback is due before anything else writes it, and starts on the next frame. The last folder's
                // notice goes.
                self.folder_update.opened(root.path().to_owned());
                self.notice_screen.close_details();
                ctx.request_repaint();
                self.workspace = Some(root);
                self.folder_error = None;
                // However the folder opened, first-run setup has nothing left to do.
                self.leave_first_run();
            }
            None => self.folder_error = Some(format!("{} is not an ai-job-search checkout.", path.display())),
        }
    }

    fn choose_folder(&mut self, ctx: &egui::Context) {
        self.choose_folder_with(ctx, || rfd::FileDialog::new().set_title("Choose your ai-job-search folder").pick_folder());
    }

    /// The dialog opens only when the folder may change. Unsaved profile text asks first, once a folder is picked.
    fn choose_folder_with(&mut self, ctx: &egui::Context, pick: impl FnOnce() -> Option<PathBuf>) {
        if self.folder_locked() {
            return;
        }
        match pick() {
            Some(path) if self.draft_unsaved() => {
                self.profile_screen.put_away();
                self.away = Some((Pending::Folder(path), true));
            }
            Some(path) => self.open(ctx, &path),
            None => {}
        }
    }

    /// Whether a profile edit holds text not saved yet.
    fn draft_unsaved(&self) -> bool {
        self.profile_editor.as_ref().is_some_and(ProfileEditor::is_dirty)
    }

    /// Asks, over whatever is on show, before a folder change or quitting drops unsaved profile text. Discard goes on
    /// with what waited. A folder that fails to open leaves the draft as it was.
    fn away_ask(&mut self, ctx: &egui::Context) {
        let Some((pending, focus)) = self.away.as_mut() else { return };
        let away = match pending {
            Pending::Folder(_) => Away::Folder,
            Pending::Quit => Away::Quit,
        };
        if let Some(discard) = self.profile_screen.ask_away(ctx, away, focus) {
            self.settle_away(ctx, discard);
        }
    }

    /// Closes the ask over every screen with the user's answer. A close asked for while it showed comes back after it.
    fn settle_away(&mut self, ctx: &egui::Context, discard: bool) {
        let Some((pending, _)) = self.away.take() else { return };
        // Keep editing keeps the window, so a Restart to update that waited on the answer is off.
        if !discard && pending == Pending::Quit {
            self.restart_pending = false;
        }
        if discard {
            self.answer_away(ctx, pending);
        }
        if std::mem::take(&mut self.quit_after) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Goes on with what waited on the ask, once the user agreed to drop their draft. Quitting keeps the draft until the
    /// window closes: a live session may still ask, and Keep working there keeps everything.
    fn answer_away(&mut self, ctx: &egui::Context, pending: Pending) {
        match pending {
            Pending::Folder(path) => self.open(ctx, &path),
            Pending::Quit => {
                self.quit_agreed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// While the folder update tries something again, the hold or a waiting rollback, it does so even with no input.
    fn ask_retry_repaint(&self, ctx: &egui::Context) {
        if self.folder_update.retrying() {
            ctx.request_repaint_after(HOLD_RETRY);
        }
    }

    /// Why Change folder waits, if it does.
    fn folder_wait(&self) -> Option<&'static str> {
        match self.folder_update.folder_wait() {
            Some(FolderWait::Updating) => Some(FOLDER_WAITS),
            // This Jobdesk holds nothing in a folder it cannot lock, and another folder is the way out.
            Some(FolderWait::Unlockable) | None => self.folder_locked().then_some(FOLDER_BUSY),
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        self.follow_first_run(ui.ctx());
        self.take_snapshot();
        self.follow_permissions();
        // A test that ended may move tasks onto the key or off it, so the desk follows on the same frame.
        let now = self.now();
        if self.providers.poll(&mut self.settings, now) {
            ui.ctx().request_repaint();
        }
        self.follow_sign_in();
        // One Claude process at a time: while a test runs, the desk starts none of its own.
        let testing = self.providers.test_gate(false) == TestGate::Testing;
        if let Some(desk) = &mut self.desk {
            desk.set_testing(testing);
            desk.poll();
        }
        self.follow_runs();
        self.settle_run_end(ui.ctx());
        if let Some(pages) = &mut self.pages {
            pages.poll();
        }
        if let Some(environment) = &mut self.environment {
            environment.poll();
        }
        self.follow_updates(ui.ctx(), now);
        // After the desk's news and the updates', and before any screen can start a task.
        self.follow_folder_update();
        self.ask_retry_repaint(ui.ctx());
        if self.community.poll(self.now_utc()) {
            self.name_community_portals();
        }
        // Cmd + and Cmd − move the zoom on any screen, and the text size follows.
        settings::ui::follow_zoom(ui.ctx(), &mut self.settings);
        self.handle_shortcuts_and_quit(ui.ctx());
        // However Settings closed, Back, Cmd K or a link out, what it was asking closes too, before Cmd , can reopen it.
        if !self.settings_open {
            self.settings_screen.closed();
        }
        // Drawn before the screens, so it takes Esc before any of them.
        self.away_ask(&ui.ctx().clone());
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
                } else if self.first_run.is_some() {
                    self.first_run_screen(ui);
                } else {
                    self.welcome(ui);
                }
            });
        });
        // A line the launcher refuses opens nothing.
        if let Some(line) = take_command_request(ui.ctx())
            && self.launcher.prefill(&line)
        {
            self.open_launcher();
        }
        if self.confirm_quit {
            self.quit_dialog(ui.ctx());
        }
        // Last, so Settings opened or closed anywhere this frame sizes the window on it.
        self.follow_window(ui.ctx());
        note_focus(ui.ctx());
    }

    /// Asks the window for the size what shows needs, once each time that changes: the small first window for
    /// first-run setup, and the app's own for Settings over it, which needs the app's smallest, and once setup hands
    /// over.
    fn follow_window(&mut self, ctx: &egui::Context) {
        let size = if self.first_run.is_some() && !self.settings_open { WindowSize::FirstRun } else { WindowSize::App };
        if self.window != Some(size) {
            self.window = Some(size);
            resize(ctx, size);
        }
    }

    /// Takes what first-run setup's worker said. Once the Folder and Computer steps are done, the folder opens as the
    /// job search.
    fn follow_first_run(&mut self, ctx: &egui::Context) {
        self.join_stopped_setup(ctx);
        let Some(first) = &mut self.first_run else { return };
        first.onboarding.poll(self.snapshot.today);
        if let SetupStage::Finished { folder } = first.onboarding.stage() {
            let folder = folder.clone();
            self.open(ctx, &folder);
            // A folder that failed to open leaves setup too, and shows on the welcome.
            self.leave_first_run();
        }
    }

    /// Joins first-run setup's worker once it has finished after setup handed over, and starts the tool check again,
    /// since an install past its last cancel check may have put a helper in place after the hand-over's check. Until
    /// then the window looks again a few times a second, as the worker's last news can wake it just before the thread
    /// ends.
    fn join_stopped_setup(&mut self, ctx: &egui::Context) {
        match self.setup_stopping.take() {
            Some(worker) if worker.is_finished() => {
                // A worker that panicked has nothing left to clean up, and the window goes on.
                let _ = worker.join();
                if let Some(environment) = &mut self.environment {
                    environment.restart();
                }
            }
            Some(worker) => {
                self.setup_stopping = Some(worker);
                ctx.request_repaint_after(Duration::from_millis(250));
            }
            None => {}
        }
    }

    /// First-run setup hands over to the app at once, whichever way a folder opened: its worker is told to stop and
    /// left to end on its own, the window grows to the app's own with Today on show, and the tool check starts again,
    /// so what setup installed is found. Nothing while setup does not show.
    fn leave_first_run(&mut self) {
        let Some(mut first) = self.first_run.take() else { return };
        self.setup_stopping = first.onboarding.stop();
        (self.screen, self.launcher_from) = (Screen::InTray, Screen::InTray);
        if let Some(environment) = &mut self.environment {
            environment.restart();
        }
    }

    /// First-run setup's screen for where it stands, then what the user asked for there.
    fn first_run_screen(&mut self, ui: &mut Ui) {
        let (today, refused) = (self.snapshot.today, self.folder_error.is_some());
        let Some(first) = &mut self.first_run else { return };
        let Some(request) = first.screen.show(ui, &first.onboarding, today, refused) else { return };
        let onboarding = &mut first.onboarding;
        match request {
            // A folder that opens leaves setup.
            FirstRunRequest::HaveFolder => self.choose_folder(&ui.ctx().clone()),
            FirstRunRequest::ChangeFolder => {
                if let Some(folder) = rfd::FileDialog::new().set_title("Choose where your job search lives").pick_folder() {
                    onboarding.choose(folder);
                }
            }
            FirstRunRequest::StartFromScratch => onboarding.start_from_scratch(),
            FirstRunRequest::Resume => onboarding.resume(),
            FirstRunRequest::Back => onboarding.back(),
            FirstRunRequest::UseSuggestion => onboarding.use_suggestion(),
            FirstRunRequest::SaveHere => onboarding.save_here(today),
            FirstRunRequest::Cancel => onboarding.cancel(),
            FirstRunRequest::GoOn => onboarding.go_on(),
            FirstRunRequest::Install => onboarding.install(),
            FirstRunRequest::Finish => onboarding.finish(false),
            FirstRunRequest::WithoutPdfs => onboarding.finish(true),
        }
        // The worker may already have news, and a finished setup opens its folder at once.
        ui.ctx().request_repaint();
    }

    /// Hands a changed switch for the automatic check to the updates, then takes what they heard and starts a check that
    /// is due. The window wakes when the next one is.
    fn follow_updates(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        let auto = self.settings.preferences().check_updates.is_on();
        if auto != self.auto_check {
            self.auto_check = auto;
            self.updates.set_auto(auto);
        }
        self.updates.poll(now);
        self.drop_gone_restart(ctx);
        if let Some(wait) = self.updates.next_due(now) {
            ctx.request_repaint_after(wait);
        }
    }

    /// A release withdrawn while Restart to update waits takes its download with it, so the restart is off, and the window
    /// stays rather than quit a Jobdesk that would not open again. The draft ask it raised closes. A close already on its
    /// way stops here, without asking again: the one Restart to update sent, which lands as a close request the frame after,
    /// or the one Discard sent, which agreed to the quit. Runs before `handle_shortcuts_and_quit` reads the request.
    fn drop_gone_restart(&mut self, ctx: &egui::Context) {
        if !self.restart_pending || self.updates.restart_plan().is_some() {
            return;
        }
        self.restart_pending = false;
        if std::mem::take(&mut self.quit_agreed) || ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dropped = true;
        }
        if matches!(self.away, Some((Pending::Quit, _))) {
            self.away = None;
        }
    }

    /// Who else writes the job search folder now, if anyone: a task that works, or the folder update or its Undo, this
    /// Jobdesk's or another's that holds the folder alone. Every other writer of the folder waits on it: a new task and
    /// a reply, through the desk, Profile's Save, and Portals' switch and Remove. The folder update waits the other way
    /// round in `folder_writable`. Profile and Portals write on the window's thread within one frame, so none is under
    /// way when `follow_folder_update` starts the update.
    fn folder_writer(&self) -> Option<FolderWriter> {
        if self.desk.as_ref().is_some_and(RunDesk::is_busy) {
            Some(FolderWriter::Task)
        } else {
            self.folder_update.folder_wait().map(FolderWriter::FolderUpdate)
        }
    }

    /// Why Portals' switches and actions wait, in its words, if they do.
    fn portals_wait(&self) -> Option<&'static str> {
        self.folder_writer().map(|writer| match writer {
            FolderWriter::Task => portals::ui::BUSY,
            FolderWriter::FolderUpdate(FolderWait::Updating) => portals::ui::FOLDER_UPDATING,
            FolderWriter::FolderUpdate(FolderWait::Unlockable) => UNLOCKABLE,
        })
    }

    /// Whether the job search folder may be written by a folder update now: no task works and no Claude session is
    /// open, even one only waiting on a reply, as Restart to update waits for, first-run setup has handed over and its
    /// worker has ended, and the window is not closing for Restart to update.
    fn folder_writable(&self) -> bool {
        self.session_state() == SessionState::None && self.first_run.is_none() && self.setup_stopping.is_none() && !self.restart_pending
    }

    /// Takes the folder update's news, then starts the open folder's rollback once after it opened, or an update once
    /// for each answer of the feed, while the folder may be written. While one works, here or in another Jobdesk that
    /// holds the folder alone, the desk starts no task and sends no turn, set before any screen draws.
    fn follow_folder_update(&mut self) {
        let writable = self.folder_writable();
        let feed = self.updates.latest_feed().map(|feed| (self.updates.feed_answers(), feed));
        self.folder_update.follow(feed, writable);
        self.share_folder_wait();
    }

    /// Hands the folder update's wait, if any, to the desk and to Test connection, so neither starts a Claude process
    /// meanwhile, and each says why.
    fn share_folder_wait(&mut self) {
        let wait = self.folder_update.folder_wait();
        if let Some(desk) = &mut self.desk {
            desk.set_folder_wait(wait);
        }
        self.providers.set_folder_wait(wait);
    }

    /// What a Claude session open in Tasks means for Restart to update: one that waits on a permission or a question, one
    /// working, one that only waits for a reply, or none. A run that asks the user still counts as working, so it is named first.
    fn session_state(&self) -> SessionState {
        match &self.desk {
            Some(desk) if desk.runs().iter().any(|run| run.needs_user()) => SessionState::Asking,
            Some(desk) if desk.is_busy() => SessionState::Working,
            Some(desk) if desk.has_live_process() => SessionState::Open,
            _ => SessionState::None,
        }
    }

    /// Why Restart to update waits, if it does: a Claude session is open, or the job search folder is being updated.
    fn restart_wait(&self) -> Option<&'static str> {
        // This Jobdesk holds nothing in a folder it cannot lock, so a fixed Jobdesk may apply.
        restart_wait(self.session_state()).or_else(|| (self.folder_update.folder_wait() == Some(FolderWait::Updating)).then_some(RESTART_WAITS))
    }

    /// Restart to update goes through the normal quit, so an unsaved profile draft still asks first. It never starts while
    /// a Claude session is open, and applies nothing without a download that is ready.
    fn restart_to_update(&mut self, ctx: &egui::Context) {
        if self.restart_wait().is_some() || self.updates.restart_plan().is_none() {
            return;
        }
        self.restart_pending = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Does what the update group, its notes or About asked. The check and the download are not runs: no Claude process,
    /// no run log, nothing counted in Usage.
    fn follow_update(&mut self, ctx: &egui::Context, request: UpdateRequest) {
        match request {
            UpdateRequest::Act(BarAction::Download | BarAction::Retry) => self.updates.download(),
            UpdateRequest::Act(BarAction::Cancel) => self.updates.cancel(),
            UpdateRequest::Act(BarAction::Restart) => self.restart_to_update(ctx),
            // Built from the version tag, never read from the feed or the notes.
            UpdateRequest::OpenPage(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        }
    }

    /// What Restart to update applies, for `run` once the window has closed, with the folder open now. Only a restart the
    /// user chose, with no session open, leaves one.
    fn leave_restart(&mut self) {
        if !self.restart_pending || self.folder_locked() {
            return;
        }
        let Some(plan) = self.updates.restart_plan() else { return };
        let args = relaunch_args(&self.launch_args, self.workspace.as_ref().map(WorkspaceRoot::path));
        if let Ok(mut slot) = self.restart_slot.lock() {
            *slot = Some((plan, args));
        }
    }

    /// Hands a changed Permissions setting to the desk, before it polls the prompts that wait. An unchanged one is left alone,
    /// so a desk set up some other way, as previews do, keeps its own.
    fn follow_permissions(&mut self) {
        let permissions = self.settings.preferences().permissions;
        if permissions == self.permissions {
            return;
        }
        self.permissions = permissions;
        if let Some(desk) = &mut self.desk {
            desk.set_asks_permission(permissions.asks());
        }
    }

    /// Hands a changed sign-in to the desk, for the next process it starts. An unchanged one is left alone, as
    /// `follow_permissions` leaves it, so a desk set up some other way keeps its own.
    fn follow_sign_in(&mut self) {
        let sign_in = self.providers.task_sign_in(self.settings.preferences());
        if sign_in == self.sign_in {
            return;
        }
        self.sign_in = sign_in;
        if let Some(desk) = &mut self.desk {
            desk.set_sign_in(sign_in);
        }
    }

    /// Takes the newest snapshot the worker sent, if any.
    fn take_snapshot(&mut self) {
        let Some(mut latest) = self.snapshots.as_ref().and_then(|snapshots| snapshots.try_iter().last()) else { return };
        // A snapshot read before the last read back would show the saved block as it was, so the profile read back stays.
        match self.profile_saved {
            Some(saved) if latest.read_at.is_some_and(|read| read < saved) => latest.profile = std::mem::take(&mut self.snapshot.profile),
            _ => self.profile_saved = None,
        }
        // The same for the portals read back after a switch or a removal.
        match self.portals_changed {
            Some(changed) if latest.read_at.is_some_and(|read| read < changed) => {
                latest.portals = std::mem::take(&mut self.snapshot.portals);
                latest.workspace.set_portals(&latest.portals);
                latest.issues.retain(|issue| issue.source != Source::Portals);
                latest.issues.extend(self.snapshot.issues.iter().filter(|issue| issue.source == Source::Portals).cloned());
            }
            _ => self.portals_changed = None,
        }
        // The first snapshot read after a run ended holds what the run wrote, so the marks compare it with the start.
        if let Some(ended) = self.run_ended
            && latest.read_at.is_some_and(|read| read >= ended)
        {
            self.marks.finish(&latest.profile, Local::now().naive_local());
            self.run_ended = None;
        }
        // A run that started before the folder was first read keeps the first snapshot as its start.
        let first = !self.loaded;
        self.snapshot = latest;
        self.name_community_portals();
        self.loaded = true;
        if first && self.was_busy && self.run_ended.is_none() {
            self.marks.begin(&self.snapshot.profile);
        }
        if let Some(open) = &mut self.open_dossier {
            open.view = None;
        }
        // A PDF read while a task was still writing it failed to render, and may be whole now with the same modification time.
        if let Some(pages) = &mut self.pages {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            pages.retry_failures(now);
        }
    }

    /// Whether Save waits for the first load after a run ended, so a Save is never marked as the run's.
    fn save_waits_for_load(&self) -> bool {
        self.run_ended.is_some() && self.snapshots.is_some()
    }

    /// Stops the wait for a load after a run once it has lasted `RUN_READ_WAIT`. Jobdesk then reads the profile itself
    /// and marks the run's changes from that read, so a Save made after it, which a late snapshot would hold, is not marked.
    fn settle_run_end(&mut self, ctx: &egui::Context) {
        let Some(ended) = self.run_ended else { return };
        let waited = ended.elapsed();
        if waited < RUN_READ_WAIT {
            // No input may come, so a frame is asked for when the wait runs out.
            ctx.request_repaint_after(RUN_READ_WAIT - waited);
            return;
        }
        self.read_back_profile();
        self.marks.finish(&self.snapshot.profile, Local::now().naive_local());
        self.run_ended = None;
    }

    /// A run's start keeps the profile as it stands. Its end asks for a fresh load, and the first snapshot read after the
    /// end marks the topics the run changed. The snapshot on show at the end may predate the run's last writes.
    /// A turn or run that starts before that load arrives keeps the kept profile, so the next end marks the changes of both:
    /// the snapshot on show may already hold the first one's writes.
    fn follow_runs(&mut self) {
        let busy = self.desk.as_ref().is_some_and(RunDesk::is_busy);
        if busy && !self.was_busy {
            // The task may build on what a folder update wrote, so that update is not offered for Undo any more. An Undo
            // that did not finish stays offered: finishing it keeps the task's edits.
            self.folder_update.task_started();
            // Before the first read the snapshot is empty, so the start waits for it.
            if self.run_ended.take().is_none() && self.loaded {
                self.marks.begin(&self.snapshot.profile);
            }
        } else if !busy && self.was_busy {
            self.run_ended = Some(Instant::now());
            // The run that ended left its cost in its log. A closed worker has nothing to reload.
            if let Some(reload) = &self.reload {
                self.usage_due = reload.ask();
            }
            // And its log grew, so Data and privacy measures again.
            self.measure_data();
        }
        self.was_busy = busy;
    }

    /// Runs change between snapshots, so the queue is dealt again each frame.
    fn queue(&mut self) -> Queue {
        let Some(tray) = &mut self.tray else { return Queue::default() };
        if self.screen != Screen::InTray {
            tray.stop_reading();
        }
        // Coming up, Later and the skip reasons are the user's, so the tray takes them before it deals.
        tray.set_rules(&self.settings.preferences().queue);
        let snapshot = &self.snapshot;
        let runs = self.desk.as_ref().map_or(&[][..], RunDesk::runs);
        tray.queue(&Sources { board: &snapshot.board, pipeline: &snapshot.pipeline, archives: &snapshot.archives, runs, today: snapshot.today })
    }

    fn follow(&mut self, ctx: &egui::Context, request: Request) {
        match request {
            Request::Command(line) => {
                // A line the launcher refuses opens nothing.
                if self.launcher.prefill(&line) {
                    self.open_launcher();
                }
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

    /// Opens the launcher with no line, as New task does: a task in plain words left from before is dropped,
    /// so Cmd Enter on the fresh form never repeats it.
    fn new_task(&mut self) {
        self.launcher.clear_words();
        self.open_launcher();
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
                (self.confirm_quit, self.quit_agreed, self.restart_pending) = (false, false, false);
            }
        }
        let active = self.desk.as_ref().is_some_and(RunDesk::has_live_process);
        // A close a withdrawn release already stopped is not a quit to ask about.
        let dropped = std::mem::take(&mut self.close_dropped);
        let closing = ctx.input(|input| input.viewport().close_requested()) && !dropped;
        // Unsaved profile text asks first, one ask at a time. Once the user agrees, the window asks to close again, and a
        // live session asks next. A close while the folder ask shows waits for its answer.
        if closing && self.draft_unsaved() && !self.quit_agreed {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            match self.away {
                None => {
                    self.profile_screen.put_away();
                    self.away = Some((Pending::Quit, true));
                }
                Some((Pending::Folder(_), _)) => self.quit_after = true,
                Some((Pending::Quit, _)) => {}
            }
        } else if closing && active {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_focus |= !self.confirm_quit;
            self.confirm_quit = true;
        }
        // Taken before any widget runs, even while typing: a text field reads Ctrl K as delete to the end of the line.
        // Cmd , is taken the same way, so Settings opens from a field too.
        let (ask, open_settings) =
            ctx.input_mut(|input| (input.consume_key(egui::Modifiers::COMMAND, egui::Key::K), input.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)));
        // While the quit dialog or the ask over every screen shows, no shortcut reaches the screens behind it.
        if self.confirm_quit || self.away.is_some() {
            note_dialog(ctx);
            return;
        }
        // The release notes close on Esc before anything behind them takes it, Settings' Back to app included.
        if self.update_screen.notes_open() && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.update_screen.close_notes();
        }
        // The folder update's details too.
        if self.notice_screen.details_open() && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.notice_screen.close_details();
        }
        // Settings opens on the page last open, with or without a folder.
        if open_settings {
            self.settings_open = true;
        }
        if ask && self.workspace.is_some() {
            self.new_task();
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
                    (self.confirm_quit, self.quit_agreed, self.restart_pending) = (false, false, false);
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
        // First-run setup has no menu: nothing else is usable yet.
        if self.first_run.is_some() && self.workspace.is_none() {
            ui.allocate_ui_with_layout(vec2(ui.available_width(), NAV_TAB_HEIGHT), Layout::left_to_right(Align::Center), brand);
            return;
        }
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
                    self.new_task();
                }
            });
        });
        if let Some(screen) = picked {
            self.leave_for(screen);
        }
    }

    /// Shows `screen`. Leaving Profile with unsaved text asks first, and the screen changes only once the user drops it.
    fn leave_for(&mut self, screen: Screen) {
        let asks = self.screen == Screen::Profile
            && screen != Screen::Profile
            && self.profile_editor.as_mut().is_some_and(|editor| self.profile_screen.ask_to_leave(editor));
        if asks {
            self.leave_to = Some(screen);
        } else {
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

    /// What the status bar says about the screen on show. Settings › Providers and Runtime status say how tasks sign in,
    /// so the bar no longer does.
    fn screen_note(&self) -> Option<String> {
        match self.screen {
            Screen::Dossier if !self.settings_open => Some("Source: tracker row and application archive".to_owned()),
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
                // Before a folder is open, the line also says how tasks sign in: Claude Code's own sign-in, or a key saved in Settings.
                // First-run setup's screens leave the key out: the check finds Claude Code installed, not signed in.
                let text = match (ready, self.workspace.is_some(), self.first_run.is_some()) {
                    (true, true, _) => "Claude is ready",
                    (true, false, true) => "Claude Code is installed. Jobdesk uses it for every task.",
                    (true, false, false) => "Claude is installed. Tasks use its sign-in, or an API key you save in Settings.",
                    (false, _, _) => "Claude Code is not installed",
                };
                status_line(ui, ready, text);
            }
            if let Some(note) = self.screen_note() {
                status_note(ui, &note, false);
            }
            // The right is Jobdesk's own: Settings, then the update group while there is one, its words cut first, then
            // the folder update's notice while there is one, its words cut before the update group's.
            let view = self.updates.view();
            let wait = self.restart_wait();
            let notice = self.folder_update.notice();
            let (asked, noticed) = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if status_button(ui, "Settings", &SETTINGS_KEYS, self.settings_open).clicked() {
                        self.settings_open = true;
                    }
                    let asked = updates::ui::bar_group(ui, &mut self.update_screen, &view, wait);
                    (asked, folder_update::ui::bar_group(ui, &mut self.notice_screen, notice.as_ref()))
                })
                .inner;
            if let Some(request) = asked {
                self.follow_update(ui.ctx(), request);
            }
            match noticed {
                // Undo writes the folder too, so it waits as an update does.
                Some(NoticeRequest::Undo) => {
                    let writable = self.folder_writable();
                    self.folder_update.undo_update(writable);
                    self.share_folder_wait();
                }
                Some(NoticeRequest::Dismiss) => self.folder_update.dismiss(),
                None => {}
            }
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
        let (now, now_utc) = (self.now(), self.now_utc());
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
            Screen::InTray | Screen::Jobs | Screen::Tracker | Screen::Insights | Screen::Profile | Screen::Portals | Screen::Workspace => false,
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
                let keys = !self.confirm_quit && !self.update_screen.notes_open() && !self.notice_screen.details_open();
                let (jobs, applications) = (failed(Source::Jobs), failed(Source::Applications));
                // In the alert's order, so the subtitle names them as the alert does.
                let missing: Vec<&str> = failures
                    .iter()
                    .filter_map(|(source, _)| match source_place(*source) {
                        (Some(screen), true) => Some(screen.title()),
                        _ => None,
                    })
                    .collect();
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
                // The filters hide rows only: each job stays in the list `/rank` put it in.
                self.jobs.set_filters(self.settings.preferences().jobs_view);
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
                if let Some(desk) = &mut self.desk
                    && self.run_screen.show(ui, desk, &root) == Some(RunScreenRequest::NewTask)
                {
                    self.new_task();
                }
            }
            Screen::Profile => {
                // A file that fails hides only the topics it feeds, so the screen as a whole never fails.
                // A run that just ended may still be writing, and its changes are not marked until the load after its end
                // arrives. A save in between would be marked as the run's, so Save waits for that load too, for
                // `RUN_READ_WAIT` at most.
                let reading = self.save_waits_for_load();
                let writer = self.folder_writer();
                let busy = writer == Some(FolderWriter::Task);
                let folder_wait = match writer {
                    Some(FolderWriter::FolderUpdate(wait)) => Some(wait),
                    Some(FolderWriter::Task) | None => None,
                };
                let moment = Moment { busy, reading, folder_wait, now: self.now() };
                let asked = match self.profile_editor.as_mut() {
                    Some(editor) => profile::ui::show(
                        ui,
                        &mut self.profile_screen,
                        &state(whole),
                        &snapshot.profile,
                        &snapshot.cv_documents,
                        self.pages.as_mut(),
                        editor,
                        &mut self.marks,
                        moment,
                    ),
                    None => None,
                };
                match asked {
                    Some(ProfileRequest::Saved) => self.read_back_profile(),
                    // Each checks the name again against `cv/` before the system's program gets the file.
                    Some(ProfileRequest::OpenPdf(name)) => {
                        if let Some(workspace) = &self.workspace {
                            profile::open_cv_pdf(workspace.path(), &name);
                        }
                    }
                    Some(ProfileRequest::Reveal(name)) => {
                        if let Some(workspace) = &self.workspace {
                            profile::reveal_cv_file(workspace.path(), &name);
                        }
                    }
                    Some(ProfileRequest::Leave) => {
                        if let Some(screen) = self.leave_to.take() {
                            self.screen = screen;
                        }
                        ui.ctx().request_repaint();
                    }
                    None => {}
                }
            }
            Screen::Portals => {
                let wait = self.portals_wait();
                let market = profile::application::home_country(&snapshot.profile);
                let browse = BrowseContext { community: &mut self.community, market: market.as_deref(), now, now_utc };
                if let Some(request) = portals::ui::show(ui, &mut self.portals_screen, &state(load(Source::Portals)), &snapshot.portals, wait, browse) {
                    self.change_portals(request);
                }
            }
            Screen::Workspace => {
                let name = self.workspace.as_ref().map(WorkspaceRoot::name).unwrap_or_default();
                // Only previews lack the tool check. They draw no Setup, but still the alert.
                let folder_wait = self.folder_wait();
                let action = match self.environment.as_mut() {
                    Some(environment) => workspace::ui::show(ui, &state(whole), &name, &snapshot.workspace, environment, folder_wait),
                    None => {
                        page_body(ui, alerts);
                        None
                    }
                };
                match action {
                    Some(WorkspaceAction::ChangeFolder) => {
                        let ctx = ui.ctx().clone();
                        self.choose_folder(&ctx);
                    }
                    Some(WorkspaceAction::OpenProfile) => self.screen = Screen::Profile,
                    Some(WorkspaceAction::OpenPortals) => self.screen = Screen::Portals,
                    None => {}
                }
            }
            Screen::Launcher => {
                // The cap sums the month as Usage statistics does, and waits while the run logs are read.
                let cap = self.cap();
                if back_pressed(ui.ctx()) {
                    self.screen = self.launcher_from;
                    ui.ctx().request_repaint();
                } else if let Some(desk) = &mut self.desk {
                    let preferences = self.settings.preferences();
                    // Test connection is a Claude process too, so Run waits for it to end.
                    let testing = self.providers.test_gate(false) == TestGate::Testing;
                    let rules = LaunchRules { defaults: &preferences.run_defaults, cap: &cap, testing };
                    match self.launcher.show(ui, desk, &rules) {
                        Some(LauncherRequest::Started(id)) => {
                            self.run_screen.select(id);
                            self.screen = Screen::Runs;
                        }
                        // Settings keeps the launcher underneath, so Back to app returns to it.
                        Some(LauncherRequest::OpenRunDefaults) => self.open_settings(Page::Defaults),
                        None => {}
                    }
                }
            }
        }
        if open_setup.get() && setup_tab {
            self.leave_for(Screen::Workspace);
        } else if open_setup.get() {
            self.open_settings(Page::Runtime);
        }
        if self.screen != Screen::Profile {
            self.profile_screen.put_away();
        }
        if self.screen != Screen::Portals {
            self.portals_screen.put_away();
        }
    }

    /// Switches a portal or moves it to the Trash, as Profile's Save writes: never while a run works or the folder
    /// updates, which the shell checks again here, whatever the screen showed. What it wrote is read back at once, and
    /// the snapshot worker is asked to read again. The Portals screen asks for it; tests ask directly.
    pub fn change_portals(&mut self, request: PortalsRequest) {
        let wait = self.portals_wait();
        let Some(changes) = &self.portal_changes else { return };
        let busy = wait.is_some();
        let result = match &request {
            PortalsRequest::Switch { folder, on } => changes.switch(folder, *on, busy),
            PortalsRequest::Remove { folder, origin } => changes.remove(folder, *origin, busy),
        };
        self.portals_screen.settle(&request, result, wait);
        self.read_back_portals();
        if let Some(reload) = &self.reload {
            reload.ask();
        }
    }

    /// Reads the portals back after a switch or a removal, so the change shows at once. The jobs they found come from the
    /// snapshot's saved jobs, and stay hidden while those cannot be read.
    fn read_back_portals(&mut self) {
        let Some(root) = self.workspace.as_ref() else { return };
        let jobs_failed = self.snapshot.issues.iter().any(|issue| issue.source == Source::Jobs);
        let (mut view, error) = portals::load(root.path(), (!jobs_failed).then_some(&self.snapshot.board));
        view.set_community(&self.community.installed_hints());
        self.snapshot.workspace.set_portals(&view);
        self.snapshot.portals = view;
        self.snapshot.issues.retain(|issue| issue.source != Source::Portals);
        self.snapshot.issues.extend(error.map(|error| Issue { source: Source::Portals, error }));
        self.portals_changed = Some(Instant::now());
    }

    /// Reads the profile back after a save, so the saved block shows at once instead of after the folder watcher's next
    /// load. It also stands in for a load that did not follow a run in time. Only the profile files are read, as a
    /// snapshot reads them, and the next snapshot replaces the result.
    fn read_back_profile(&mut self) {
        let Some(root) = self.workspace.as_ref() else { return };
        let (view, errors) = profile::load(root.path());
        self.snapshot.profile = view;
        self.snapshot.issues.retain(|issue| issue.source != Source::Profile);
        self.snapshot.issues.extend(errors.into_iter().map(|error| Issue { source: Source::Profile, error }));
        self.profile_saved = Some(Instant::now());
    }

    /// The monthly cap against this month's spend. The spend is unknown until the run logs are read, and again from a task's
    /// end until a snapshot read after it arrives, so a new task cannot start over the cap in between.
    fn cap(&self) -> CapCheck {
        let snapshot = &self.snapshot;
        let usage = (snapshot.usage_read && snapshot.usage_asked >= self.usage_due).then_some(&snapshot.usage);
        cap_check(&self.settings.preferences().cap, usage, snapshot.today)
    }

    /// The newest task that said how it signed in, with the method its process started with. A task read back from its
    /// log has none, so a key it reports is nobody's in particular.
    fn last_task(&self) -> Option<LastTask> {
        let run = self.desk.as_ref()?.latest_signed_in()?;
        let at = NaiveDateTime::parse_from_str(&run.started_at, "%Y-%m-%d %H:%M").ok();
        Some(LastTask { source: run.api_key_source.clone().unwrap_or_default(), method: run.sign_in, at })
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
        let folder_wait = self.folder_wait();
        let snapshot = &self.snapshot;
        // The checkout can show before the run logs are read, so Usage statistics waits for its own read.
        let usage = match (&self.workspace, self.loaded && snapshot.usage_read) {
            (None, _) => UsageState::NoFolder,
            (Some(_), false) => UsageState::Reading,
            (Some(_), true) => UsageState::Read { report: &snapshot.usage, pipeline: &snapshot.pipeline, today: snapshot.today },
        };
        // The cap as the launcher checks it, so the rail says it is reached exactly when Run stops.
        let cap = self.cap();
        let data = self.data_label.clone().or_else(|| self.data.as_ref().map(|data| data.display().to_string()));
        // Only the open folder's decisions can be cleared, and none from a tray that could not be read or saved.
        let decisions = self.tray.as_ref().filter(|tray| folder_open && tray.issue().is_none()).map(InTray::decision_count);
        // Until the first tool check answers, About says it is checking, as Runtime status does, never that Claude Code is missing.
        let checking = self.environment.as_ref().is_some_and(|environment| environment.results().is_none());
        let claude_version = self.environment.as_ref().and_then(|environment| environment.version_of("claude")).map(str::to_owned);
        let claude = match claude_version.as_deref() {
            Some(version) => ClaudeVersion::Found(version),
            None if checking => ClaudeVersion::Checking,
            None => ClaudeVersion::NotFound,
        };
        // The mask is read on the first frame Providers shows, never at launch. It asks the keychain only when no test or task asked it yet,
        // and never waits on a read a task's start has under way. Each later frame only picks up a key read since a failed read, or one under way.
        if self.settings_screen.page() == Page::Providers {
            self.providers.read_once();
        }
        let last_task = self.last_task();
        // Any task process still up counts, a session that waits on a reply included: one Claude process at a time.
        let task_running = self.desk.as_ref().is_some_and(RunDesk::has_live_process);
        let task_working = self.desk.as_ref().is_some_and(RunDesk::is_busy);
        let now = self.now();
        let terms = match (folder_open, self.loaded) {
            (false, _) => TermsState::NoFolder,
            (true, false) => TermsState::Reading,
            (true, true) => TermsState::Read(&snapshot.terms),
        };
        // What the Jobs view hides, counted as Jobs hides it, once the saved jobs are read.
        let jobs = (folder_open && self.loaded && !unread.contains(&Source::Jobs)).then(|| {
            let kept = self.tray.as_ref().map(InTray::kept_jobs).unwrap_or_default();
            hidden_total(&snapshot.board, &self.settings.preferences().jobs_view, &kept)
        });
        let update = self.updates.view();
        let restart_wait = self.restart_wait();
        // Of the open folder only, which the folder update forgets at each open. With the automatic check off, it asks
        // for Check now while a check would update the folder: Jobdesk reads nothing from the network on its own.
        let auto_check = self.settings.preferences().check_updates.is_on();
        let folder_note = self.folder_update.about_line(auto_check).filter(|_| folder_open);
        let (environment, view) = (&mut self.environment, &snapshot.workspace);
        let (x, top, _) = SHELL_PADDING;
        let margin = Margin { left: x as i8, right: SCREEN_PADDING.0 as i8, top: top as i8, bottom: top as i8 };
        let mut runtime_asked = None;
        let asked = Frame::new()
            .inner_margin(margin)
            .show(ui, |ui| {
                let page_view = SettingsView {
                    settings: &mut self.settings,
                    folder: folder.as_deref(),
                    folder_read,
                    runtime,
                    folder_wait,
                    usage,
                    today: snapshot.today,
                    cap,
                    data: data.as_deref(),
                    sizes: self.data_sizes,
                    decisions,
                    version: &self.version,
                    claude,
                    providers: &mut self.providers,
                    last_task,
                    task_running,
                    task_working,
                    now,
                    terms,
                    jobs,
                    update: &update,
                    restart_wait,
                    folder_note,
                };
                settings::ui::show(ui, &mut self.settings_screen, page_view, &mut |ui, page, query| match (page, environment.as_mut()) {
                    (Page::Runtime, Some(environment)) => {
                        runtime_asked = workspace::ui::runtime_status(ui, &state, view, environment, sign_in, folder_open, query);
                    }
                    // Only previews lack the tool check. They draw no tools, but still the alert.
                    (Page::Runtime, None) => alerts(ui),
                    _ => {}
                })
            })
            .inner;
        // Open Profile and Open Portals leave Settings for their screen, as Back to app would leave it for the screen underneath.
        match runtime_asked {
            Some(WorkspaceAction::OpenProfile) => (self.settings_open, self.screen) = (false, Screen::Profile),
            Some(WorkspaceAction::OpenPortals) => (self.settings_open, self.screen) = (false, Screen::Portals),
            Some(WorkspaceAction::ChangeFolder) | None => {}
        }
        let ctx = ui.ctx().clone();
        // Providers picked this frame drew before the read, so it reads now and draws again.
        // While a task's start reads the keychain, it looks again a little later instead of waiting on that prompt.
        if self.settings_screen.page() == Page::Providers && (self.providers.saved() == &Saved::NotRead || self.providers.keychain_busy()) {
            if self.providers.read_once() {
                ctx.request_repaint_after(KEYCHAIN_LOOK_AGAIN);
            } else {
                ctx.request_repaint();
            }
        }
        match asked {
            Some(SettingsRequest::Back) => {
                self.settings_open = false;
                ctx.request_repaint();
            }
            Some(SettingsRequest::OpenLastTask) => {
                if let Some(id) = self.desk.as_ref().and_then(RunDesk::latest_signed_in).map(|run| run.id.clone()) {
                    self.run_screen.select(id);
                    self.settings_open = false;
                    self.screen = Screen::Runs;
                    ctx.request_repaint();
                }
            }
            Some(SettingsRequest::ChangeFolder) => self.choose_folder(&ctx),
            Some(SettingsRequest::ShowFolder) => {
                if let Some(workspace) = &self.workspace {
                    settings::reveal_folder(workspace.path());
                }
            }
            Some(SettingsRequest::OpenApplications) => {
                self.settings_open = false;
                self.screen = Screen::Tracker;
                ctx.request_repaint();
            }
            Some(SettingsRequest::NewTask) => self.new_task(),
            // Edit in Profile leaves Settings for the Profile tab, as Back to app would leave it for the screen underneath.
            Some(SettingsRequest::OpenProfile) => {
                self.settings_open = false;
                self.screen = Screen::Profile;
                ctx.request_repaint();
            }
            // Only Jobdesk's own data folder, never the checkout.
            Some(SettingsRequest::ShowDataFolder) => {
                if let Some(data) = &self.data {
                    settings::reveal_folder(data);
                }
            }
            Some(SettingsRequest::CheckForUpdates) => self.updates.check_now(now),
            Some(SettingsRequest::DownloadUpdate) => self.follow_update(&ctx, UpdateRequest::Act(BarAction::Download)),
            Some(SettingsRequest::CancelUpdate) => self.follow_update(&ctx, UpdateRequest::Act(BarAction::Cancel)),
            Some(SettingsRequest::RestartToUpdate) => self.follow_update(&ctx, UpdateRequest::Act(BarAction::Restart)),
            // Over Settings, anchored to the update group in the bar, which shows whenever there are notes.
            Some(SettingsRequest::OpenReleaseNotes) => self.update_screen.open_notes(),
            Some(SettingsRequest::OpenReleasePage) => {
                if let Some(url) = update.notes.and_then(|notes| notes.release_page) {
                    self.follow_update(&ctx, UpdateRequest::OpenPage(url));
                }
            }
            // The tray belongs to the open folder, so other folders keep their decisions. A tray that could not be read clears nothing.
            Some(SettingsRequest::ClearDecisions) => {
                if self.workspace.is_some()
                    && let Some(tray) = &mut self.tray
                {
                    tray.clear_decisions();
                }
                self.measure_data();
            }
            None => {}
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

/// Deletes the run logs `retention` no longer keeps, judged by when each was last written. Forever deletes nothing.
/// It reads only `runs/` in the data folder, so the settings file and the tray's decisions beside it stay.
fn prune_run_logs(data: &std::path::Path, retention: Retention, now: NaiveDateTime) -> usize {
    // A moment the clock repeats takes its earlier reading, which deletes the fewer logs. One it skips has none, so nothing is deleted.
    let Some(keep_after) = retention.keep_after(now).and_then(|cutoff| Local.from_local_datetime(&cutoff).earliest()) else { return 0 };
    runs::prune_logs(&data.join("runs"), SystemTime::from(keep_after))
}

/// First-run setup over the data folder `data`, and the home folder it works in. Its record is cleared once setup gets
/// past the Computer step, and a remembered folder skips setup before this. macOS only, and only with a home folder and
/// a processor setup has helper files for.
#[cfg(target_os = "macos")]
fn first_run(ctx: &egui::Context, data: &Path) -> Option<(Onboarding, PathBuf)> {
    let home = std::env::var_os("HOME").map(PathBuf::from).filter(|home| home.is_absolute())?;
    let helpers = crate::onboarding::helper_setup(data, env!("CARGO_PKG_VERSION"), home.clone())?;
    let repaint = ctx.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || repaint.request_repaint());
    let setup = crate::onboarding::first_run(data, env!("CARGO_PKG_VERSION"), home.join("Documents"), helpers, wake);
    Some((setup, home))
}

/// Windows shows the welcome and the folder picker, as before first-run setup.
#[cfg(not(target_os = "macos"))]
fn first_run(_: &egui::Context, _: &Path) -> Option<(Onboarding, PathBuf)> {
    None
}

/// Asks the window to take `size`, keeping its centre where it was.
fn resize(ctx: &egui::Context, size: WindowSize) {
    let (monitor, outer, inner) = ctx.input(|input| (input.viewport().monitor_size, input.viewport().outer_rect, input.viewport().inner_rect));
    for command in window_commands(size, monitor, outer, inner) {
        ctx.send_viewport_cmd(command);
    }
}

/// What the window is asked, in order, to take `size` on a screen of `monitor` points, from where `outer` stands
/// round `inner`. The smallest size goes first for the small window, so the old one never holds it up, and last for the
/// app's. Either way the window keeps its centre, title bar included, so growing and shrinking it again leaves it
/// where it was.
fn window_commands(size: WindowSize, monitor: Option<egui::Vec2>, outer: Option<Rect>, inner: Option<Rect>) -> Vec<egui::ViewportCommand> {
    use egui::ViewportCommand::{InnerSize, MinInnerSize, OuterPosition};
    let (to, mut commands) = match size {
        WindowSize::FirstRun => (FIRST_WINDOW, vec![MinInnerSize(FIRST_WINDOW), InnerSize(FIRST_WINDOW)]),
        WindowSize::App => {
            let grown = monitor.map_or(WINDOW, |monitor| WINDOW.min(monitor)).max(SMALLEST_WINDOW);
            (grown, vec![InnerSize(grown), MinInnerSize(SMALLEST_WINDOW)])
        }
    };
    if let Some(outer) = outer {
        // The title bar and borders round the inner rect, which the window keeps at either size.
        let frame = inner.map_or(egui::Vec2::ZERO, |inner| ((inner.min - outer.min) + (outer.max - inner.max)).max(egui::Vec2::ZERO));
        commands.push(OuterPosition(outer.center() - (to + frame) / 2.0));
    }
    commands
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
    // Setup can leave the menu once the folder is chosen, so the step names Profile, whose tab always stays.
    welcome_step(ui, 2, "Check your profile", "Profile shows each topic, what is filled in and what is missing.", false);
    welcome_step(ui, 3, "Find your first jobs", "Opens the search task. You start it, and Today fills in as it works.", false);
    ui.add_space(WELCOME_STEP_PADDING / 2.0);
    if let Some(error) = error {
        alert(ui, error);
        ui.add_space(WELCOME_STEP_PADDING / 2.0);
    }
    let promise = "Jobdesk reads this folder and changes it only when you save a Profile edit, switch a portal on or off, or remove one, and when a Jobdesk update brings newer versions of its files, which you can undo right after. Tasks you start change it through Claude Code on this computer.";
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
        Source::Profile | Source::CvDocuments => (Some(Screen::Profile), false),
        Source::Portals => (Some(Screen::Portals), false),
        Source::Decisions => (None, false),
    }
}

/// What an alert says when a source could not be read: what failed, then what still works when it fails alone.
fn failed_words(source: Source) -> (&'static str, &'static str) {
    match source {
        Source::Applications => {
            ("Your applications could not be read.", "Today shows new matches and tasks until it is fixed. Jobs, Insights and Tasks still work.")
        }
        Source::Jobs => {
            ("Your saved jobs could not be read.", "Today shows applications and tasks until it is fixed. Applications, Insights and Tasks still work.")
        }
        Source::SkillGaps => ("Your skill gap reports could not be read.", "Everything else still works."),
        Source::Profile => ("Part of your profile could not be read.", "Everything else still works."),
        Source::CvDocuments => ("Your CV folder could not be read.", "Your profile topics still show, and every task still runs."),
        Source::Portals => ("Your portals could not be read.", "Everything else still works."),
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

    fn on_exit(&mut self) {
        self.leave_restart();
    }
}

pub fn install_look(ctx: &egui::Context) {
    install_fonts(ctx);
    install_theme(ctx);
}

/// `--workspace <path>` wins over `JOBDESK_WORKSPACE`, which wins over the last folder used.
fn requested_workspace() -> Option<PathBuf> {
    requested_workspace_from(std::env::args_os().skip(1), std::env::var_os("JOBDESK_WORKSPACE"))
}

/// Takes `OsString`s, so a path that is not Unicode arrives whole.
fn requested_workspace_from(mut args: impl Iterator<Item = OsString>, env: Option<OsString>) -> Option<PathBuf> {
    args.find(|arg| arg == "--workspace").and_then(|_| args.next()).or(env).map(PathBuf::from)
}

/// The data folder `JOBDESK_DATA_DIR` names, if it names one. Only an absolute path counts: an empty or relative one
/// names none, so the folder never depends on where the app was started.
fn moved_data_folder(env: Option<OsString>) -> Option<PathBuf> {
    env.map(PathBuf::from).filter(|path| path.is_absolute())
}

/// A `moved` folder wins over eframe's usual place, so a copy that is tried out never touches the user's own data folder.
fn data_folder_from(moved: Option<PathBuf>, default: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    moved.or_else(default)
}

/// Makes the data folder, as eframe did when it looked it up itself. A `moved` folder that cannot be made is an error,
/// so the window never opens on a session whose saves all fail. The usual place is only tried, so an unwritable home
/// still opens the app.
fn make_data_folder(data: &Path, moved: bool) -> std::io::Result<()> {
    match std::fs::create_dir_all(data) {
        Err(error) if moved => Err(std::io::Error::new(error.kind(), format!("cannot make the data folder {}: {error}", data.display()))),
        _ => Ok(()),
    }
}

/// eframe keeps the window's state in `app.ron` in the data folder, not in a place of its own.
fn native_options(icon: egui::IconData, data: Option<&Path>) -> eframe::NativeOptions {
    eframe::NativeOptions {
        // First-run setup asks for its small window on its first frame, whatever size eframe restored, and grows it to
        // this size when it hands over.
        viewport: egui::ViewportBuilder::default().with_inner_size(WINDOW).with_min_inner_size(SMALLEST_WINDOW).with_title("Jobdesk").with_icon(icon),
        persistence_path: data.map(|data| data.join("app.ron")),
        ..Default::default()
    }
}

/// Opens the window, and once it has closed applies the update the user chose Restart to update for, if any.
pub fn run() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/brand/jobdesk-icon-1024.png")).unwrap_or_default();
    let moved = moved_data_folder(std::env::var_os(DATA_DIR_VARIABLE));
    let data = data_folder_from(moved.clone(), || eframe::storage_dir("Jobdesk"));
    if let Some(data) = &data {
        make_data_folder(data, moved.is_some()).map_err(|error| eframe::Error::AppCreation(Box::new(error)))?;
    }
    let options = native_options(icon, data.as_deref());
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let restart = RestartSlot::default();
    let (slot, folder) = (Arc::clone(&restart), data.clone());
    let result = eframe::run_native(
        "Jobdesk",
        options,
        Box::new(move |creation| {
            install_look(&creation.egui_ctx);
            let saved = creation.storage.and_then(|storage| storage.get_string(WORKSPACE_KEY)).map(PathBuf::from);
            Ok(Box::new(Shell::start(&creation.egui_ctx, requested_workspace().or(saved), folder, slot, args)))
        }),
    );
    // The window is closed and every session with it, so nothing is cut off as the update applies.
    let chosen = restart.lock().ok().and_then(|mut slot| slot.take());
    if let (Some((plan, args)), Some(data)) = (chosen, &data) {
        // The new copy opens the same moved folder, or the usual one.
        updates::finish(data, moved.as_deref(), &plan, &args);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::path::Path;
    use std::rc::Rc;

    use super::*;
    use crate::runs::application::{AgentGateway, AgentSession, Decision, EventSink, Prompt, RunRequest, StartOptions};
    use crate::settings::application::{Preferences, ThemeChoice};
    use crate::workspace::application::{ToolProber, ToolSpec};

    /// Without a test clock, the community list's times count from the system's UTC clock, never from the wall clock
    /// turned back, which reads an hour early in the hour repeated when clocks go back. A test clock reads as local time.
    #[test]
    fn utc_now_is_the_systems_clock() {
        let shell = Shell::without_workspace(NaiveDate::from_ymd_opt(2026, 10, 25).unwrap());
        let (before, now, after) = (Utc::now(), shell.now_utc(), Utc::now());
        assert!(before <= now && now <= after, "{before} {now} {after}");
        let wall = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap().and_hms_opt(12, 0, 0).unwrap();
        let fixed = Shell::without_workspace(wall.date()).with_now(wall);
        assert_eq!(fixed.now_utc(), Local.from_local_datetime(&wall).single().unwrap().to_utc());
    }

    /// Every tool answers, so launching asks no real program.
    struct AllTools;

    impl ToolProber for AllTools {
        fn probe(&self, _: &ToolSpec) -> Option<String> {
            Some("1.0".to_owned())
        }
    }

    fn launch(ctx: &egui::Context, path: Option<PathBuf>, data: &Path) -> Shell {
        launch_with(ctx, path, data, None)
    }

    /// A launch whose keychain, kept in memory, holds `key`, and whose Test connection never answers.
    fn launch_with(ctx: &egui::Context, path: Option<PathBuf>, data: &Path, key: Option<&str>) -> Shell {
        let providers = settings::open_providers(settings::memory_keychain(key), settings::idle_tester());
        let checks = EnvironmentChecks::new(Arc::new(AllTools), Arc::new(|| {}));
        Shell::launch(
            ctx,
            path,
            Some(data.to_owned()),
            checks,
            providers,
            |auto| updates::idle_updates("0.1.0", auto),
            |_| portals::scripted_community(None, Vec::new()),
            folder_update::idle_folder_update("0.1.0"),
        )
    }

    const FAKE_KEY: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";

    /// Every desk takes the method in use as it opens, a folder change included, since a new desk starts on the Claude
    /// sign-in. A change of method reaches the open desk once.
    #[test]
    fn every_desk_opened_takes_the_sign_in_in_use() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        settings::open_settings(data.path(), Preferences::default()).set_sign_in(SignIn::ApiKey);
        let ctx = egui::Context::default();
        let mut shell = launch_with(&ctx, Some(checkout(&folder)), data.path(), Some(FAKE_KEY));
        assert_eq!(shell.desk.as_ref().unwrap().sign_in(), SignIn::ApiKey, "the desk opened at launch");

        shell.open(&ctx, &checkout_named(&folder, "other-search"));
        assert_eq!(shell.workspace.as_ref().map(WorkspaceRoot::name), Some("other-search".to_owned()));
        assert_eq!(shell.desk.as_ref().unwrap().sign_in(), SignIn::ApiKey, "the desk of the new folder");

        shell.settings.set_sign_in(SignIn::ClaudeSignIn);
        shell.follow_sign_in();
        assert_eq!(shell.desk.as_ref().unwrap().sign_in(), SignIn::ClaudeSignIn, "the change reaches the open desk");
        shell.desk.as_mut().unwrap().set_sign_in(SignIn::ApiKey);
        shell.follow_sign_in();
        assert_eq!(shell.desk.as_ref().unwrap().sign_in(), SignIn::ApiKey, "an unchanged method leaves the desk alone");

        shell.open(&ctx, &checkout_named(&folder, "third-search"));
        assert_eq!(shell.desk.as_ref().unwrap().sign_in(), SignIn::ClaudeSignIn, "a new desk takes the method in use, not the last desk's");
    }

    /// With no data folder, the community list is still asked for, with no folder to save it in, so Browse reads it.
    #[test]
    fn no_data_folder_still_asks_for_the_community_list() {
        let providers = settings::open_providers(settings::memory_keychain(None), settings::idle_tester());
        let checks = EnvironmentChecks::new(Arc::new(AllTools), Arc::new(|| {}));
        let mut asked = None;
        let ctx = egui::Context::default();
        Shell::launch(
            &ctx,
            None,
            None,
            checks,
            providers,
            |auto| updates::idle_updates("0.1.0", auto),
            |data| {
                asked = Some(data.map(Path::to_owned));
                portals::scripted_community(None, Vec::new())
            },
            folder_update::idle_folder_update("0.1.0"),
        );
        assert_eq!(asked, Some(None));
    }

    /// A key known to be gone signs tasks in with the Claude sign-in, whatever the setting says.
    #[test]
    fn a_missing_key_opens_the_desk_on_the_claude_sign_in() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        settings::open_settings(data.path(), Preferences::default()).set_sign_in(SignIn::ApiKey);
        let ctx = egui::Context::default();
        let mut shell = launch_with(&ctx, Some(checkout(&folder)), data.path(), None);
        assert_eq!(shell.desk.as_ref().unwrap().sign_in(), SignIn::ApiKey, "not read yet, so the task's own read decides");
        shell.providers.read_once();
        shell.follow_sign_in();
        assert_eq!(shell.desk.as_ref().unwrap().sign_in(), SignIn::ClaudeSignIn);
    }

    /// A session that stays up until the test ends.
    struct Idle;

    impl AgentGateway for Idle {
        fn start(&self, _: &RunRequest, _: EventSink) -> Box<dyn AgentSession> {
            Box::new(Idle)
        }
    }

    impl AgentSession for Idle {
        fn send_message(&mut self, _: &str) {}
        fn respond(&mut self, _: &Prompt, _: &Decision) {}
        fn interrupt(&mut self) {}
    }

    /// An empty folder that opens as a checkout.
    fn checkout(folder: &tempfile::TempDir) -> PathBuf {
        checkout_named(folder, "ai-job-search")
    }

    fn checkout_named(folder: &tempfile::TempDir, name: &str) -> PathBuf {
        let root = folder.path().join(name);
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

    /// The Today queue and Jobs view settings saved in one session are in force at the next launch, and the tray deals by them.
    #[test]
    fn the_queue_and_jobs_view_come_back_at_launch() {
        use crate::settings::application::{ComingUp, LaterLasts};
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let mut saved = settings::open_settings(data.path(), Preferences::default());
        saved.set_coming_up(ComingUp::Week);
        saved.set_later(LaterLasts::ThreeDays);
        saved.add_skip_reason("Culture").unwrap();
        saved.set_hide_under_score(true);
        saved.set_score_floor(75);
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        let preferences = shell.settings.preferences();
        assert_eq!((preferences.queue.coming_up, preferences.queue.later), (ComingUp::Week, LaterLasts::ThreeDays));
        assert_eq!(preferences.queue.skip_reasons.list().last().map(String::as_str), Some("Culture"));
        assert!(preferences.jobs_view.under_score);
        assert_eq!(preferences.jobs_view.score, 75);
        assert_eq!(shell.queue().horizon, ComingUp::Week, "the tray deals by the saved range");
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

    /// A running task keeps its folder: a new one would replace the desk and drop the live session.
    #[test]
    fn a_running_task_keeps_its_folder() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let first = checkout(&folder);
        let mut shell = launch(&ctx, Some(first.clone()), data.path());
        let mut desk = RunDesk::new(Box::new(Idle), Arc::new(|| {}));
        let id = desk.start("/apply https://kestrel.example/jobs/2", &StartOptions { model: None, budget_usd: 5.0 }, Local::now().naive_local()).unwrap();
        shell.desk = Some(desk);

        shell.open(&ctx, &checkout_named(&folder, "other-search"));

        assert_eq!(shell.workspace.as_ref().map(WorkspaceRoot::path), Some(first.as_path()), "the folder stays");
        assert!(shell.desk.as_ref().is_some_and(|desk| desk.has_session(&id)), "the task keeps its session");

        shell.desk = None;
        shell.open(&ctx, &checkout_named(&folder, "other-search"));
        assert_eq!(shell.workspace.as_ref().map(|workspace| workspace.name()), Some("other-search".to_owned()), "with no task, the folder changes");
    }

    /// A task in plain words belongs to the folder it was written for, so a folder change drops it, even from the
    /// launcher Settings keeps underneath.
    #[test]
    fn a_folder_change_drops_a_task_in_plain_words() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        assert!(shell.launcher.prefill("Read the sample portal's code and tell me what it does."));
        shell.open(&ctx, &checkout_named(&folder, "other-search"));
        assert_eq!(shell.launcher.words(), None);
    }

    /// A folder change from anywhere, such as Settings, asks before it drops unsaved profile text, and goes on only once
    /// the user agrees.
    #[test]
    fn a_folder_change_asks_before_it_drops_a_draft() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let first = checkout(&folder);
        let mut shell = launch(&ctx, Some(first.clone()), data.path());
        let key = crate::profile::application::BlockKey {
            file: crate::profile::application::ProfileFile::Candidate,
            level: 2,
            heading: "Education".to_owned(),
            parent: None,
            nth: 0,
        };
        let editor = shell.profile_editor.as_mut().unwrap();
        editor.open(key, "## Education\nMSc".to_owned());
        *editor.draft_mut().unwrap() = "## Education\nPhD".to_owned();

        let other = checkout_named(&folder, "other-search");
        shell.choose_folder_with(&ctx, || Some(other.clone()));
        assert_eq!(shell.workspace.as_ref().map(WorkspaceRoot::path), Some(first.as_path()), "the folder waits for the answer");
        assert_eq!(shell.away, Some((Pending::Folder(other.clone()), true)));
        assert!(shell.draft_unsaved(), "the draft stays");

        let (pending, _) = shell.away.take().unwrap();
        shell.answer_away(&ctx, pending);
        assert_eq!(shell.workspace.as_ref().map(|workspace| workspace.name()), Some("other-search".to_owned()), "Discard opens it");
        assert!(!shell.draft_unsaved());
    }

    /// While a session is open, Change folder opens no dialog at all.
    #[test]
    fn a_running_task_opens_no_folder_dialog() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        let mut desk = RunDesk::new(Box::new(Idle), Arc::new(|| {}));
        desk.start("/apply https://kestrel.example/jobs/2", &StartOptions { model: None, budget_usd: 5.0 }, Local::now().naive_local()).unwrap();
        shell.desk = Some(desk);
        let asked = Cell::new(false);

        shell.choose_folder_with(&ctx, || {
            asked.set(true);
            None
        });
        assert!(!asked.get(), "no dialog while the session is open");
        assert_eq!(shell.folder_wait(), Some(FOLDER_BUSY));

        shell.desk = None;
        shell.choose_folder_with(&ctx, || {
            asked.set(true);
            None
        });
        assert!(asked.get(), "with no session, the dialog opens");
        assert_eq!(shell.folder_wait(), None);
    }

    /// A data folder with an old run log and a fresh one, and the settings and tray files beside `runs/`, all as old as the old log.
    /// Returns the old log's path and the fresh one's.
    fn aged_data(data: &Path, retention: Retention) -> (PathBuf, PathBuf) {
        settings::open_settings(data, Preferences::default()).set_retention(retention);
        let runs = data.join("runs");
        fs::create_dir_all(&runs).unwrap();
        fs::create_dir_all(data.join("intray")).unwrap();
        let (old, fresh) = (runs.join("20250102-091500-1.jsonl"), runs.join("20261031-091500-1.jsonl"));
        let long_ago = SystemTime::now() - std::time::Duration::from_secs(400 * 24 * 60 * 60);
        fs::write(data.join("intray/decisions.json"), "{}").unwrap();
        for path in [&old, &fresh] {
            fs::write(path, "{\"type\":\"jobdesk_meta\"}\n").unwrap();
        }
        for path in [old.clone(), data.join("settings.json"), data.join("intray/decisions.json")] {
            fs::File::options().write(true).open(path).unwrap().set_modified(long_ago).unwrap();
        }
        (old, fresh)
    }

    /// A shorter time deletes the old run logs at launch, and only those: the settings and the tray's decisions stay.
    #[test]
    fn a_launch_deletes_the_run_logs_past_the_kept_time() {
        let data = tempfile::tempdir().unwrap();
        let (old, fresh) = aged_data(data.path(), Retention::Days30);
        let ctx = egui::Context::default();
        let shell = launch(&ctx, None, data.path());
        assert!(!old.exists(), "the old log went");
        assert!(fresh.exists(), "the fresh log stays");
        assert!(data.path().join("settings.json").exists() && data.path().join("intray/decisions.json").exists());
        assert_eq!(shell.settings.preferences().retention, Retention::Days30, "the settings file still reads");
    }

    #[test]
    fn forever_deletes_no_run_log() {
        let data = tempfile::tempdir().unwrap();
        let (old, fresh) = aged_data(data.path(), Retention::Forever);
        let ctx = egui::Context::default();
        launch(&ctx, None, data.path());
        assert!(old.exists() && fresh.exists());
    }

    /// The usual place, as the test reads it: a folder no test writes to.
    fn usual_place() -> Option<PathBuf> {
        Some(PathBuf::from("usual place"))
    }

    /// `JOBDESK_DATA_DIR` set to an absolute path is the data folder, and eframe's usual place is never looked up.
    #[test]
    fn an_absolute_data_dir_is_the_data_folder() {
        let moved = tempfile::tempdir().unwrap();
        let named = moved_data_folder(Some(moved.path().as_os_str().to_owned()));
        assert_eq!(named.as_deref(), Some(moved.path()));
        let found = data_folder_from(named, || panic!("the usual place is not looked up"));
        assert_eq!(found.as_deref(), Some(moved.path()));
    }

    /// Unset, empty or relative, the folder is the usual place, never one under where the app was started.
    #[test]
    fn an_unset_empty_or_relative_data_dir_keeps_the_usual_place() {
        for env in [None, Some(""), Some("data"), Some("./data"), Some("../data")] {
            let named = moved_data_folder(env.map(OsString::from));
            assert_eq!(named, None, "{env:?}");
            assert_eq!(data_folder_from(named, usual_place), usual_place(), "{env:?}");
        }
        assert_eq!(data_folder_from(None, || None), None, "no usual place on this computer");
    }

    /// eframe's `app.ron` goes in the data folder, so a moved folder takes the window's state with it.
    #[test]
    fn the_window_state_is_kept_in_the_data_folder() {
        let moved = tempfile::tempdir().unwrap();
        let options = native_options(egui::IconData::default(), Some(moved.path()));
        assert_eq!(options.persistence_path, Some(moved.path().join("app.ron")));
        assert_eq!(native_options(egui::IconData::default(), None).persistence_path, None);
    }

    /// A moved data folder holds the settings, and the run logs that launch prunes, while the usual place is never read.
    #[test]
    fn a_moved_data_folder_holds_the_settings_and_the_run_logs() {
        let moved = tempfile::tempdir().unwrap();
        let named = moved_data_folder(Some(moved.path().as_os_str().to_owned()));
        let data = data_folder_from(named, || panic!("the usual place is not looked up")).unwrap();
        let ctx = egui::Context::default();
        let shell = launch(&ctx, None, &data);
        assert_eq!(shell.data.as_deref(), Some(moved.path()));
        assert!(moved.path().join("settings.json").exists(), "the first launch saves the settings there");

        let (old, fresh) = aged_data(&data, Retention::Days30);
        launch(&ctx, None, &data);
        assert!(!old.exists() && fresh.exists() && fresh.starts_with(moved.path().join("runs")), "the launch prunes the run logs there");
    }

    /// A moved data folder that cannot be made stops the launch, while the usual place that cannot be made still opens.
    #[test]
    fn a_data_folder_that_cannot_be_made_stops_only_a_moved_launch() {
        let scratch = tempfile::tempdir().unwrap();
        let file = scratch.path().join("a file");
        fs::write(&file, "").unwrap();
        assert!(make_data_folder(&file, true).is_err(), "a moved folder that is a file stops the launch");
        assert!(make_data_folder(&file, false).is_ok(), "the usual place that is a file is ignored");
        let fresh = scratch.path().join("fresh/data");
        make_data_folder(&fresh, true).unwrap();
        assert!(fresh.is_dir(), "a moved folder that can be made is made");
    }

    /// Ask me before each tool reaches a desk as it opens, and the desk that is open on the frame the setting changes.
    #[test]
    fn the_permissions_setting_reaches_the_desk() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        assert!(!shell.desk.as_ref().unwrap().asks_permission(), "allowed and noted by default");
        shell.settings.set_permissions(PermissionMode::Ask);
        shell.follow_permissions();
        assert!(shell.desk.as_ref().unwrap().asks_permission(), "the change reaches the open desk");

        let mut shell = launch(&ctx, Some(checkout_named(&folder, "other-search")), data.path());
        assert!(shell.desk.as_ref().unwrap().asks_permission(), "a newly opened desk asks");
        shell.desk.as_mut().unwrap().set_asks_permission(false);
        shell.follow_permissions();
        assert!(!shell.desk.as_ref().unwrap().asks_permission(), "an unchanged setting leaves the desk alone");
    }

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            let target = to.join(entry.file_name());
            if entry.path().is_dir() { copy_tree(&entry.path(), &target) } else { fs::copy(entry.path(), target).map(|_| ()).unwrap() }
        }
    }

    /// The sample workspace as a checkout named `name`.
    fn sample_checkout(folder: &tempfile::TempDir, name: &str) -> PathBuf {
        let root = checkout_named(folder, name);
        copy_tree(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample_workspace"), &root);
        fs::rename(root.join("CLAUDE.profile.md"), root.join("CLAUDE.md")).unwrap();
        root
    }

    const CANDIDATE: &str = ".claude/skills/job-application-assistant/01-candidate-profile.md";

    /// egui keeps a field's undo history for the life of the window, and a folder change starts a new Profile screen.
    /// An edit in the new folder still has a field of its own, so undo never brings back the last folder's text.
    #[test]
    fn undo_never_crosses_folders() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let folder = tempfile::tempdir().unwrap();
        let (first, second) = (sample_checkout(&folder, "search-a"), sample_checkout(&folder, "search-b"));
        let text = fs::read_to_string(second.join(CANDIDATE)).unwrap().replace("Antwerp. Freelance.", "Ghent. Employed.");
        fs::write(second.join(CANDIDATE), &text).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        let shell = Rc::new(RefCell::new(Shell::with_snapshot(Screen::Profile, workspace::open(&first).unwrap(), Snapshot::load(&first, today))));
        let inner = Rc::clone(&shell);
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(install_look).build_ui(move |ui| inner.borrow_mut().show(ui));
        harness.run_steps(3);
        let field = |harness: &Harness| harness.get_by_role(egui::accesskit::Role::MultilineTextInput).value().unwrap_or_default();

        harness.get_all_by_label("Edit").next().unwrap().click();
        harness.run_steps(3);
        harness.get_by_role(egui::accesskit::Role::MultilineTextInput).type_text(" Remote since 2023.");
        harness.run_steps(3);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        harness.run_steps(3);
        assert!(fs::read_to_string(first.join(CANDIDATE)).unwrap().contains("Remote since 2023."), "saved in the first folder");

        {
            let ctx = harness.ctx.clone();
            let mut shell = shell.borrow_mut();
            shell.open(&ctx, &second);
            (shell.snapshot, shell.loaded) = (Snapshot::load(&second, today), true);
        }
        harness.run_steps(3);
        harness.get_all_by_label("Edit").next().unwrap().click();
        harness.run_steps(3);
        let opened = field(&harness);
        assert!(opened.contains("Ghent. Employed."), "{opened}");
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        harness.run_steps(3);
        assert_eq!(field(&harness), opened, "undo has nothing of the first folder's");
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        harness.run_steps(3);
        assert_eq!(fs::read_to_string(second.join(CANDIDATE)).unwrap(), text, "nothing of the first folder was written");
    }

    /// A snapshot worker that sends nothing after a run ends holds Save for `RUN_READ_WAIT` at most. Jobdesk then reads the
    /// profile itself and marks what the run changed, so a Save after that is not marked when a late snapshot comes.
    #[test]
    fn a_silent_snapshot_worker_holds_save_only_briefly() {
        use crate::profile::application::TopicId;

        let folder = tempfile::tempdir().unwrap();
        let root = sample_checkout(&folder, "search");
        let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        let mut shell = Shell::with_snapshot(Screen::Profile, workspace::open(&root).unwrap(), Snapshot::load(&root, today));
        let feed = shell.snapshot_feed();
        let rewrite = |from: &str, to: &str| {
            let text = fs::read_to_string(root.join(CANDIDATE)).unwrap();
            assert!(text.contains(from), "{from}");
            fs::write(root.join(CANDIDATE), text.replace(from, to)).unwrap();
        };
        shell.marks.begin(&shell.snapshot.profile);
        rewrite("Antwerp. Freelance.", "Ghent. Employed.");
        shell.run_ended = Some(Instant::now());
        let ctx = egui::Context::default();
        shell.settle_run_end(&ctx);
        assert!(shell.save_waits_for_load(), "Save waits for the load after the run");

        shell.run_ended = Instant::now().checked_sub(RUN_READ_WAIT);
        shell.settle_run_end(&ctx);
        assert!(!shell.save_waits_for_load(), "the wait ends without a snapshot");
        assert!(shell.marks.rail_mark(TopicId::Experience), "the run's change is marked from Jobdesk's own read");

        // The user saves, then the worker's late snapshot holds the save.
        rewrite("Meuse Valley University", "Scheldt Valley University");
        feed.send(Snapshot::load(&root, today)).unwrap();
        shell.take_snapshot();
        assert!(!shell.marks.rail_mark(TopicId::Education), "the save is not marked as the run's");
    }

    /// A frame of `shell`'s keys with the window asked to close, and the commands it sent to the window.
    fn close_frame(ctx: &egui::Context, shell: &mut Shell) -> Vec<egui::ViewportCommand> {
        let mut input = egui::RawInput::default();
        input.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
        commands(ctx.run_ui(input, |ui| shell.handle_shortcuts_and_quit(ui.ctx())))
    }

    /// The commands a frame sent to the window. Its textures are dropped, as no renderer takes them.
    fn commands(mut output: egui::FullOutput) -> Vec<egui::ViewportCommand> {
        output.textures_delta.clear();
        output.viewport_output.get(&egui::ViewportId::ROOT).map(|viewport| viewport.commands.clone()).unwrap_or_default()
    }

    /// A close asked for while the folder ask shows is not forgotten: once that ask is answered, the close comes back.
    #[test]
    fn a_close_during_the_folder_ask_comes_back_after_it() {
        let data = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        let key = crate::profile::application::BlockKey {
            file: crate::profile::application::ProfileFile::Candidate,
            level: 2,
            heading: "Education".to_owned(),
            parent: None,
            nth: 0,
        };
        let editor = shell.profile_editor.as_mut().unwrap();
        editor.open(key, "## Education\nMSc".to_owned());
        *editor.draft_mut().unwrap() = "## Education\nPhD".to_owned();
        let other = checkout_named(&folder, "other-search");
        shell.away = Some((Pending::Folder(other.clone()), true));

        let sent = close_frame(&ctx, &mut shell);
        assert!(sent.contains(&egui::ViewportCommand::CancelClose));
        assert_eq!(shell.away, Some((Pending::Folder(other), true)), "the folder ask stays, one ask at a time");

        let sent = commands(ctx.run_ui(egui::RawInput::default(), |ui| shell.settle_away(ui.ctx(), false)));
        assert!(sent.contains(&egui::ViewportCommand::Close), "Keep editing, then the close comes back");
        close_frame(&ctx, &mut shell);
        assert_eq!(shell.away, Some((Pending::Quit, true)), "and asks about the draft");
    }

    /// 09:12 on a fictional day, when the scripted update checks run.
    fn update_time() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(9, 12, 0).unwrap()
    }

    fn release_020() -> crate::updates::application::Release {
        let version = crate::updates::application::Version { major: 0, minor: 2, patch: 0 };
        crate::updates::application::Release {
            version,
            released: None,
            notes: "### Features\n\n- A fictional change".into(),
            size: 10,
            signature: "sig".into(),
            pin: None,
        }
    }

    /// Updates that found 0.2.0 and downloaded it, as a Mac copy of 0.1.0, so Restart to update has a plan.
    fn ready_updates() -> Updates {
        ready_updates_then(Vec::new())
    }

    /// As `ready_updates`, with `later` answering the checks that come after.
    fn ready_updates_then(later: Vec<crate::updates::application::ServiceEvent>) -> Updates {
        use crate::updates::application::{DownloadEnd, DownloadId, Platform, ServiceEvent};
        let mut script = vec![
            ServiceEvent::Checked(Ok(release_020())),
            ServiceEvent::Ended { id: DownloadId(0), end: DownloadEnd::Done(PathBuf::from("/fictional/updates/Jobdesk.app.tar.gz")) },
        ];
        script.extend(later);
        let mut updates = updates::scripted_updates("0.1.0", script, None, true, Some(Platform::MacArm));
        updates.poll(update_time());
        updates.poll(update_time());
        updates.download();
        updates.poll(update_time());
        assert!(updates.restart_plan().is_some());
        updates
    }

    /// A launched shell on a fresh checkout, with a download ready and an unsaved profile draft.
    fn ready_with_draft(ctx: &egui::Context, data: &Path, folder: &tempfile::TempDir) -> Shell {
        let mut shell = launch(ctx, Some(checkout(folder)), data);
        shell.updates = ready_updates();
        let key = crate::profile::application::BlockKey {
            file: crate::profile::application::ProfileFile::Candidate,
            level: 2,
            heading: "Education".to_owned(),
            parent: None,
            nth: 0,
        };
        let editor = shell.profile_editor.as_mut().unwrap();
        editor.open(key, "## Education\nMSc".to_owned());
        *editor.draft_mut().unwrap() = "## Education\nPhD".to_owned();
        shell
    }

    fn restart(ctx: &egui::Context, shell: &mut Shell) -> Vec<egui::ViewportCommand> {
        commands(ctx.run_ui(egui::RawInput::default(), |ui| shell.restart_to_update(ui.ctx())))
    }

    fn slot(shell: &Shell) -> Option<(crate::updates::application::RestartPlan, Vec<OsString>)> {
        shell.restart_slot.lock().unwrap().clone()
    }

    /// Restart to update goes through the normal quit: an unsaved draft asks first, and Keep editing keeps the window
    /// and applies nothing when it later closes.
    #[test]
    fn restart_to_update_asks_about_a_draft_and_keep_editing_applies_nothing() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = ready_with_draft(&ctx, data.path(), &folder);
        assert!(restart(&ctx, &mut shell).contains(&egui::ViewportCommand::Close));
        let sent = close_frame(&ctx, &mut shell);
        assert!(sent.contains(&egui::ViewportCommand::CancelClose));
        assert_eq!(shell.away, Some((Pending::Quit, true)), "the draft asks first");

        commands(ctx.run_ui(egui::RawInput::default(), |ui| shell.settle_away(ui.ctx(), false)));
        assert!(shell.draft_unsaved(), "Keep editing keeps the draft");
        assert!(!shell.restart_pending);
        shell.leave_restart();
        assert_eq!(slot(&shell), None, "a later quit does not update");
    }

    /// Discard goes on with the restart, and the new copy opens on the folder that is open, whatever it started with.
    #[test]
    fn discard_lets_restart_to_update_go_with_the_open_folder() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = ready_with_draft(&ctx, data.path(), &folder);
        shell.launch_args = os_args(&["--workspace", "/fictional/old-search", "--verbose"]).collect();
        restart(&ctx, &mut shell);
        close_frame(&ctx, &mut shell);
        let sent = commands(ctx.run_ui(egui::RawInput::default(), |ui| shell.settle_away(ui.ctx(), true)));
        assert!(sent.contains(&egui::ViewportCommand::Close), "Discard closes the window");
        assert!(!close_frame(&ctx, &mut shell).contains(&egui::ViewportCommand::CancelClose), "and nothing asks again");
        shell.leave_restart();
        let (plan, args) = slot(&shell).expect("the restart is left for run");
        assert_eq!(plan, shell.updates.restart_plan().unwrap());
        let open = shell.workspace.as_ref().unwrap().path().as_os_str().to_owned();
        assert_eq!(args, [OsString::from("--verbose"), OsString::from("--workspace"), open]);
    }

    /// Ready updates whose next check finds 0.2.0 withdrawn.
    fn withdrawn_updates() -> Updates {
        let withdrawn =
            crate::updates::application::Release { version: crate::updates::application::Version { major: 0, minor: 1, patch: 0 }, ..release_020() };
        ready_updates_then(vec![crate::updates::application::ServiceEvent::Checked(Ok(withdrawn))])
    }

    /// A shell whose ready 0.2.0 the next check finds withdrawn, with an unsaved draft.
    fn withdrawn_with_draft(ctx: &egui::Context, data: &Path, folder: &tempfile::TempDir) -> Shell {
        let mut shell = ready_with_draft(ctx, data, folder);
        shell.updates = withdrawn_updates();
        shell
    }

    /// A frame in which the check asked for answers, then the window's keys and close request are read, in the order
    /// `show` takes them. `close` is whether the window was asked to close in it.
    fn update_frame(ctx: &egui::Context, shell: &mut Shell, close: bool) -> Vec<egui::ViewportCommand> {
        let mut input = egui::RawInput::default();
        if close {
            input.viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
        }
        commands(ctx.run_ui(input, |ui| {
            shell.follow_updates(ui.ctx(), update_time());
            shell.handle_shortcuts_and_quit(ui.ctx());
        }))
    }

    /// A release withdrawn while the draft asks takes Restart to update with it: the ask closes and the window and the
    /// draft stay, so Discard can never quit a Jobdesk that would not open again.
    #[test]
    fn a_release_withdrawn_while_the_draft_asks_keeps_the_window() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = withdrawn_with_draft(&ctx, data.path(), &folder);
        restart(&ctx, &mut shell);
        close_frame(&ctx, &mut shell);
        assert_eq!(shell.away, Some((Pending::Quit, true)), "the draft asks first");

        shell.updates.check_now(update_time());
        update_frame(&ctx, &mut shell, false);
        assert_eq!(shell.updates.restart_plan(), None, "the download went with the release");
        assert_eq!(shell.away, None, "the ask closes, with nothing left to restart for");
        assert!(!shell.restart_pending && shell.draft_unsaved(), "the window and the draft stay");
        shell.leave_restart();
        assert_eq!(slot(&shell), None);
    }

    /// A release withdrawn after Restart to update sent its close, before the close is read, keeps the window: with
    /// nothing to apply, the close would quit a Jobdesk that does not open again. A later quit goes as usual.
    #[test]
    fn a_release_withdrawn_before_the_restart_closes_keeps_the_window() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        shell.updates = withdrawn_updates();
        assert!(restart(&ctx, &mut shell).contains(&egui::ViewportCommand::Close));

        shell.updates.check_now(update_time());
        assert!(update_frame(&ctx, &mut shell, true).contains(&egui::ViewportCommand::CancelClose), "the close stops");
        assert!(!shell.restart_pending && shell.away.is_none() && !shell.confirm_quit, "and nothing asks");
        shell.leave_restart();
        assert_eq!(slot(&shell), None);
        assert!(!close_frame(&ctx, &mut shell).contains(&egui::ViewportCommand::CancelClose), "the user's next quit goes");
    }

    /// A release withdrawn between Discard and the close keeps the window and the draft, and asks nothing: a second
    /// Discard would quit a Jobdesk that does not open again. A later quit asks about the draft as usual.
    #[test]
    fn a_release_withdrawn_after_discard_stops_the_close() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = withdrawn_with_draft(&ctx, data.path(), &folder);
        restart(&ctx, &mut shell);
        close_frame(&ctx, &mut shell);
        let sent = commands(ctx.run_ui(egui::RawInput::default(), |ui| shell.settle_away(ui.ctx(), true)));
        assert!(sent.contains(&egui::ViewportCommand::Close), "Discard closes the window");

        shell.updates.check_now(update_time());
        assert!(update_frame(&ctx, &mut shell, true).contains(&egui::ViewportCommand::CancelClose), "the close stops");
        assert_eq!(shell.away, None, "with no ask");
        assert!(!shell.restart_pending && !shell.quit_agreed && shell.draft_unsaved(), "the window and the draft stay");
        shell.leave_restart();
        assert_eq!(slot(&shell), None);
        assert!(close_frame(&ctx, &mut shell).contains(&egui::ViewportCommand::CancelClose));
        assert_eq!(shell.away, Some((Pending::Quit, true)), "the user's next quit asks about the draft");
    }

    /// A gateway that hands its sink to the test, so a session can end its turn and wait for a reply.
    struct Sinking(Arc<Mutex<Option<crate::runs::application::EventSink>>>);

    impl AgentGateway for Sinking {
        fn start(&self, _: &RunRequest, sink: EventSink) -> Box<dyn AgentSession> {
            *self.0.lock().unwrap() = Some(sink);
            Box::new(Idle)
        }
    }

    /// Restart to update waits while a session is open, working or only waiting on a reply, and says why.
    #[test]
    fn restart_to_update_waits_while_a_session_is_open() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        shell.updates = ready_updates();
        let sink = Arc::new(Mutex::new(None));
        let mut desk = RunDesk::new(Box::new(Sinking(Arc::clone(&sink))), Arc::new(|| {}));
        desk.start("/apply https://kestrel.example/jobs/2", &StartOptions { model: None, budget_usd: 5.0 }, Local::now().naive_local()).unwrap();
        shell.desk = Some(desk);
        assert_eq!(shell.restart_wait(), Some("A task is running. Restart once it finishes, so it is not cut off halfway."));
        assert!(!restart(&ctx, &mut shell).contains(&egui::ViewportCommand::Close) && !shell.restart_pending, "nothing closes");

        let ended = crate::runs::application::AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.5, duration_ms: 1_000 };
        sink.lock().unwrap().as_ref().unwrap()(ended);
        shell.desk.as_mut().unwrap().poll();
        assert_eq!(shell.restart_wait(), Some("A task waits for your reply in Tasks. End its session there, then restart."));
        assert!(!restart(&ctx, &mut shell).contains(&egui::ViewportCommand::Close) && !shell.restart_pending, "a session that waits on a reply counts too");

        shell.restart_pending = true;
        shell.leave_restart();
        assert_eq!(slot(&shell), None, "an open session never leaves a plan, whatever asked");
        shell.desk = None;
        assert_eq!(shell.restart_wait(), None);
    }

    /// A run that waits on a permission is still working, but Restart to update names what it waits for: the user's answer.
    #[test]
    fn restart_to_update_names_a_prompt_that_waits_on_the_user() {
        use crate::runs::application::{AgentEvent, PromptKind};
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        shell.updates = ready_updates();
        let sink = Arc::new(Mutex::new(None));
        let mut desk = RunDesk::new(Box::new(Sinking(Arc::clone(&sink))), Arc::new(|| {}));
        desk.set_asks_permission(true);
        desk.start("/apply https://kestrel.example/jobs/2", &StartOptions { model: None, budget_usd: 5.0 }, Local::now().naive_local()).unwrap();
        shell.desk = Some(desk);
        let kind = PromptKind::Permission { tool: "Bash".into(), detail: "ls cv".into(), can_remember: true };
        let prompt = Prompt { request_id: "p".into(), kind, input_json: "{}".into(), suggestions_json: None };
        sink.lock().unwrap().as_ref().unwrap()(AgentEvent::PromptRaised(prompt));
        shell.desk.as_mut().unwrap().poll();
        assert!(shell.desk.as_ref().unwrap().is_busy(), "the run still counts as working");
        assert_eq!(shell.restart_wait(), Some("A task waits for your answer in Tasks. Answer it, then restart once the task finishes."));
        assert!(!restart(&ctx, &mut shell).contains(&egui::ViewportCommand::Close) && !shell.restart_pending);
    }

    /// A folder update that works, as an idle one showing a download keeps working with no worker to end it.
    fn working_folder_update() -> FolderUpdate {
        use crate::folder_update::application::{Phase, Stage};
        folder_update::idle_folder_update("0.1.0").showing(Stage::Working(Phase::Downloading { received: 0 }), false)
    }

    /// While the folder updates, Restart to update waits, says why, and leaves no plan, whatever asked.
    #[test]
    fn restart_to_update_waits_while_the_folder_updates() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        shell.desk = None;
        shell.updates = ready_updates();
        shell.folder_update = working_folder_update();
        assert_eq!(shell.restart_wait(), Some(RESTART_WAITS));
        assert!(!restart(&ctx, &mut shell).contains(&egui::ViewportCommand::Close) && !shell.restart_pending, "nothing closes");
        shell.restart_pending = true;
        shell.leave_restart();
        assert_eq!(slot(&shell), None, "a folder update that works never leaves a plan");

        shell.restart_pending = false;
        shell.folder_update = folder_update::idle_folder_update("0.1.0");
        assert_eq!(shell.restart_wait(), None);
        assert!(restart(&ctx, &mut shell).contains(&egui::ViewportCommand::Close), "once it has ended, Restart goes");
    }

    /// While the folder updates, Run in the launcher starts nothing and says why. The same keys start the task once
    /// the update has ended, so the update is what held it.
    #[test]
    fn the_launcher_starts_no_task_while_the_folder_updates() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        // A gateway that starts no real process, should the guard fail.
        shell.desk = Some(RunDesk::new(Box::new(Idle), Arc::new(|| {})));
        shell.folder_update = working_folder_update();
        shell.screen = Screen::Launcher;
        let shell = Rc::new(RefCell::new(shell));
        let inner = Rc::clone(&shell);
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(install_look).build_ui(move |ui| inner.borrow_mut().show(ui));
        harness.run_steps(3);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        harness.run_steps(3);
        assert!(shell.borrow().desk.as_ref().unwrap().runs().is_empty(), "no task started");
        assert!(shell.borrow().desk.as_ref().unwrap().folder_updating());
        assert!(harness.query_by_label("Jobdesk is updating your job search folder. Run opens once it ends.").is_some(), "the form says why");

        shell.borrow_mut().folder_update = folder_update::idle_folder_update("0.1.0");
        harness.run_steps(3);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        harness.run_steps(3);
        assert_eq!(shell.borrow().desk.as_ref().unwrap().runs().len(), 1, "the same keys start it once the update has ended");
    }

    /// While the folder updates, Test connection waits and says why, and goes again once the update has ended.
    #[test]
    fn test_connection_waits_while_the_folder_updates() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        shell.desk = None;
        shell.folder_update = working_folder_update();
        shell.follow_folder_update();
        assert_eq!(shell.providers.test_gate(false), TestGate::FolderUpdating);
        assert_eq!(shell.providers.test_sign_in(false), Err("Jobdesk is updating your job search folder. Test once it ends.".to_owned()));
        shell.folder_update = folder_update::idle_folder_update("0.1.0");
        shell.follow_folder_update();
        assert_eq!(shell.providers.test_gate(false), TestGate::Ready);
    }

    /// The rollback due at an open starts only while the folder may be written: no session open, first-run setup's
    /// worker ended, and the window not closing for Restart to update. Once it starts, every task waits.
    #[test]
    fn a_folder_that_opens_rolls_back_only_while_nothing_else_may_write_it() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, Some(checkout(&folder)), data.path());
        let idle_desk = || RunDesk::new(Box::new(Idle), Arc::new(|| {}));
        shell.desk = Some(idle_desk());

        shell.restart_pending = true;
        shell.follow_folder_update();
        assert!(!shell.folder_update.is_working(), "not while the window closes for Restart to update");
        shell.restart_pending = false;

        shell.setup_stopping = Some(std::thread::spawn(|| {}));
        shell.follow_folder_update();
        assert!(!shell.folder_update.is_working(), "not while first-run setup's worker may still write");
        shell.setup_stopping = None;

        let mut open = idle_desk();
        open.start("/apply https://kestrel.example/jobs/2", &StartOptions { model: None, budget_usd: 5.0 }, Local::now().naive_local()).unwrap();
        shell.desk = Some(open);
        shell.follow_folder_update();
        assert!(!shell.folder_update.is_working(), "not while a session is open");

        shell.desk = Some(idle_desk());
        shell.follow_folder_update();
        assert!(shell.folder_update.is_working(), "the rollback starts");
        assert!(shell.desk.as_ref().unwrap().folder_updating(), "and every task waits for it");
        assert_eq!(shell.folder_wait(), Some(FOLDER_WAITS), "and so does Change folder");
    }

    /// Another Jobdesk that updates the folder holds its lock alone, as a second handle does here, so this one cannot
    /// hold it open: every writer of the folder waits as for its own update, until the hold is taken.
    #[test]
    fn every_writer_waits_while_another_jobdesk_updates_the_folder() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let path = checkout(&folder);
        fs::create_dir(path.join(".jobdesk")).unwrap();
        let other = fs::File::options().read(true).write(true).create(true).truncate(false).open(path.join(".jobdesk/.open")).unwrap();
        other.lock().unwrap();
        let mut shell = launch(&ctx, Some(path.clone()), data.path());
        // A clock that moves only when the test moves it.
        let hands = Arc::new(Mutex::new(std::time::Instant::now()));
        let clock = Arc::clone(&hands);
        let disk = folder_update::folder_update(data.path(), "0.1.0", Arc::new(|| {}));
        shell.folder_update = disk.with_clock(Arc::new(move || *clock.lock().unwrap()));
        shell.open(&ctx, &path);
        // A gateway that starts no real process, should the guard fail.
        shell.desk = Some(RunDesk::new(Box::new(Idle), Arc::new(|| {})));
        let options = StartOptions { model: None, budget_usd: 5.0 };
        // Whatever worker the open starts has ended, so only the other Jobdesk's hold can keep a writer waiting.
        shell.follow_folder_update();
        while shell.folder_update.is_working() {
            std::thread::sleep(std::time::Duration::from_millis(2));
            shell.follow_folder_update();
        }
        let start = |shell: &mut Shell| shell.desk.as_mut().unwrap().start("/apply https://kestrel.example/jobs/2", &options, Local::now().naive_local());
        assert!(start(&mut shell).is_err(), "no task starts");
        assert_eq!(shell.folder_writer(), Some(FolderWriter::FolderUpdate(FolderWait::Updating)), "Profile's Save and the portal changes wait");
        assert_eq!(shell.restart_wait(), Some(RESTART_WAITS));
        assert_eq!(shell.folder_wait(), Some(FOLDER_WAITS));
        assert_eq!(shell.portals_wait(), Some(portals::ui::FOLDER_UPDATING), "another Jobdesk truly updates it");
        assert_eq!(shell.providers.test_gate(false), TestGate::FolderUpdating);
        assert_eq!(shell.folder_update.notice(), None, "the other window's notice says how it goes");

        drop(other);
        shell.follow_folder_update();
        assert!(shell.folder_update.kept_out(), "the hold is tried again only after HOLD_RETRY");
        *hands.lock().unwrap() += HOLD_RETRY;
        shell.follow_folder_update();
        assert!(!shell.folder_update.kept_out(), "the hold is taken");
        // Then the rollback due at the open runs, and ends.
        while shell.folder_update.is_working() {
            std::thread::sleep(std::time::Duration::from_millis(2));
            shell.follow_folder_update();
        }
        assert_eq!(shell.folder_writer(), None);
        assert_eq!((shell.restart_wait(), shell.folder_wait()), (None, None));
        assert!(start(&mut shell).is_ok(), "the task starts once this Jobdesk holds the folder");
    }

    /// A stopped update whose rollback waits for another window to close the folder is looked at again every
    /// `HOLD_RETRY`, so the window asks to repaint that often even when nothing else moves it.
    #[test]
    fn a_rollback_that_waits_on_another_window_keeps_the_window_repainting() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let path = checkout(&folder);
        fs::create_dir(path.join(".jobdesk")).unwrap();
        // A stopped update's journal, and another window that has the folder open.
        fs::write(path.join(".jobdesk/update.json"), "{}").unwrap();
        let other = fs::File::options().read(true).write(true).create(true).truncate(false).open(path.join(".jobdesk/.open")).unwrap();
        other.lock_shared().unwrap();
        let mut shell = launch(&ctx, Some(path.clone()), data.path());
        shell.folder_update = folder_update::folder_update(data.path(), "0.1.0", Arc::new(|| {}));
        shell.open(&ctx, &path);
        shell.desk = Some(RunDesk::new(Box::new(Idle), Arc::new(|| {})));
        shell.follow_folder_update();
        while shell.folder_update.is_working() {
            std::thread::sleep(std::time::Duration::from_millis(2));
            shell.follow_folder_update();
        }
        let notice = shell.folder_update.notice().map(|notice| notice.words).unwrap_or_default();
        assert!(notice.contains("another Jobdesk window has it open"), "{notice}");
        assert!(!shell.folder_update.kept_out(), "this window holds the folder open");

        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        ctx.set_repaint_observer(move |info| seen.lock().unwrap().push(info.delay));
        shell.ask_retry_repaint(&ctx);
        assert!(asked.lock().unwrap().contains(&HOLD_RETRY), "{:?}", asked.lock().unwrap());
        drop(other);
    }

    /// A lock file that does not open, here a link planted where it goes, keeps every writer waiting as another
    /// Jobdesk's update does, but this Jobdesk holds nothing: each says that Jobdesk cannot lock the folder, never that
    /// it updates it. Nothing is written through the link.
    #[cfg(unix)]
    #[test]
    fn every_writer_waits_and_says_so_while_the_folder_lock_does_not_open() {
        const CANNOT_LOCK: &str =
            "Jobdesk cannot lock your job search folder, so tasks and saves wait. Open another folder, or check that this one can be written.";
        let (data, folder, outside) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let path = checkout(&folder);
        fs::create_dir(path.join(".jobdesk")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("planted"), path.join(".jobdesk/.open")).unwrap();
        let mut shell = launch(&ctx, Some(path.clone()), data.path());
        shell.folder_update = folder_update::folder_update(data.path(), "0.1.0", Arc::new(|| {}));
        shell.open(&ctx, &path);
        shell.desk = Some(RunDesk::new(Box::new(Idle), Arc::new(|| {})));
        shell.follow_folder_update();
        let options = StartOptions { model: None, budget_usd: 5.0 };
        assert!(shell.desk.as_mut().unwrap().start("/apply https://kestrel.example/jobs/2", &options, Local::now().naive_local()).is_err());
        assert_eq!(shell.folder_update.notice().map(|notice| notice.words), Some(CANNOT_LOCK.to_owned()));
        assert_eq!(shell.portals_wait(), Some(CANNOT_LOCK));
        assert_eq!(shell.providers.test_sign_in(false), Err(CANNOT_LOCK.to_owned()));
        assert_eq!(shell.folder_writer(), Some(FolderWriter::FolderUpdate(FolderWait::Unlockable)), "Profile's Save waits and says so");
        assert_eq!(shell.desk.as_ref().unwrap().folder_wait(), Some(FolderWait::Unlockable), "the launcher and the reply say so");
        assert!(!outside.path().join("planted").exists(), "nothing is written through the link");

        // This Jobdesk holds nothing, so the ways out stay open: Restart to update, and Change folder.
        assert_eq!((shell.restart_wait(), shell.folder_wait()), (None, None));
        shell.updates = ready_updates();
        assert!(restart(&ctx, &mut shell).contains(&egui::ViewportCommand::Close), "Restart to update goes");
        shell.restart_pending = false;
        let elsewhere = tempfile::tempdir().unwrap();
        let other = checkout(&elsewhere);
        shell.choose_folder_with(&ctx, || Some(other.clone()));
        assert_eq!(shell.workspace.as_ref().map(|root| root.path().to_owned()), Some(other), "Change folder opens another folder");
        assert!(!shell.folder_update.is_busy(), "where nothing waits");
    }

    /// Keep working in the quit dialog, by click or by Esc, keeps the window, so a Restart to update that waited on it is off.
    #[test]
    fn keep_working_ends_a_restart_to_update() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let data = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, None, data.path());
        shell.updates = ready_updates();
        (shell.restart_pending, shell.confirm_quit) = (true, true);
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
        commands(ctx.run_ui(input, |ui| shell.handle_shortcuts_and_quit(ui.ctx())));
        assert!(!shell.confirm_quit && !shell.restart_pending, "Esc keeps working");
        shell.leave_restart();
        assert_eq!(slot(&shell), None);

        (shell.restart_pending, shell.confirm_quit) = (true, true);
        let shell = Rc::new(RefCell::new(shell));
        let inner = Rc::clone(&shell);
        let mut harness =
            Harness::builder().with_size([800.0, 600.0]).with_context_setup(install_look).build_ui(move |ui| inner.borrow_mut().quit_dialog(ui.ctx()));
        harness.run_steps(2);
        harness.get_by_label("Keep working").click();
        harness.run_steps(2);
        let mut shell = shell.borrow_mut();
        assert!(!shell.confirm_quit && !shell.restart_pending, "a click on Keep working");
        shell.leave_restart();
        assert_eq!(slot(&shell), None);
    }

    /// Checking and downloading an update is not a run: no task starts, no run log is written, and Usage counts nothing.
    #[test]
    fn an_update_check_and_download_write_no_run_log() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ctx = egui::Context::default();
        let root = checkout(&folder);
        let mut shell = launch(&ctx, Some(root.clone()), data.path());
        let usage = || crate::runs::usage_logs(data.path().join("runs"), &root).tasks().len();
        let before = usage();
        shell.updates = ready_updates();
        shell.updates.check_now(update_time());
        shell.updates.poll(update_time());
        assert!(shell.desk.as_ref().unwrap().runs().is_empty(), "no task started");
        let logs = fs::read_dir(data.path().join("runs")).map(|entries| entries.count()).unwrap_or(0);
        assert_eq!(logs, 0, "no run log");
        assert_eq!((before, usage()), (0, 0), "Usage counts no task");
    }

    /// The switch reaches the updates at launch and when it changes, and a check that is due starts on the next frame.
    #[test]
    fn the_switch_for_the_automatic_check_reaches_the_updates() {
        let (data, _folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        settings::open_settings(data.path(), Preferences::default()).set_check_updates(false);
        let ctx = egui::Context::default();
        let mut shell = launch(&ctx, None, data.path());
        assert!(!shell.updates.view().about.auto, "the saved switch, at launch");
        shell.follow_updates(&ctx, update_time());
        assert!(!shell.updates.view().about.checking, "no check starts against the switch");
        shell.settings.set_check_updates(true);
        shell.follow_updates(&ctx, update_time());
        assert!(shell.updates.view().about.auto && shell.updates.view().about.checking, "the change reaches the updates and a check starts");
    }

    /// A Restart to update that did not install is said at the next launch, in the failed look.
    #[test]
    fn a_launch_says_why_the_last_restart_did_not_install() {
        let data = tempfile::tempdir().unwrap();
        fs::create_dir_all(data.path().join("updates")).unwrap();
        fs::write(data.path().join("updates/apply-failed"), "0.2.0 replace\n").unwrap();
        fs::write(data.path().join("updates/1-Jobdesk-v0.2.0-macos-arm64.app.tar.gz"), "fictional").unwrap();
        let ctx = egui::Context::default();
        let shell = launch(&ctx, None, data.path());
        let bar = shell.updates.view().bar.expect("the bar says it");
        assert_eq!(bar.words, "0.2.0 was not installed: the new copy could not be put in place");
        assert_eq!(bar.mark, crate::updates::application::Mark::Failed);
        assert!(!data.path().join("updates/1-Jobdesk-v0.2.0-macos-arm64.app.tar.gz").exists(), "an old download is never applied");
        assert!(!data.path().join("updates/apply-failed").exists(), "said once");
    }

    fn os_args(list: &[&str]) -> std::vec::IntoIter<OsString> {
        list.iter().map(OsString::from).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn the_flag_wins_over_the_environment() {
        let env = Some(OsString::from("/from/env"));
        assert_eq!(requested_workspace_from(os_args(&["--workspace", "/from/flag"]), env.clone()), Some(PathBuf::from("/from/flag")));
        assert_eq!(requested_workspace_from(os_args(&[]), env), Some(PathBuf::from("/from/env")));
        assert_eq!(requested_workspace_from(os_args(&["--workspace"]), None), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_utf8_arrives_whole() {
        use std::os::unix::ffi::OsStringExt;

        let raw = OsString::from_vec(vec![b'/', b'w', 0xFF, b'x']);
        let from_flag = vec![OsString::from("--workspace"), raw.clone()].into_iter();
        assert_eq!(requested_workspace_from(from_flag, None), Some(PathBuf::from(raw.clone())));
        assert_eq!(requested_workspace_from(os_args(&[]), Some(raw.clone())), Some(PathBuf::from(raw)));
    }

    /// The first window lowers the smallest size before it shrinks, so a saved large window never holds it up.
    #[test]
    fn first_run_asks_for_the_small_window_whatever_was_saved() {
        use egui::ViewportCommand::{InnerSize, MinInnerSize};
        let small = vec2(720.0, 560.0);
        assert_eq!(window_commands(WindowSize::FirstRun, Some(vec2(1728.0, 1117.0)), None, None), [MinInnerSize(small), InnerSize(small)]);
    }

    /// A window at `min` whose title bar is 28 points tall over `inner`.
    fn framed(min: egui::Pos2, inner: egui::Vec2) -> (Rect, Rect) {
        (Rect::from_min_size(min, inner + vec2(0.0, 28.0)), Rect::from_min_size(min + vec2(0.0, 28.0), inner))
    }

    /// Handing over grows the window to the app's size round its old centre, title bar included, or to the screen when
    /// that is smaller, but never below the app's smallest.
    #[test]
    fn handing_over_grows_the_window_to_the_app_on_the_screen() {
        use egui::ViewportCommand::{InnerSize, MinInnerSize, OuterPosition};
        let smallest = vec2(1040.0, 640.0);
        let (outer, inner) = framed(pos2(400.0, 200.0), vec2(720.0, 560.0));
        let grown = window_commands(WindowSize::App, Some(vec2(1728.0, 1117.0)), Some(outer), Some(inner));
        assert_eq!(grown, [InnerSize(vec2(1280.0, 820.0)), MinInnerSize(smallest), OuterPosition(pos2(120.0, 70.0))]);
        let laptop = window_commands(WindowSize::App, Some(vec2(1180.0, 700.0)), None, None);
        assert_eq!(laptop, [InnerSize(vec2(1180.0, 700.0)), MinInnerSize(smallest)], "the screen, when smaller");
        let tiny = window_commands(WindowSize::App, Some(vec2(800.0, 600.0)), None, None);
        assert_eq!(tiny, [InnerSize(smallest), MinInnerSize(smallest)], "never below the smallest");
    }

    /// Growing the window for Settings and shrinking it again puts it back where it was, however often it is done.
    #[test]
    fn growing_and_shrinking_the_window_leaves_it_where_it_was() {
        use egui::ViewportCommand::{InnerSize, OuterPosition};
        let start = pos2(400.0, 200.0);
        let (mut outer, mut inner) = framed(start, vec2(720.0, 560.0));
        for size in [WindowSize::App, WindowSize::FirstRun, WindowSize::App, WindowSize::FirstRun] {
            let sent = window_commands(size, Some(vec2(1728.0, 1117.0)), Some(outer), Some(inner));
            let to = sent.iter().find_map(|command| if let InnerSize(to) = command { Some(*to) } else { None }).unwrap();
            let Some(OuterPosition(at)) = sent.last() else { panic!("no position: {sent:?}") };
            // The window takes what it was asked.
            (outer, inner) = framed(*at, to);
        }
        assert_eq!(outer.min, start);
    }
}
