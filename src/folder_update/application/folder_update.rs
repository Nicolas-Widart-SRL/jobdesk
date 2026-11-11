use std::collections::HashSet;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::folder_update::application::notice::{self, Notice, Quiet};
use crate::folder_update::domain::{Base, BaseReader, FolderProbe, FolderUpdater, Release, Report, Reverted, Update, UpdateError, ZipFetcher};
use crate::shared_kernel::FolderWait;
use crate::shared_kernel::download::DownloadFailure;
use crate::shared_kernel::installed_source::InstalledSource;
use crate::shared_kernel::release_zip::ZipFailure;
use crate::updates::application::{LatestFeed, Version, source_zip_name, source_zip_url};

/// How often `follow` tries again to hold the open folder while another Jobdesk holds it alone. The window repaints at
/// least this often meanwhile, so the try comes even with no input.
pub const HOLD_RETRY: Duration = Duration::from_secs(1);

/// What tells `FolderUpdate` the time, so a test moves it on at will. The system's monotonic clock by default.
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// Where an update or an undo is while its worker runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// An update that stopped midway is rolled back, if one did. That is all `recover` does. An update pass then asks
    /// the gate whether an update is due, and goes on to `Downloading` only when it is.
    Checking,
    /// So many bytes of the pinned release have arrived. GitHub does not say the size first.
    Downloading { received: u64 },
    /// The release is checked against the pin's SHA-256, then merged into the folder.
    Updating,
    /// The last update is undone.
    Undoing,
}

/// Why an update or an undo did not finish. No server text is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// No internet, a timeout, or a download cut short.
    Offline,
    /// GitHub answered the download with this status code.
    Refused(u16),
    /// The download is not the release the signed feed pins, or holds an entry no folder may take. Nothing changed.
    Mismatch,
    /// The download grew past the largest release Jobdesk takes, a limit of Jobdesk's own, and was stopped and deleted.
    /// Nothing changed.
    TooLarge,
    /// A file could not be read or written. An update that failed after it changed files was rolled back, or the next
    /// open rolls it back. An undo that failed may have put back some of the files.
    Disk,
    /// An update stopped midway and the rest of the folder was rolled back, but these files stay as they are: each
    /// changed after the update wrote it, or its original could not be put back because the update's backup was gone.
    Kept(Vec<String>),
    /// An update stopped midway, at an earlier open, and its record does not read, so nothing could be rolled back:
    /// the folder may hold part of that update. The record is set aside, and the next update runs.
    Stopped,
    /// Undo could not note the release it would undo, which keeps it from being applied again, so it undid nothing.
    Unnoted,
    /// A task or a Claude session is open, another Jobdesk works on this folder, or a file changed while the update
    /// ran. Nothing changed, and the next open tries again.
    Busy,
    /// An update stopped midway, and its rollback waits until no other Jobdesk window has the folder open. Nothing
    /// changed, and the rollback runs once the folder can be held alone.
    OpenElsewhere,
}

/// How one pass of a folder update ended. Each gate that fails is its own outcome, so the screen says the right thing
/// or nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// An update that stopped midway, at an earlier open, was rolled back. Nothing else ran, and no pass updates the
    /// folder for the rest of this launch: the next launch does.
    Recovered(Reverted),
    /// The folder is not there.
    NoFolder,
    /// The folder holds a `.git` entry: the user keeps it under git, and git updates it.
    GitFolder,
    /// The folder keeps no base: it was set up before Jobdesk kept one, or it is a folder the user already had.
    NoBase,
    /// The folder's base is there but does not read or does not match its record.
    BadBase,
    /// This Jobdesk is not the latest the feed names, so the release it pins is not this Jobdesk's.
    JobdeskNotLatest,
    /// The feed pins nothing this Jobdesk can trust: a build with no update key, a feed with no pin, or a pin whose
    /// signature fails.
    NoPin,
    /// The pinned release is not newer than the folder's base. An older pin, even a signed one, is never applied.
    UpToDate,
    /// The user undid the update to this very release, so it is not applied again. A newer release updates the folder.
    Skipped,
    /// The folder is updated, and this is what changed.
    Updated(Report),
    /// The update did not finish. The folder is as the update left it, and the next open tries again. After `Kept`, no
    /// pass updates the folder for the rest of this launch: the backup holds the originals of the files kept.
    Failed(Failure),
    /// The rollback of an update that stopped midway, at an earlier open, did not finish, so no update ran. Unless it was
    /// `Busy`, which ran nothing, no pass updates the folder for the rest of this launch either.
    RollbackFailed(Failure),
    /// An Undo of the last update started, at an earlier open, and did not finish: the folder holds some files of each
    /// release. Undo finishes it, and the release it noted is not applied again meanwhile.
    UndoUnfinished,
    /// An update pass found an Undo that did not finish, so it updated nothing, whatever the pin: an update now would
    /// take the leftover files for the user's and replace the backup Undo needs. The open's notice already says so.
    UndoPending,
}

/// How an undo ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    /// The last update is undone. Files changed since it stay as they are, in `kept`.
    Undone(Reverted),
    /// No update is recorded, its backup has gone, or its record does not read. Nothing changed.
    NothingToUndo,
    Failed(Failure),
}

/// What the screens read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Working(Phase),
    /// How the last update pass ended.
    Ended(Outcome),
    /// How the last undo ended.
    Undid(UndoOutcome),
}

/// What the worker tells the window.
enum Event {
    Phase(Phase),
    /// What the rollback at an open found of the folder.
    Looked(Look),
    /// The version of the folder's base as the worker read it, at the open's look or once a pass or an undo ended.
    /// `None` when the folder keeps no base, or one that does not read.
    Base(Option<Version>),
    /// The stage it ended at, and whether the last update can be undone now.
    Ended(Stage, bool),
}

/// What the rollback at an open found of the folder, so About can say what a check would do, before any feed is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    /// Jobdesk leaves the folder as it is, and this is why.
    Quiet(Quiet),
    /// The folder has a base and no git, so a check that finds a newer release updates it.
    Updatable,
}

/// The ports one job needs on its worker thread.
#[derive(Clone)]
struct Ports {
    probe: Arc<dyn FolderProbe>,
    base: Arc<dyn BaseReader>,
    fetcher: Arc<dyn ZipFetcher>,
    updater: Arc<dyn FolderUpdater>,
}

/// Updates a job search folder to the `ai-job-search` release the signed feed pins, when the feed names this Jobdesk as
/// the latest, and undoes the last update. One worker at a time does the network and disk work, away from the window's
/// thread. The UI reads `notice` and `quiet`, and acts through the methods. `recover` runs at every open of the
/// folder, so an update that stopped midway is rolled back even when no feed is read; `start` runs it first too.
/// `opened` and `follow` drive both for the window: the rollback once per open, then an update once per folder and
/// answer of the feed.
///
/// The run lock is the caller's: an update, an undo and the rollback before each write the folder, so none may run
/// while a task or a Claude session is open, even one only waiting on a reply. Each entry point that starts a worker
/// takes `runs_idle`, and refuses when it is false. While `is_working` holds, the caller starts no run, applies no
/// Jobdesk update and opens no other folder.
pub struct FolderUpdate {
    ports: Ports,
    /// This Jobdesk's version. `None` when it does not read, and then no feed names it as the latest.
    current: Option<Version>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    /// The time, for the retries: see `with_clock`.
    clock: Clock,
    stage: Stage,
    working: bool,
    undoable: bool,
    events: Receiver<Event>,
    sender: Sender<Event>,
    /// The job search folder open now, as `opened` named it.
    folder: Option<PathBuf>,
    /// The open folder's rollback has not run yet.
    recover_due: bool,
    /// Each folder and answer of the feed an update pass started with, this launch, so each is tried once.
    tried: HashSet<(PathBuf, u64)>,
    /// The folder the worker runs on, while one does.
    working_on: Option<PathBuf>,
    /// This Jobdesk's last try to hold the open folder, made at this moment, could not, and why: another Jobdesk held it
    /// alone, to update, undo or roll it back, or its lock did not open or lock. While it is set, nothing writes the
    /// folder, as while this Jobdesk's own worker runs.
    kept_out: Option<(Instant, FolderWait)>,
    /// A stopped update's rollback of the open folder waits for another Jobdesk window to close it, as the last try,
    /// made at this moment, found. `follow` tries again every `HOLD_RETRY`, once the folder can be held alone.
    rollback_waits: Option<Instant>,
    /// Each folder whose rollback ran or failed this launch, at an open or in a pass, or whose update failed and kept
    /// files in its own rollback. No pass starts for it again until the next launch, so the notice stays, and so does
    /// the backup that holds the originals of any file kept, which an update would replace.
    held_back: HashSet<PathBuf>,
    /// The last stage worth telling, which the notice says until a later one, a dismissal or another folder.
    told: Option<Stage>,
    /// Undo is offered: an update of the open folder ended this launch and no task has started since, or an Undo that
    /// did not finish was found, or a failed Undo of either left the record to retry.
    undo_offered: bool,
    /// The offer finishes an Undo that did not finish, so a task does not end it: finishing puts back only files that
    /// still hold what the update wrote, and keeps a task's edits.
    offer_outlives_tasks: bool,
    /// Why the open folder stays as it is, as the rollback at its open or the last pass that asked the gate found.
    quiet: Option<Quiet>,
    /// The rollback at the open found a folder a check would update.
    updatable: bool,
    /// A pass has asked the gate for the open folder, so a check has already done what it can.
    checked: bool,
    /// The version of the open folder's base, as the worker last read it: at the open's look, and again once each
    /// update pass or undo ended. `None` until then, and when the folder keeps no base or one that does not read.
    base: Option<Version>,
}

impl FolderUpdate {
    /// `current` is the version this Jobdesk runs, as `CARGO_PKG_VERSION` gives it, and `repaint` wakes the window when
    /// the worker has news. It writes nothing, so it may run while a task is open. It leaves the downloads folder alone,
    /// where another Jobdesk may be making its own folder at that moment: setup clears what a stopped one left.
    pub fn new(
        probe: Arc<dyn FolderProbe>,
        base: Arc<dyn BaseReader>,
        fetcher: Arc<dyn ZipFetcher>,
        updater: Arc<dyn FolderUpdater>,
        current: &str,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, events) = mpsc::channel();
        Self {
            ports: Ports { probe, base, fetcher, updater },
            current: Version::parse(current),
            repaint,
            clock: Arc::new(Instant::now),
            stage: Stage::Idle,
            working: false,
            undoable: false,
            events,
            sender,
            folder: None,
            recover_due: false,
            tried: HashSet::new(),
            working_on: None,
            kept_out: None,
            rollback_waits: None,
            held_back: HashSet::new(),
            told: None,
            undo_offered: false,
            offer_outlives_tasks: false,
            quiet: None,
            updatable: false,
            checked: false,
            base: None,
        }
    }

    /// Shows `stage` as though a worker had just reached it, for tests and previews. A `Working` stage keeps working,
    /// with no worker to end it. `undoable` is whether the last update can be undone, so an update shown with it offers
    /// Undo.
    pub fn showing(mut self, stage: Stage, undoable: bool) -> Self {
        self.working = matches!(stage, Stage::Working(_));
        self.undoable = undoable;
        self.remember(&stage);
        self.stage = stage;
        self
    }

    /// Reads the time from `clock` instead of the system's, for tests: a retry waits on it alone, never on how long a
    /// test takes to run.
    pub fn with_clock(self, clock: Clock) -> Self {
        Self { clock, ..self }
    }

    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    /// `folder` opened as the job search folder: this Jobdesk holds it open until the next open, so no other Jobdesk
    /// updates, undoes or rolls it back meanwhile, its rollback is due, and what the notice, Undo and About said was of
    /// the last folder, so it goes. Call it at every open, a launch's, a switch's and first-run setup's hand-over, and
    /// open no folder while `is_working`.
    pub fn opened(&mut self, folder: PathBuf) {
        self.kept_out = self.ports.updater.open(&folder).map(|wait| ((self.clock)(), wait));
        self.folder = Some(folder);
        (self.recover_due, self.rollback_waits) = (true, None);
        (self.told, self.undo_offered, self.undoable, self.quiet) = (None, false, false, None);
        self.offer_outlives_tasks = false;
        (self.updatable, self.checked, self.base) = (false, false, None);
    }

    /// Call it every frame. It takes the worker's news, then, while no worker runs and `runs_idle` holds, starts the
    /// open folder's rollback once after it opened, or else an update pass once for each answer of the feed, unless a
    /// rollback of the folder ran or failed this launch, at an open or in a pass, or an update of it failed and kept
    /// files: `feed` is how many checks have read the feed, and what the last one found. `runs_idle` is false while a
    /// task or a Claude session is open, even one only waiting on a reply, and while anything else may write the folder.
    ///
    /// While another Jobdesk holds the open folder alone, it tries to hold the folder again every `HOLD_RETRY`, and
    /// starts nothing until it does: see `is_busy`. While a stopped update's rollback waits for another Jobdesk window
    /// to close the folder, it looks every `HOLD_RETRY` whether the folder can be held alone, starts nothing else, and
    /// starts the rollback once it can.
    pub fn follow(&mut self, feed: Option<(u64, &LatestFeed)>, runs_idle: bool) {
        self.poll();
        let now = (self.clock)();
        if let (Some((tried, _)), Some(folder), false) = (self.kept_out, &self.folder, self.working)
            && now.saturating_duration_since(tried) >= HOLD_RETRY
        {
            self.kept_out = self.ports.updater.hold_again(folder).map(|wait| (now, wait));
        }
        if self.working || !runs_idle || self.kept_out.is_some() {
            return;
        }
        let Some(folder) = self.folder.clone() else { return };
        if let Some(tried) = self.rollback_waits {
            if now.saturating_duration_since(tried) < HOLD_RETRY {
                return;
            }
            // A look that lets go of the hold for a moment, not a worker: nothing reads as busy while the wait lasts.
            // Another Jobdesk may take the folder alone in that moment, so what taking the hold back found is kept, and
            // every writer waits while this one is kept out.
            let look = self.ports.updater.alone_possible(&folder);
            self.kept_out = look.kept_out.map(|wait| (now, wait));
            if !look.alone || self.kept_out.is_some() {
                self.rollback_waits = Some(now);
                return;
            }
            (self.rollback_waits, self.recover_due) = (None, true);
        }
        if self.recover_due {
            self.recover_due = !self.recover(folder, true);
            return;
        }
        if self.held_back.contains(&folder) {
            return;
        }
        if let Some((answer, feed)) = feed
            && self.tried.insert((folder.clone(), answer))
        {
            self.start(folder, feed, true);
        }
    }

    /// A task started, so the Undo right after an update is offered no more: the task may have built on what the update
    /// wrote. The Undo that finishes an unfinished one stays, for this launch.
    pub fn task_started(&mut self) {
        if !self.offer_outlives_tasks {
            self.undo_offered = false;
        }
    }

    /// Puts the notice away, Undo with it, until a later stage is worth telling. Ignored while a worker runs.
    pub fn dismiss(&mut self) {
        if !self.working {
            (self.told, self.undo_offered) = (None, false);
        }
    }

    /// What the status bar says of the folder update, if anything. While this Jobdesk cannot lock the folder, and no
    /// update holds it, that comes first: the writers wait, and nothing else will end the wait.
    pub fn notice(&self) -> Option<Notice> {
        if self.folder_wait() == Some(FolderWait::Unlockable) {
            return Some(notice::unlockable());
        }
        notice::notice(&self.stage, self.told.as_ref(), self.undo_offered && self.can_undo(), self.current)
    }

    /// The one quiet line Settings › About shows of the open folder, when Jobdesk leaves it as it is.
    pub fn quiet(&self) -> Option<&'static str> {
        self.quiet.map(Quiet::words)
    }

    /// The one quiet line Settings › About shows of the open folder: why Jobdesk leaves it as it is, or, while the
    /// automatic check is off, `auto_check` false, that the folder is brought up to date only after a check. That line
    /// shows for a folder with a base and no git, until a pass has asked the gate, and never after a rollback that ran or
    /// failed, or an update that failed and kept files, since no pass updates that folder for the rest of the launch.
    pub fn about_line(&self, auto_check: bool) -> Option<&'static str> {
        let held_back = self.folder.as_ref().is_some_and(|folder| self.held_back.contains(folder));
        self.quiet().or_else(|| (!auto_check && self.updatable && !self.checked && !held_back).then_some(notice::CHECK_HINT))
    }

    /// The line Settings › About shows when the open folder's files come from a release that `feed`, the answer of the
    /// last check that read it, no longer lists as compatible. Nothing when no check has read the feed, when its signed
    /// list is empty, or when the folder's base was not read or does not read: nothing can be said then.
    pub fn support_line(&self, feed: Option<&LatestFeed>) -> Option<&'static str> {
        let (base, feed) = (self.base?, feed?);
        (feed.supports(base) == Some(false)).then_some(notice::UNSUPPORTED)
    }

    /// Undoes the update the notice offers Undo for, of the open folder, as `undo` does. Ignored when none is offered.
    /// While another Jobdesk holds the folder alone, it is refused as while a task is open.
    pub fn undo_update(&mut self, runs_idle: bool) {
        if let (true, Some(folder)) = (self.undo_offered && self.can_undo(), self.folder.clone()) {
            self.undo(folder, runs_idle && self.kept_out.is_none());
        }
    }

    /// Takes what a worker or a refusal ended at: what it says of the folder's state for About, and the stage itself when
    /// it is worth telling.
    fn remember(&mut self, stage: &Stage) {
        if let Stage::Ended(outcome) = stage
            && let Some(quiet) = notice::quiet_after(outcome)
        {
            (self.quiet, self.checked) = (quiet, true);
        }
        // A rollback that failed, or an Undo that did not finish, has its own line in the notice, and no check updates
        // the folder meanwhile, so About asks for none until the next open looks.
        if matches!(stage, Stage::Ended(Outcome::RollbackFailed(_) | Outcome::UndoUnfinished | Outcome::UndoPending)) {
            self.updatable = false;
        }
        if notice::told(stage) {
            self.told = Some(stage.clone());
            // A failed Undo leaves the update's record, so Undo stays offered while it can run: a retry puts back again
            // only what is not back yet.
            let undo_failed = matches!(stage, Stage::Undid(UndoOutcome::Failed(_)));
            let offers = matches!(stage, Stage::Ended(Outcome::Updated(_) | Outcome::UndoUnfinished));
            self.undo_offered = offers || (undo_failed && self.undo_offered);
            if offers {
                self.offer_outlives_tasks = matches!(stage, Stage::Ended(Outcome::UndoUnfinished));
            }
        }
    }

    /// Ends at `stage` with no worker, as a refusal does.
    fn refuse(&mut self, stage: Stage) {
        self.remember(&stage);
        self.stage = stage;
    }

    /// Whether a worker is still running, so the window keeps polling and the caller starts no run.
    pub fn is_working(&self) -> bool {
        self.working
    }

    /// Whether this Jobdesk could not hold the open folder when it last tried: another Jobdesk held it alone, or its lock
    /// did not open or lock. `follow` tries again every `HOLD_RETRY`, so the window should repaint at least that often
    /// meanwhile.
    pub fn kept_out(&self) -> bool {
        self.kept_out.is_some()
    }

    /// Whether `follow` tries something again every `HOLD_RETRY`: the hold of a folder this Jobdesk is kept out of, or a
    /// rollback that waits for another Jobdesk window to close the folder. The window should repaint at least that
    /// often meanwhile, even with no input.
    pub fn retrying(&self) -> bool {
        self.kept_out.is_some() || self.rollback_waits.is_some()
    }

    /// Why the folder's writers wait on the folder update, if they do: `Updating` while this Jobdesk's worker, or
    /// another Jobdesk that holds the folder alone, updates, undoes or rolls it back, and `Unlockable` while this
    /// Jobdesk cannot lock the folder or look at its `.jobdesk` for any other reason, when this Jobdesk holds nothing.
    /// On either, the caller starts no task and sends no reply, and every other writer of the folder and Test
    /// connection wait, each saying why. Restart to update and Change folder wait only on `Updating`: on `Unlockable`
    /// they are the ways out.
    pub fn folder_wait(&self) -> Option<FolderWait> {
        if self.working { Some(FolderWait::Updating) } else { self.kept_out.map(|(_, wait)| wait) }
    }

    /// Whether the folder's writers wait on the folder update: see `folder_wait`.
    pub fn is_busy(&self) -> bool {
        self.folder_wait().is_some()
    }

    /// Whether the last update of the folder can be undone, as the last update pass or undo found it on its worker.
    /// `false` while one works, and before the first has ended.
    pub fn can_undo(&self) -> bool {
        !self.working && self.undoable
    }

    /// Updates `folder` when every gate holds, from `feed`, the answer of the last updates check, on a worker. It first
    /// rolls back an update that stopped midway, if one did. Ignored while a worker runs.
    ///
    /// Call it only while no task and no Claude session is open, and pass that as `runs_idle`: when it is false,
    /// nothing runs and the stage is `Ended(Failed(Busy))`. Start no run until `is_working` is false.
    pub fn start(&mut self, folder: PathBuf, feed: &LatestFeed, runs_idle: bool) {
        if self.working {
            return;
        }
        if !runs_idle {
            self.refuse(Stage::Ended(Outcome::Failed(Failure::Busy)));
            return;
        }
        let (feed, current) = (feed.clone(), self.current);
        self.working_on = Some(folder.clone());
        self.spawn(Phase::Checking, Stage::Ended(Outcome::Failed(Failure::Disk)), move |ports, tell| {
            let outcome = run(ports, &folder, &feed, current, &mut |phase| tell(Event::Phase(phase)));
            tell(Event::Base(base_version(ports, &folder)));
            (Stage::Ended(outcome), ports.updater.can_undo(&folder))
        });
    }

    /// Rolls back an update of `folder` that stopped midway, on a worker, whatever the feed says or whether it was read:
    /// call it at every open of the folder. It ends at `Ended(Recovered)` when it rolled one back, at
    /// `Ended(RollbackFailed)` when it could not, and at `Idle` when no update had stopped. Returns whether its worker started.
    ///
    /// Call it only while no task and no Claude session is open, and pass that as `runs_idle`. When it is false, or a
    /// worker runs, nothing starts, the stage stays as it was, and it returns false, so the caller asks again later: at
    /// an open no update was tried, so there is no failure to show. Start no run until `is_working` is false.
    #[must_use]
    pub fn recover(&mut self, folder: PathBuf, runs_idle: bool) -> bool {
        if self.working || !runs_idle {
            return false;
        }
        self.working_on = Some(folder.clone());
        self.spawn(Phase::Checking, Stage::Ended(Outcome::Failed(Failure::Disk)), move |ports, tell| {
            // An Undo that stopped partway leaves no journal to roll back, only its record and its note, which a look
            // that changes nothing finds.
            let unfinished = || ports.updater.undo_unfinished(&folder).then_some(Outcome::UndoUnfinished);
            let stage = recover_stopped(ports, &folder).or_else(unfinished).map_or(Stage::Idle, Stage::Ended);
            let (look, base) = look_at(ports, &folder);
            if let Some(look) = look {
                tell(Event::Looked(look));
            }
            tell(Event::Base(base));
            (stage, ports.updater.can_undo(&folder))
        });
        true
    }

    /// Undoes the last update of `folder` on a worker. Ignored while a worker runs.
    ///
    /// Call it only while no task and no Claude session is open, and pass that as `runs_idle`: when it is false,
    /// nothing runs and the stage is `Undid(Failed(Busy))`. Start no run until `is_working` is false.
    pub fn undo(&mut self, folder: PathBuf, runs_idle: bool) {
        if self.working {
            return;
        }
        if !runs_idle {
            self.refuse(Stage::Undid(UndoOutcome::Failed(Failure::Busy)));
            return;
        }
        self.spawn(Phase::Undoing, Stage::Undid(UndoOutcome::Failed(Failure::Disk)), move |ports, tell| {
            let outcome = undo(ports, &folder);
            tell(Event::Base(base_version(ports, &folder)));
            (Stage::Undid(outcome), ports.updater.can_undo(&folder))
        });
    }

    /// Takes what the worker said since the last call. Call it every frame while `is_working`.
    pub fn poll(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Phase(phase) => self.stage = Stage::Working(phase),
                Event::Looked(Look::Quiet(quiet)) => (self.quiet, self.updatable) = (Some(quiet), false),
                Event::Looked(Look::Updatable) => (self.quiet, self.updatable) = (None, true),
                Event::Base(base) => self.base = base,
                Event::Ended(stage, undoable) => {
                    let folder = self.working_on.take();
                    // A rollback another Jobdesk kept out ran nothing, so it holds nothing back. One that waits for
                    // another window to close the folder is tried again until it runs. An update whose own rollback
                    // kept files left their originals only in its backup, as a rollback at an open that kept files does.
                    let kept_out = matches!(&stage, Stage::Ended(Outcome::RollbackFailed(Failure::Busy | Failure::OpenElsewhere)));
                    let rolled_back = matches!(&stage, Stage::Ended(Outcome::Recovered(_) | Outcome::RollbackFailed(_))) && !kept_out;
                    let kept_files = matches!(&stage, Stage::Ended(Outcome::Failed(Failure::Kept(_))));
                    if let (true, Some(folder)) = (rolled_back || kept_files, folder) {
                        self.held_back.insert(folder);
                    }
                    if stage == Stage::Ended(Outcome::RollbackFailed(Failure::OpenElsewhere)) {
                        self.rollback_waits = Some((self.clock)());
                    }
                    self.remember(&stage);
                    self.stage = stage;
                    self.undoable = undoable;
                    self.working = false;
                    // The worker let go of this Jobdesk's hold while it ran, and another Jobdesk may hold the folder
                    // alone now, before this one took the hold again.
                    if let Some(folder) = &self.folder {
                        self.kept_out = self.ports.updater.hold_again(folder).map(|wait| ((self.clock)(), wait));
                    }
                }
            }
        }
    }

    /// Runs `job` on a worker. It always ends with an `Ended` event, so `working` never stays true: a job that panics
    /// ends at `panicked`, with nothing to undo, since what it left is not known.
    fn spawn(&mut self, first: Phase, panicked: Stage, job: impl FnOnce(&Ports, &dyn Fn(Event)) -> (Stage, bool) + Send + 'static) {
        self.working = true;
        self.stage = Stage::Working(first);
        let (ports, sender, repaint) = (self.ports.clone(), self.sender.clone(), Arc::clone(&self.repaint));
        thread::spawn(move || {
            let tell = |event: Event| {
                let _ = sender.send(event);
                repaint();
            };
            // The ports hold no state of a job: a later one reads the folder afresh, so a panic leaves nothing half
            // changed in memory that it would trust. What it left on the disk is the next open's rollback to handle.
            let (stage, undoable) = panic::catch_unwind(AssertUnwindSafe(|| job(&ports, &tell))).unwrap_or((panicked, false));
            tell(Event::Ended(stage, undoable));
        });
    }
}

/// One pass: the rollback of a stopped update, the gate, the download of the pinned release and the update.
fn run(ports: &Ports, folder: &Path, feed: &LatestFeed, current: Option<Version>, phase: &mut dyn FnMut(Phase)) -> Outcome {
    phase(Phase::Checking);
    if let Some(outcome) = recover_stopped(ports, folder) {
        return outcome;
    }
    // An update over an unfinished Undo would take its leftover files for the user's, and its backup would replace the
    // one the Undo needs, so none runs until the Undo has finished, whatever the pin.
    if ports.updater.undo_unfinished(folder) {
        return Outcome::UndoPending;
    }
    if !ports.probe.exists(folder) {
        return Outcome::NoFolder;
    }
    if ports.probe.has_git(folder) {
        return Outcome::GitFolder;
    }
    let (source, base_zip) = match ports.base.read(folder) {
        Base::Missing => return Outcome::NoBase,
        Base::Bad => return Outcome::BadBase,
        Base::Found { source, zip } => (source, zip),
    };
    let Some(base_version) = Version::parse(&source.version) else { return Outcome::BadBase };
    if current != Some(feed.latest()) {
        return Outcome::JobdeskNotLatest;
    }
    let Some(pin) = feed.pin() else { return Outcome::NoPin };
    // A signed pin of an older or the same release is refused, so an old feed replayed never takes a folder back.
    if pin.version <= base_version {
        return Outcome::UpToDate;
    }
    // The release the user undid is not applied again. The note names one release, and only that very one, version and
    // hash alike, is skipped: a note written by anything else can skip no other release and makes nothing apply.
    let undone = |note: InstalledSource| Version::parse(&note.version) == Some(pin.version) && note.sha256 == pin.sha256;
    if ports.base.skipped(folder).is_some_and(undone) {
        return Outcome::Skipped;
    }
    phase(Phase::Downloading { received: 0 });
    let name = format!("update-{}", source_zip_name(&pin.version));
    let zip = match ports.fetcher.fetch(&source_zip_url(&pin.version), &name, &mut |received| phase(Phase::Downloading { received })) {
        Ok(zip) => zip,
        Err(failure) => return Outcome::Failed(download_failure(failure)),
    };
    phase(Phase::Updating);
    let version = pin.version.to_string();
    // What the gate read of the base goes with it, so the update changes nothing when another Jobdesk moved it since.
    let base = Release { zip: &base_zip, sha256: &source.sha256 };
    let update = Update { base, base_version: &source.version, new: Release { zip: &zip, sha256: &pin.sha256 }, version: &version };
    match ports.updater.update(folder, &update) {
        Ok(report) => Outcome::Updated(report),
        Err(UpdateError::Superseded) => Outcome::UpToDate,
        Err(UpdateError::Undone) => Outcome::Skipped,
        Err(UpdateError::NoBase) => Outcome::NoBase,
        Err(UpdateError::BadBase(_)) => Outcome::BadBase,
        Err(error) => Outcome::Failed(failure_of(error)),
    }
}

/// The rollback of an update of `folder` that stopped midway, and how it ended. `None` when none had stopped. A
/// rollback that ran ends the pass, so the user hears of it before the next open updates.
fn recover_stopped(ports: &Ports, folder: &Path) -> Option<Outcome> {
    match ports.updater.recover(folder) {
        Ok(None) => None,
        Ok(Some(reverted)) => Some(Outcome::Recovered(reverted)),
        Err(error) => Some(Outcome::RollbackFailed(failure_of(error))),
    }
}

/// What the gate would find of `folder` before it asks the feed: why Jobdesk leaves it as it is, or that a check would
/// update it, and the version of its base, for About to say whether the feed still lists it. A folder under git has its
/// base read too, since its files may still be the ones setup unpacked. `(None, None)` when it is not there. It reads
/// only the folder.
fn look_at(ports: &Ports, folder: &Path) -> (Option<Look>, Option<Version>) {
    if !ports.probe.exists(folder) {
        return (None, None);
    }
    let git = ports.probe.has_git(folder);
    let base = ports.base.read(folder);
    let version = base_of(&base);
    let look = match base {
        _ if git => Look::Quiet(Quiet::GitFolder),
        Base::Missing => Look::Quiet(Quiet::NoBase),
        Base::Found { .. } if version.is_some() => Look::Updatable,
        Base::Bad | Base::Found { .. } => Look::Quiet(Quiet::BadBase),
    };
    (Some(look), version)
}

/// The version of `folder`'s base, as the worker reads it once a pass or an undo ended. `None` when it keeps none, or
/// one that does not read.
fn base_version(ports: &Ports, folder: &Path) -> Option<Version> {
    base_of(&ports.base.read(folder))
}

fn base_of(base: &Base) -> Option<Version> {
    match base {
        Base::Found { source, .. } => Version::parse(&source.version),
        Base::Missing | Base::Bad => None,
    }
}

fn undo(ports: &Ports, folder: &Path) -> UndoOutcome {
    match ports.updater.undo(folder) {
        Ok(reverted) => UndoOutcome::Undone(reverted),
        // Undo reads the record of the last update before it changes anything, so one that does not read changed nothing.
        Err(UpdateError::NothingToUndo | UpdateError::NoBase | UpdateError::BadJournal) => UndoOutcome::NothingToUndo,
        Err(error) => UndoOutcome::Failed(failure_of(error)),
    }
}

/// Each download failure in the update's words. Nothing cancels a download, so a cancel can only be one cut short.
fn download_failure(failure: DownloadFailure) -> Failure {
    match failure {
        DownloadFailure::Offline | DownloadFailure::Cancelled => Failure::Offline,
        DownloadFailure::Refused(code) => Failure::Refused(code),
        DownloadFailure::TooLarge => Failure::TooLarge,
        DownloadFailure::Disk => Failure::Disk,
    }
}

/// Each error of an update, an undo or a rollback in the update's words.
fn failure_of(error: UpdateError) -> Failure {
    match error {
        UpdateError::BadRelease(ZipFailure::TooLarge) => Failure::TooLarge,
        UpdateError::BadRelease(ZipFailure::Mismatch | ZipFailure::BadArchive) | UpdateError::BadVersion => Failure::Mismatch,
        // Another Jobdesk holds the folder, the rollback of a stopped update is due, which the next open runs first,
        // or a file changed while it ran: each time, nothing changed, and the next open tries again.
        UpdateError::Busy | UpdateError::Interrupted | UpdateError::Changed => Failure::Busy,
        // Only an update ends so, and `run` takes each for the gate it stands for.
        UpdateError::Superseded | UpdateError::Undone => Failure::Busy,
        UpdateError::OpenElsewhere => Failure::OpenElsewhere,
        UpdateError::Kept(files) => Failure::Kept(files),
        UpdateError::Unnoted => Failure::Unnoted,
        // Only the rollback reads a journal, and it sets aside one that does not read without rolling anything back.
        UpdateError::BadJournal => Failure::Stopped,
        // The gate reads the base before any of these, so only a folder changed meanwhile gets the first two here.
        UpdateError::NoBase | UpdateError::BadBase(_) | UpdateError::NothingToUndo | UpdateError::Disk | UpdateError::RollbackFailed => Failure::Disk,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::folder_update::domain::AloneLook;
    use crate::shared_kernel::installed_source::InstalledSource;
    use crate::updates::application::SourcePin;

    const BASE_SHA: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const PIN_SHA: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    const FOLDER: &str = "/Users/sam/Job search";

    /// Every port at once, with what each answers, and a log of what was asked, in order.
    struct Fake {
        log: Mutex<Vec<String>>,
        exists: bool,
        git: bool,
        base: Base,
        /// The base an update or an undo that succeeded wrote, which every later read finds in place of `base`.
        moved: Mutex<Option<Base>>,
        /// The base an update that succeeds writes, if it moves the one the fake reads.
        after_update: Option<Base>,
        /// The base an undo that succeeds puts back, if it moves the one the fake reads.
        after_undo: Option<Base>,
        /// What Undo noted it undid.
        skipped: Option<InstalledSource>,
        recover: Result<Option<Reverted>, UpdateError>,
        fetch: Result<Vec<u8>, DownloadFailure>,
        update: Result<Report, UpdateError>,
        undo: Result<Reverted, UpdateError>,
        undoable: bool,
        /// An Undo of the last update stopped partway, at an earlier open.
        unfinished: bool,
        /// `update` and `undo` panic, as the zip crate or the planner might on bytes a run's tools wrote.
        panics: bool,
        /// Another Jobdesk holds the folder alone, so this one cannot hold it open.
        kept_out: AtomicBool,
        /// Another Jobdesk has the folder open, so a stopped update's rollback waits.
        others_open: AtomicBool,
        /// Another Jobdesk takes the folder alone in the moment `alone_possible` lets go of this one's hold, so this one
        /// is kept out once it has looked.
        grabbed_in_probe: AtomicBool,
    }

    impl Fake {
        /// A folder at 1.7.5 that every gate lets through, whose update to 1.8.0 succeeds.
        fn ready() -> Self {
            Fake {
                log: Mutex::default(),
                exists: true,
                git: false,
                base: based_on("1.7.5"),
                moved: Mutex::default(),
                after_update: None,
                after_undo: None,
                skipped: None,
                recover: Ok(None),
                fetch: Ok(b"new zip".to_vec()),
                update: Ok(report()),
                undo: Ok(Reverted { reverted: vec!["CLAUDE.md".into()], kept: Vec::new() }),
                undoable: true,
                unfinished: false,
                panics: false,
                kept_out: AtomicBool::new(false),
                others_open: AtomicBool::new(false),
                grabbed_in_probe: AtomicBool::new(false),
            }
        }

        fn note(&self, line: String) {
            self.log.lock().unwrap().push(line);
        }

        fn asked(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }

        fn ports(self: &Arc<Self>) -> Ports {
            Ports { probe: self.clone(), base: self.clone(), fetcher: self.clone(), updater: self.clone() }
        }
    }

    impl FolderProbe for Fake {
        fn exists(&self, folder: &Path) -> bool {
            assert_eq!(folder, Path::new(FOLDER));
            self.note("exists".into());
            self.exists
        }
        fn has_git(&self, _folder: &Path) -> bool {
            self.note("git".into());
            self.git
        }
    }

    impl BaseReader for Fake {
        fn read(&self, _folder: &Path) -> Base {
            self.note("base".into());
            self.moved.lock().unwrap().clone().unwrap_or_else(|| self.base.clone())
        }
        fn skipped(&self, _folder: &Path) -> Option<InstalledSource> {
            self.skipped.clone()
        }
    }

    impl ZipFetcher for Fake {
        fn fetch(&self, url: &str, name: &str, progress: &mut dyn FnMut(u64)) -> Result<Vec<u8>, DownloadFailure> {
            self.note(format!("fetch {url} {name}"));
            progress(7);
            self.fetch.clone()
        }
    }

    impl FolderUpdater for Fake {
        fn open(&self, folder: &Path) -> Option<FolderWait> {
            self.note(format!("open {}", folder.display()));
            self.kept_out.load(Ordering::SeqCst).then_some(FolderWait::Updating)
        }
        fn hold_again(&self, _folder: &Path) -> Option<FolderWait> {
            self.kept_out.load(Ordering::SeqCst).then_some(FolderWait::Updating)
        }
        fn alone_possible(&self, _folder: &Path) -> AloneLook {
            if self.grabbed_in_probe.load(Ordering::SeqCst) {
                self.kept_out.store(true, Ordering::SeqCst);
            }
            let kept_out = self.kept_out.load(Ordering::SeqCst).then_some(FolderWait::Updating);
            AloneLook { alone: !self.others_open.load(Ordering::SeqCst), kept_out }
        }
        fn recover(&self, _folder: &Path) -> Result<Option<Reverted>, UpdateError> {
            self.note("recover".into());
            if self.others_open.load(Ordering::SeqCst) {
                return Err(UpdateError::OpenElsewhere);
            }
            self.recover.clone()
        }
        fn update(&self, folder: &Path, update: &Update) -> Result<Report, UpdateError> {
            assert_eq!(folder, Path::new(FOLDER));
            assert_eq!((update.base.zip, update.new.zip), (&b"base zip"[..], &b"new zip"[..]), "the kept base and the download");
            if let Base::Found { source, .. } = &self.base {
                assert_eq!(update.base_version, source.version, "the base as the gate read it, for the update to read again");
            }
            self.note(format!("update {} {} {}", update.version, update.base.sha256, update.new.sha256));
            assert!(!self.panics, "a fictional panic in the update");
            if let (Ok(_), Some(after)) = (&self.update, &self.after_update) {
                *self.moved.lock().unwrap() = Some(after.clone());
            }
            self.update.clone()
        }
        fn can_undo(&self, _folder: &Path) -> bool {
            self.note("can undo".into());
            self.undoable
        }
        fn undo_unfinished(&self, _folder: &Path) -> bool {
            self.unfinished
        }
        fn undo(&self, _folder: &Path) -> Result<Reverted, UpdateError> {
            self.note("undo".into());
            assert!(!self.panics, "a fictional panic in the undo");
            if let (Ok(_), Some(after)) = (&self.undo, &self.after_undo) {
                *self.moved.lock().unwrap() = Some(after.clone());
            }
            self.undo.clone()
        }
    }

    /// A base that reads, of `version`.
    fn based_on(version: &str) -> Base {
        Base::Found { source: InstalledSource { version: version.into(), sha256: BASE_SHA.into() }, zip: b"base zip".to_vec() }
    }

    fn report() -> Report {
        Report { version: "1.8.0".into(), updated: vec!["CLAUDE.md".into()], ..Report::default() }
    }

    fn pin(version: &str) -> SourcePin {
        SourcePin::parse(version, PIN_SHA).unwrap()
    }

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    /// The feed of a check that names 0.2.0 as the latest Jobdesk and pins 1.8.0.
    fn feed() -> LatestFeed {
        pinning(Some("1.8.0"))
    }

    /// The feed of a check that names 0.2.0 as the latest Jobdesk and pins `pinned`, if anything.
    fn pinning(pinned: Option<&str>) -> LatestFeed {
        LatestFeed::for_tests(version("0.2.0"), pinned.map(pin))
    }

    /// One pass on this thread, as Jobdesk 0.2.0, with the phases it reported.
    fn pass(fake: Fake, feed: &LatestFeed) -> (Outcome, Vec<String>, Vec<Phase>) {
        let fake = Arc::new(fake);
        let mut phases = Vec::new();
        let outcome = run(&fake.ports(), Path::new(FOLDER), feed, Some(version("0.2.0")), &mut |phase| phases.push(phase));
        (outcome, fake.asked(), phases)
    }

    fn fetched(asked: &[String]) -> bool {
        asked.iter().any(|line| line.starts_with("fetch"))
    }

    #[test]
    fn every_gate_that_holds_leads_to_the_update_of_the_pinned_release() {
        let (outcome, asked, phases) = pass(Fake::ready(), &feed());
        assert_eq!(outcome, Outcome::Updated(report()));
        assert_eq!(
            asked,
            [
                "recover",
                "exists",
                "git",
                "base",
                "fetch https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.8.0 update-ai-job-search-v1.8.0.zip",
                &format!("update 1.8.0 {BASE_SHA} {PIN_SHA}"),
            ]
        );
        assert_eq!(phases, [Phase::Checking, Phase::Downloading { received: 0 }, Phase::Downloading { received: 7 }, Phase::Updating]);
    }

    #[test]
    fn each_gate_that_fails_is_its_own_outcome_and_downloads_nothing() {
        let cases: [(Fake, LatestFeed, Outcome); 8] = [
            (Fake { exists: false, ..Fake::ready() }, feed(), Outcome::NoFolder),
            (Fake { git: true, ..Fake::ready() }, feed(), Outcome::GitFolder),
            (Fake { base: Base::Missing, ..Fake::ready() }, feed(), Outcome::NoBase),
            (Fake { base: Base::Bad, ..Fake::ready() }, feed(), Outcome::BadBase),
            (
                Fake { base: Base::Found { source: InstalledSource { version: "1.7".into(), sha256: BASE_SHA.into() }, zip: Vec::new() }, ..Fake::ready() },
                feed(),
                Outcome::BadBase,
            ),
            (Fake::ready(), LatestFeed::for_tests(version("0.3.0"), Some(pin("1.8.0"))), Outcome::JobdeskNotLatest),
            (Fake::ready(), pinning(None), Outcome::NoPin),
            (Fake::ready(), pinning(Some("1.7.5")), Outcome::UpToDate),
        ];
        for (fake, feed, expected) in cases {
            let (outcome, asked, _) = pass(fake, &feed);
            assert_eq!(outcome, expected);
            assert!(!fetched(&asked), "{expected:?}: {asked:?}");
            assert!(!asked.iter().any(|line| line.starts_with("update")), "{expected:?}");
        }
    }

    #[test]
    fn a_jobdesk_older_or_newer_than_the_latest_or_of_no_version_is_not_the_latest() {
        for current in [Some(version("0.1.9")), Some(version("0.2.1")), None] {
            let fake = Arc::new(Fake::ready());
            assert_eq!(run(&fake.ports(), Path::new(FOLDER), &feed(), current, &mut |_| {}), Outcome::JobdeskNotLatest, "{current:?}");
            assert!(!fetched(&fake.asked()));
        }
    }

    #[test]
    fn a_signed_pin_older_than_or_equal_to_the_base_is_refused() {
        for older in ["1.7.5", "1.7.4", "1.6.9", "0.9.9"] {
            let (outcome, asked, _) = pass(Fake::ready(), &pinning(Some(older)));
            assert_eq!(outcome, Outcome::UpToDate, "{older}");
            assert!(!fetched(&asked), "{older}");
        }
        let (outcome, _, _) = pass(Fake::ready(), &pinning(Some("1.7.6")));
        assert_eq!(outcome, Outcome::Updated(report()), "one patch newer is newer");
    }

    /// After Undo the same release is never applied again, and a newer one updates the folder as usual. The note names
    /// one release, compared for equality with the pin, so a note crafted in the folder can skip only that very release
    /// and never makes anything apply.
    #[test]
    fn a_release_undone_is_not_applied_again_but_a_newer_one_is() {
        let note = |version: &str, sha256: &str| Some(InstalledSource { version: version.into(), sha256: sha256.into() });
        let (outcome, asked, _) = pass(Fake { skipped: note("1.8.0", PIN_SHA), ..Fake::ready() }, &feed());
        assert_eq!(outcome, Outcome::Skipped);
        assert!(!fetched(&asked), "nothing is downloaded");

        let (outcome, _, _) = pass(Fake { skipped: note("1.8.0", PIN_SHA), ..Fake::ready() }, &pinning(Some("1.8.1")));
        assert_eq!(outcome, Outcome::Updated(report()), "a newer pin updates");

        for crafted in [note("1.8.0", BASE_SHA), note("1.9.0", PIN_SHA), note("1.7.9", PIN_SHA), note("99.0.0", PIN_SHA), note("1.8", PIN_SHA)] {
            let (outcome, _, _) = pass(Fake { skipped: crafted.clone(), ..Fake::ready() }, &feed());
            assert_eq!(outcome, Outcome::Updated(report()), "{crafted:?} skips only an equal release");
        }
        for older in ["1.7.5", "1.7.0"] {
            let (outcome, asked, _) = pass(Fake { skipped: note(older, PIN_SHA), ..Fake::ready() }, &pinning(Some(older)));
            assert_eq!(outcome, Outcome::UpToDate, "a note never makes an older pin apply");
            assert!(!fetched(&asked));
        }
    }

    #[test]
    fn a_keyless_build_or_a_feed_with_no_pin_reads_nothing_from_the_network() {
        let (outcome, asked, phases) = pass(Fake::ready(), &pinning(None));
        assert_eq!(outcome, Outcome::NoPin);
        assert_eq!(asked, ["recover", "exists", "git", "base"]);
        assert_eq!(phases, [Phase::Checking]);
    }

    #[test]
    fn a_stopped_update_is_rolled_back_before_the_gate() {
        let (outcome, asked, _) = pass(Fake { exists: false, ..Fake::ready() }, &feed());
        assert_eq!((outcome, asked[0].as_str()), (Outcome::NoFolder, "recover"));

        let rolled_back = Reverted { reverted: vec!["CLAUDE.md".into()], kept: Vec::new() };
        let (outcome, asked, _) = pass(Fake { recover: Ok(Some(rolled_back.clone())), ..Fake::ready() }, &feed());
        assert_eq!(outcome, Outcome::Recovered(rolled_back));
        assert_eq!(asked, ["recover"], "nothing else runs until the next open");

        for (error, failure) in [
            (UpdateError::Busy, Failure::Busy),
            (UpdateError::Kept(vec!["CLAUDE.md".into()]), Failure::Kept(vec!["CLAUDE.md".into()])),
            (UpdateError::BadJournal, Failure::Stopped),
            (UpdateError::Disk, Failure::Disk),
        ] {
            let (outcome, asked, _) = pass(Fake { recover: Err(error.clone()), ..Fake::ready() }, &feed());
            assert_eq!(outcome, Outcome::RollbackFailed(failure), "{error:?}: the rollback failed, and no update ran");
            assert_eq!(asked, ["recover"], "{error:?}");
        }
    }

    #[test]
    fn each_download_failure_reads_in_the_updates_words() {
        for (download, failure) in [
            (DownloadFailure::Offline, Failure::Offline),
            (DownloadFailure::Cancelled, Failure::Offline),
            (DownloadFailure::Refused(404), Failure::Refused(404)),
            (DownloadFailure::TooLarge, Failure::TooLarge),
            (DownloadFailure::Disk, Failure::Disk),
        ] {
            let (outcome, asked, _) = pass(Fake { fetch: Err(download), ..Fake::ready() }, &feed());
            assert_eq!(outcome, Outcome::Failed(failure), "{download:?}");
            assert!(!asked.iter().any(|line| line.starts_with("update")), "{download:?}: nothing is updated");
        }
    }

    #[test]
    fn each_update_error_reads_in_the_updates_words() {
        let kept = vec!["CLAUDE.md".to_owned()];
        for (error, outcome) in [
            (UpdateError::BadRelease(ZipFailure::Mismatch), Outcome::Failed(Failure::Mismatch)),
            (UpdateError::BadRelease(ZipFailure::BadArchive), Outcome::Failed(Failure::Mismatch)),
            (UpdateError::BadRelease(ZipFailure::TooLarge), Outcome::Failed(Failure::TooLarge)),
            (UpdateError::BadVersion, Outcome::Failed(Failure::Mismatch)),
            (UpdateError::Busy, Outcome::Failed(Failure::Busy)),
            (UpdateError::Interrupted, Outcome::Failed(Failure::Busy)),
            (UpdateError::Changed, Outcome::Failed(Failure::Busy)),
            (UpdateError::Kept(kept.clone()), Outcome::Failed(Failure::Kept(kept.clone()))),
            (UpdateError::Disk, Outcome::Failed(Failure::Disk)),
            (UpdateError::RollbackFailed, Outcome::Failed(Failure::Disk)),
            (UpdateError::BadJournal, Outcome::Failed(Failure::Stopped)),
            (UpdateError::NothingToUndo, Outcome::Failed(Failure::Disk)),
            (UpdateError::NoBase, Outcome::NoBase),
            (UpdateError::BadBase(ZipFailure::BadArchive), Outcome::BadBase),
            // Another Jobdesk moved the base, or the user undid this release there, during the download.
            (UpdateError::Superseded, Outcome::UpToDate),
            (UpdateError::Undone, Outcome::Skipped),
        ] {
            assert_eq!(pass(Fake { update: Err(error.clone()), ..Fake::ready() }, &feed()).0, outcome, "{error:?}");
        }
    }

    /// A clock that stands still until a test moves it on.
    #[derive(Clone)]
    struct Hands(Arc<Mutex<Instant>>);

    impl Hands {
        fn new() -> Self {
            Hands(Arc::new(Mutex::new(Instant::now())))
        }

        fn clock(&self) -> Clock {
            let hands = self.0.clone();
            Arc::new(move || *hands.lock().unwrap())
        }

        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap() += by;
        }
    }

    fn desk(fake: &Arc<Fake>, current: &str) -> FolderUpdate {
        FolderUpdate::new(fake.clone(), fake.clone(), fake.clone(), fake.clone(), current, Arc::new(|| {}))
    }

    /// Polls until the worker has ended, or fails the test after two seconds.
    fn settle(desk: &mut FolderUpdate) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while desk.is_working() {
            assert!(Instant::now() < deadline, "the worker never ended");
            desk.poll();
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn an_update_runs_on_a_worker_and_ends_updated_with_its_report() {
        let fake = Arc::new(Fake::ready());
        let mut desk = desk(&fake, "0.2.0");
        assert!(fake.asked().is_empty(), "nothing is asked of the folder or the network until a start");
        assert_eq!((desk.stage(), desk.can_undo()), (&Stage::Idle, false));
        desk.start(PathBuf::from(FOLDER), &feed(), true);
        assert!(desk.is_working());
        assert!(matches!(desk.stage(), Stage::Working(_)));
        assert!(!desk.can_undo(), "not while it works");
        desk.start(PathBuf::from(FOLDER), &feed(), true);
        settle(&mut desk);
        assert_eq!(desk.stage(), &Stage::Ended(Outcome::Updated(report())));
        assert!(desk.can_undo());
        let asked = fake.asked();
        assert_eq!(asked.iter().filter(|line| line.starts_with("update")).count(), 1, "a second start while one works is ignored");
        assert_eq!(asked.last().map(String::as_str), Some("can undo"));
    }

    #[test]
    fn a_jobdesk_whose_version_does_not_read_is_never_the_latest() {
        let fake = Arc::new(Fake::ready());
        let mut desk = desk(&fake, "0.2.0-dev");
        desk.start(PathBuf::from(FOLDER), &feed(), true);
        settle(&mut desk);
        assert_eq!(desk.stage(), &Stage::Ended(Outcome::JobdeskNotLatest));
    }

    #[test]
    fn an_open_run_refuses_everything_undo_included() {
        let fake = Arc::new(Fake::ready());
        let mut desk = desk(&fake, "0.2.0");
        desk.start(PathBuf::from(FOLDER), &feed(), false);
        assert!(!desk.is_working());
        assert_eq!(desk.stage(), &Stage::Ended(Outcome::Failed(Failure::Busy)));
        desk.undo(PathBuf::from(FOLDER), false);
        assert!(!desk.is_working());
        assert_eq!(desk.stage(), &Stage::Undid(UndoOutcome::Failed(Failure::Busy)));
        assert!(!desk.recover(PathBuf::from(FOLDER), false), "the caller hears of the refusal");
        assert!(!desk.is_working());
        assert_eq!(desk.stage(), &Stage::Undid(UndoOutcome::Failed(Failure::Busy)), "a refused rollback leaves the stage as it was");
        assert!(fake.asked().is_empty(), "no rollback, gate, download, update or undo");
    }

    /// At an open no update was tried, so a rollback refused while a task is open shows no failure in place of what
    /// the screen said.
    #[test]
    fn a_rollback_refused_at_an_open_shows_no_failure() {
        let fake = Arc::new(Fake::ready());
        let mut desk = desk(&fake, "0.2.0");
        desk.start(PathBuf::from(FOLDER), &feed(), true);
        settle(&mut desk);
        let notice = desk.notice();
        assert!(!desk.recover(PathBuf::from(FOLDER), false));
        assert_eq!(desk.stage(), &Stage::Ended(Outcome::Updated(report())));
        assert_eq!(desk.notice(), notice);
        assert!(desk.recover(PathBuf::from(FOLDER), true) && desk.is_working(), "an idle desk starts it");
        settle(&mut desk);
    }

    /// Runs `follow` once and waits for the worker it started, if any.
    fn follow(desk: &mut FolderUpdate, feed: Option<(u64, &LatestFeed)>, runs_idle: bool) {
        desk.follow(feed, runs_idle);
        settle(desk);
    }

    /// The update passes that reached the update, each of which the fake lets through.
    fn passes(fake: &Fake) -> usize {
        fake.asked().iter().filter(|line| line.starts_with("update ")).count()
    }

    fn rollbacks(fake: &Fake) -> usize {
        fake.asked().iter().filter(|line| line.as_str() == "recover").count()
    }

    #[test]
    fn an_open_folder_rolls_back_first_then_updates_once_per_answer_of_the_feed() {
        let fake = Arc::new(Fake::ready());
        let mut desk = desk(&fake, "0.2.0");
        let feed = feed();
        follow(&mut desk, Some((1, &feed)), true);
        assert!(fake.asked().is_empty(), "no folder is open, so nothing runs");

        desk.opened(PathBuf::from(FOLDER));
        assert_eq!(fake.asked(), ["open /Users/sam/Job search"], "the open holds the folder at once");
        follow(&mut desk, Some((1, &feed)), false);
        assert_eq!(fake.asked(), ["open /Users/sam/Job search"], "nothing runs while a task or a session is open");
        follow(&mut desk, None, true);
        assert_eq!((rollbacks(&fake), passes(&fake)), (1, 0), "the rollback runs at the open, with no feed");
        follow(&mut desk, None, true);
        assert_eq!(rollbacks(&fake), 1, "once per open");

        follow(&mut desk, Some((1, &feed)), true);
        assert_eq!(desk.stage(), &Stage::Ended(Outcome::Updated(report())));
        for _ in 0..3 {
            follow(&mut desk, Some((1, &feed)), true);
        }
        assert_eq!(passes(&fake), 1, "one pass for one answer, however many frames");
        follow(&mut desk, Some((2, &feed)), true);
        assert_eq!(passes(&fake), 2, "the next answer of the feed tries again");

        desk.opened(PathBuf::from(FOLDER));
        follow(&mut desk, Some((2, &feed)), true);
        follow(&mut desk, Some((2, &feed)), true);
        assert_eq!((rollbacks(&fake), passes(&fake)), (4, 2), "an open again rolls back, and the answer it was tried with stays tried");
    }

    /// A rollback at an open that ran, with or without files it kept, or that failed, starts no update pass of that
    /// folder for the rest of the launch: its notice stays, and so does the backup that holds the originals of any file
    /// it kept, which an update would replace.
    #[test]
    fn no_update_runs_after_a_rollback_in_the_same_launch() {
        let reverted = |kept: &[&str]| Reverted { reverted: vec!["a.md".into()], kept: kept.iter().map(|path| (*path).to_owned()).collect() };
        for recover in [
            Ok(Some(reverted(&[]))),
            Ok(Some(reverted(&["CLAUDE.md"]))),
            Err(UpdateError::Kept(vec!["CLAUDE.md".into()])),
            Err(UpdateError::BadJournal),
            Err(UpdateError::Disk),
        ] {
            let fake = Arc::new(Fake { recover: recover.clone(), ..Fake::ready() });
            let mut opened = desk(&fake, "0.2.0");
            opened.opened(PathBuf::from(FOLDER));
            follow(&mut opened, None, true);
            let (stage, notice) = (opened.stage().clone(), opened.notice());
            assert!(matches!(stage, Stage::Ended(Outcome::Recovered(_) | Outcome::RollbackFailed(_))), "{recover:?}: {stage:?}");
            assert!(notice.is_some(), "{recover:?}");
            for answer in 1..=2 {
                follow(&mut opened, Some((answer, &feed())), true);
            }
            assert_eq!((rollbacks(&fake), passes(&fake)), (1, 0), "{recover:?}: no pass starts");
            assert_eq!((opened.stage(), opened.notice()), (&stage, notice), "{recover:?}: the notice stays");
            assert_eq!(opened.about_line(false), None, "{recover:?}: About asks for no check, which would update nothing");

            opened.opened(PathBuf::from(FOLDER));
            follow(&mut opened, None, true);
            follow(&mut opened, Some((3, &feed())), true);
            assert_eq!(passes(&fake), 0, "{recover:?}: nor after the folder opens again in this launch");
        }
    }

    /// An update whose rollback kept files leaves their originals only in its backup, which the next update would
    /// replace, so no pass of that folder starts again for the rest of the launch. A failure that kept nothing is tried
    /// again at the next answer of the feed: its journal, if it left one, is rolled back first.
    #[test]
    fn no_update_runs_after_an_update_whose_rollback_kept_files_in_the_same_launch() {
        let kept = vec!["CLAUDE.md".to_owned()];
        let fake = Arc::new(Fake { update: Err(UpdateError::Kept(kept.clone())), ..Fake::ready() });
        let mut opened = desk(&fake, "0.2.0");
        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, None, true);
        follow(&mut opened, Some((1, &feed())), true);
        let failed = Stage::Ended(Outcome::Failed(Failure::Kept(kept)));
        assert_eq!((opened.stage(), passes(&fake)), (&failed, 1));
        let notice = opened.notice();
        assert!(notice.is_some());
        for answer in 2..=3 {
            follow(&mut opened, Some((answer, &feed())), true);
        }
        assert_eq!(passes(&fake), 1, "no pass starts at a later answer of the feed");
        assert_eq!((opened.stage(), opened.notice()), (&failed, notice), "the notice stays");

        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, None, true);
        follow(&mut opened, Some((4, &feed())), true);
        assert_eq!(passes(&fake), 1, "nor after the folder opens again in this launch");
        assert_eq!(opened.about_line(false), None, "About asks for no check, which would update nothing");

        for error in [UpdateError::Disk, UpdateError::RollbackFailed, UpdateError::Busy] {
            let fake = Arc::new(Fake { update: Err(error.clone()), ..Fake::ready() });
            let mut opened = desk(&fake, "0.2.0");
            opened.opened(PathBuf::from(FOLDER));
            follow(&mut opened, None, true);
            follow(&mut opened, Some((1, &feed())), true);
            follow(&mut opened, Some((2, &feed())), true);
            assert_eq!(passes(&fake), 2, "{error:?}: the next answer of the feed tries again");
        }
    }

    /// While another Jobdesk holds the open folder alone, this one is busy as while its own worker runs, starts no
    /// rollback or pass, and tries to hold the folder again every `HOLD_RETRY`. Once it does, the rollback due runs.
    #[test]
    fn a_folder_another_jobdesk_holds_alone_is_busy_until_this_one_holds_it() {
        let fake = Arc::new(Fake { kept_out: AtomicBool::new(true), ..Fake::ready() });
        let hands = Hands::new();
        let mut opened = desk(&fake, "0.2.0").with_clock(hands.clock());
        assert!(!opened.is_busy());
        opened.opened(PathBuf::from(FOLDER));
        assert!(opened.is_busy() && opened.kept_out() && !opened.is_working());
        follow(&mut opened, Some((1, &feed())), true);
        assert_eq!((rollbacks(&fake), passes(&fake)), (0, 0), "nothing starts");
        assert!(opened.is_busy());

        fake.kept_out.store(false, Ordering::SeqCst);
        hands.advance(HOLD_RETRY - Duration::from_millis(1));
        follow(&mut opened, None, true);
        assert!(opened.is_busy(), "not tried again before HOLD_RETRY");
        hands.advance(Duration::from_millis(1));
        follow(&mut opened, None, true);
        assert!(!opened.kept_out(), "held once tried again");
        assert_eq!(rollbacks(&fake), 1, "and the rollback due at the open runs");
    }

    /// A stopped update whose rollback waits because another Jobdesk window has the folder open says so, and the
    /// rollback is tried again on the hold's cadence, starting only once the folder can be held alone.
    #[test]
    fn a_rollback_another_window_keeps_waiting_says_so_and_runs_once_it_closes() {
        let rolled_back = Reverted { reverted: vec!["CLAUDE.md".into()], kept: Vec::new() };
        let fake = Arc::new(Fake { others_open: AtomicBool::new(true), recover: Ok(Some(rolled_back.clone())), ..Fake::ready() });
        let hands = Hands::new();
        let mut opened = desk(&fake, "0.2.0").with_clock(hands.clock());
        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, Some((1, &feed())), true);
        assert_eq!(opened.stage(), &Stage::Ended(Outcome::RollbackFailed(Failure::OpenElsewhere)));
        let notice = opened.notice().expect("the wait is told");
        assert_eq!(
            notice.words,
            "Jobdesk could not finish an interrupted update of your job search folder because another Jobdesk window has it open. Close the other window to let it finish."
        );

        hands.advance(HOLD_RETRY);
        follow(&mut opened, Some((2, &feed())), true);
        assert_eq!((rollbacks(&fake), passes(&fake)), (1, 0), "nothing starts while the other window has it open");

        fake.others_open.store(false, Ordering::SeqCst);
        follow(&mut opened, None, true);
        assert_eq!(rollbacks(&fake), 1, "not before HOLD_RETRY");
        hands.advance(HOLD_RETRY);
        follow(&mut opened, None, true);
        assert_eq!(rollbacks(&fake), 2, "the rollback runs once the folder can be held alone");
        assert_eq!(opened.stage(), &Stage::Ended(Outcome::Recovered(rolled_back)));
    }

    /// The look whether the folder can be held alone lets go of this Jobdesk's hold for a moment. When another Jobdesk
    /// takes the folder alone in that moment, this one is kept out, and every writer waits, as at an open.
    #[test]
    fn a_look_that_cannot_take_the_hold_back_keeps_the_writers_waiting() {
        let fake = Arc::new(Fake { others_open: AtomicBool::new(true), ..Fake::ready() });
        let hands = Hands::new();
        let mut opened = desk(&fake, "0.2.0").with_clock(hands.clock());
        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, None, true);
        assert_eq!(opened.stage(), &Stage::Ended(Outcome::RollbackFailed(Failure::OpenElsewhere)));
        assert!(!opened.is_busy());

        fake.grabbed_in_probe.store(true, Ordering::SeqCst);
        hands.advance(HOLD_RETRY);
        follow(&mut opened, None, true);
        assert!(opened.is_busy() && opened.kept_out(), "the look found the hold taken by another Jobdesk");
        assert_eq!(rollbacks(&fake), 1, "and starts nothing");
    }

    /// A rollback another Jobdesk kept out ran nothing and says nothing, so it holds no update back.
    #[test]
    fn a_busy_rollback_holds_no_update_back() {
        let fake = Arc::new(Fake { recover: Err(UpdateError::Busy), ..Fake::ready() });
        let mut opened = desk(&fake, "0.2.0");
        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, None, true);
        follow(&mut opened, Some((1, &feed())), true);
        assert_eq!(rollbacks(&fake), 2, "the pass starts, and rolls back first");
    }

    #[test]
    fn undo_is_offered_after_an_update_until_a_task_starts() {
        let fake = Arc::new(Fake::ready());
        let mut desk = desk(&fake, "0.2.0");
        desk.opened(PathBuf::from(FOLDER));
        follow(&mut desk, None, true);
        assert_eq!(desk.notice(), None, "a rollback with nothing to roll back says nothing");
        let feed = feed();
        follow(&mut desk, Some((1, &feed)), true);
        let notice = desk.notice().unwrap();
        assert_eq!((notice.words.as_str(), notice.undo), ("Your job search folder was updated with Jobdesk 0.2.0.", true));

        follow(&mut desk, Some((2, &LatestFeed::for_tests(version("0.2.0"), Some(pin("1.7.0"))))), true);
        assert_eq!(desk.stage(), &Stage::Ended(Outcome::UpToDate));
        assert_eq!(desk.notice(), Some(notice), "a later silent pass leaves the notice and Undo");

        desk.task_started();
        assert!(!desk.notice().unwrap().undo, "a task started, so Undo goes");
        desk.undo_update(true);
        assert!(!desk.is_working() && !fake.asked().contains(&"undo".to_owned()), "and cannot be asked for");
    }

    #[test]
    fn undo_answers_in_the_notice_with_the_files_it_kept() {
        let kept = Reverted { reverted: vec!["a.md".into()], kept: vec!["CLAUDE.md".into()] };
        let fake = Arc::new(Fake { undo: Ok(kept), ..Fake::ready() });
        let mut desk = desk(&fake, "0.2.0");
        desk.opened(PathBuf::from(FOLDER));
        follow(&mut desk, None, true);
        follow(&mut desk, Some((1, &feed())), true);
        desk.undo_update(true);
        assert_eq!(desk.notice().unwrap().words, "Undoing the last update of your job search folder…");
        settle(&mut desk);
        let notice = desk.notice().unwrap();
        assert!(notice.words.starts_with("Jobdesk undid the update, except for 1 file that changed since."), "{}", notice.words);
        assert_eq!((notice.undo, notice.lists[0].items.as_slice()), (false, &["CLAUDE.md".to_owned()][..]));
    }

    /// An updated folder, its Undo pressed against a fake whose undo answers `undo`.
    fn undo_answering(undo: Result<Reverted, UpdateError>) -> (Arc<Fake>, FolderUpdate) {
        let fake = Arc::new(Fake { undo, ..Fake::ready() });
        let mut updated = desk(&fake, "0.2.0");
        updated.opened(PathBuf::from(FOLDER));
        follow(&mut updated, None, true);
        follow(&mut updated, Some((1, &feed())), true);
        updated.undo_update(true);
        settle(&mut updated);
        (fake, updated)
    }

    /// An Undo that failed, before it changed anything or partway, leaves the update's record, so Undo is still offered
    /// and a retry goes on: files already put back are put back again as they are.
    #[test]
    fn a_failed_undo_still_offers_undo() {
        for (error, failure) in [(UpdateError::Unnoted, Failure::Unnoted), (UpdateError::Disk, Failure::Disk), (UpdateError::Busy, Failure::Busy)] {
            let (fake, mut failed) = undo_answering(Err(error.clone()));
            assert_eq!(failed.stage(), &Stage::Undid(UndoOutcome::Failed(failure)), "{error:?}");
            let notice = failed.notice().unwrap();
            assert!(notice.undo, "{error:?}: {notice:?}");
            failed.undo_update(true);
            settle(&mut failed);
            assert_eq!(fake.asked().iter().filter(|line| line.as_str() == "undo").count(), 2, "{error:?}: a retry runs");
        }
    }

    /// An Undo that stopped partway, at an earlier open, is found at the next open: the notice says so honestly and
    /// offers Undo, which finishes it.
    #[test]
    fn an_unfinished_undo_is_offered_at_the_open_and_undo_finishes_it() {
        let fake = Arc::new(Fake { unfinished: true, ..Fake::ready() });
        let mut opened = desk(&fake, "0.2.0");
        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, None, true);
        assert_eq!(opened.stage(), &Stage::Ended(Outcome::UndoUnfinished));
        let notice = opened.notice().unwrap();
        assert_eq!(notice.words, "Jobdesk did not finish undoing the last update of your job search folder.");
        assert!(notice.undo && notice.dismiss, "{notice:?}");
        opened.undo_update(true);
        settle(&mut opened);
        assert!(matches!(opened.stage(), Stage::Undid(UndoOutcome::Undone(_))), "{:?}", opened.stage());

        let clean = Arc::new(Fake::ready());
        let mut opened = desk(&clean, "0.2.0");
        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, None, true);
        assert_eq!((opened.stage(), opened.notice()), (&Stage::Idle, None), "an Undo that finished says nothing");
    }

    /// An open that found an unfinished Undo, as the fake reports one.
    fn opened_unfinished() -> (Arc<Fake>, FolderUpdate) {
        let fake = Arc::new(Fake { unfinished: true, ..Fake::ready() });
        let mut opened = desk(&fake, "0.2.0");
        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, None, true);
        assert_eq!(opened.stage(), &Stage::Ended(Outcome::UndoUnfinished));
        (fake, opened)
    }

    /// While an Undo is unfinished no pass updates the folder, even to a newer release, so the revert can still be
    /// finished. The notice and its Undo stay, About asks for no check, and Undo still finishes.
    #[test]
    fn no_update_runs_while_an_undo_is_unfinished() {
        let (fake, mut opened) = opened_unfinished();
        let notice = opened.notice();
        assert_eq!(opened.about_line(false), None, "a check would update nothing");
        follow(&mut opened, Some((1, &pinning(Some("1.9.0")))), true);
        assert_eq!(opened.stage(), &Stage::Ended(Outcome::UndoPending));
        assert!(!fetched(&fake.asked()) && passes(&fake) == 0, "nothing is downloaded or updated");
        assert_eq!(opened.notice(), notice, "the notice of the open stays, its Undo with it");
        assert_eq!(opened.about_line(false), None);
        opened.undo_update(true);
        settle(&mut opened);
        assert!(matches!(opened.stage(), Stage::Undid(UndoOutcome::Undone(_))), "{:?}", opened.stage());
    }

    /// The Undo of an unfinished undo stays offered once a task starts: finishing it puts back only files that still
    /// hold what the update wrote, so a task's edits stay. The Undo right after an update still ends with the next task.
    #[test]
    fn a_task_ends_the_undo_of_an_update_but_not_of_an_unfinished_undo() {
        let (_, mut opened) = opened_unfinished();
        opened.task_started();
        assert!(opened.notice().unwrap().undo, "the way forward stays");
        opened.dismiss();
        assert_eq!(opened.notice(), None, "Dismiss hides it for this launch");
        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, None, true);
        assert!(opened.notice().unwrap().undo, "and the next open finds it again");

        let fake = Arc::new(Fake::ready());
        let mut updated = desk(&fake, "0.2.0");
        updated.opened(PathBuf::from(FOLDER));
        follow(&mut updated, None, true);
        follow(&mut updated, Some((1, &feed())), true);
        assert!(updated.notice().unwrap().undo);
        updated.task_started();
        assert!(!updated.notice().unwrap().undo, "a task ends the Undo right after an update");
    }

    #[test]
    fn a_dismissed_notice_stays_away_and_another_folder_starts_afresh() {
        let fake = Arc::new(Fake { git: true, ..Fake::ready() });
        let mut desk = desk(&fake, "0.2.0").showing(Stage::Ended(Outcome::Failed(Failure::Offline)), false);
        assert!(desk.notice().is_some());
        desk.dismiss();
        assert_eq!(desk.notice(), None);

        desk.opened(PathBuf::from(FOLDER));
        follow(&mut desk, None, true);
        follow(&mut desk, Some((1, &feed())), true);
        assert_eq!(desk.stage(), &Stage::Ended(Outcome::GitFolder));
        assert_eq!((desk.notice(), desk.quiet()), (None, Some("This folder uses git, so updating it is up to you.")));
        desk.opened(PathBuf::from("/Users/sam/Other search"));
        assert_eq!(desk.quiet(), None, "About speaks of the open folder only");
    }

    const CHECK: &str = "With automatic checks off, your job search folder is brought up to date only after you check for updates.";

    /// With the automatic check off, About says at the open that a check would update the folder, as long as it has a
    /// base and no git, until a pass has asked the gate. Jobdesk reads nothing from the network for it.
    #[test]
    fn about_asks_for_a_check_while_the_automatic_one_is_off() {
        let fake = Arc::new(Fake::ready());
        let mut first = desk(&fake, "0.2.0");
        assert_eq!(first.about_line(false), None, "no folder is open");
        first.opened(PathBuf::from(FOLDER));
        follow(&mut first, None, true);
        assert!(!fetched(&fake.asked()));
        assert_eq!(first.about_line(false), Some(CHECK), "the rollback at the open looked at the folder");
        assert_eq!(first.about_line(true), None, "the automatic check comes on its own");
        follow(&mut first, Some((1, &feed())), true);
        assert_eq!(first.about_line(false), None, "Check now has run a pass");

        for (fake, line) in [
            (Fake { git: true, ..Fake::ready() }, "This folder uses git, so updating it is up to you."),
            (Fake { base: Base::Missing, ..Fake::ready() }, "This folder was set up before Jobdesk could update it, so Jobdesk leaves it as it is."),
        ] {
            let mut opened = desk(&Arc::new(fake), "0.2.0");
            opened.opened(PathBuf::from(FOLDER));
            follow(&mut opened, None, true);
            assert_eq!((opened.about_line(false), opened.about_line(true)), (Some(line), Some(line)), "known at the open, with no feed");
        }

        // A rollback at the open that failed has its own line in the notice, and About asks for no check.
        let mut stuck = desk(&Arc::new(Fake { recover: Err(UpdateError::Disk), ..Fake::ready() }), "0.2.0");
        stuck.opened(PathBuf::from(FOLDER));
        follow(&mut stuck, None, true);
        assert_eq!(stuck.stage(), &Stage::Ended(Outcome::RollbackFailed(Failure::Disk)));
        assert_eq!(stuck.about_line(false), None);
    }

    /// The feed of a check that names 0.2.0 as the latest Jobdesk and lists `listed` as compatible, newest first.
    fn listing(listed: &[&str]) -> LatestFeed {
        LatestFeed::for_tests_with(version("0.2.0"), listed.iter().copied().map(pin).collect())
    }

    /// `fake`'s folder opened, and the look of its rollback done.
    fn looked(fake: Fake) -> FolderUpdate {
        let mut opened = desk(&Arc::new(fake), "0.2.0");
        opened.opened(PathBuf::from(FOLDER));
        follow(&mut opened, None, true);
        opened
    }

    /// About says the folder's files come from a release Jobdesk no longer supports only when the folder's base reads
    /// and a check read a signed list that leaves it out. Under git too, since its files may still be setup's.
    #[test]
    fn about_says_when_the_folders_version_is_no_longer_supported() {
        let dropped = listing(&["1.4.0"]);
        assert_eq!(desk(&Arc::new(Fake::ready()), "0.2.0").support_line(Some(&dropped)), None, "no folder is open");

        let old = looked(Fake { base: based_on("1.3.0"), ..Fake::ready() });
        assert_eq!(old.support_line(Some(&dropped)), Some(notice::UNSUPPORTED));
        assert_eq!(old.support_line(None), None, "no check has read the feed");
        assert_eq!(old.support_line(Some(&listing(&[]))), None, "a feed with no verified release says nothing");
        let under_git = looked(Fake { base: based_on("1.3.0"), git: true, ..Fake::ready() });
        assert_eq!(under_git.support_line(Some(&dropped)), Some(notice::UNSUPPORTED));

        let listed = looked(Fake { base: based_on("1.4.0"), ..Fake::ready() });
        assert_eq!(listed.support_line(Some(&listing(&["1.4.0", "1.3.0"]))), None);
        let older_listed = looked(Fake { base: based_on("1.3.0"), ..Fake::ready() });
        assert_eq!(older_listed.support_line(Some(&listing(&["1.4.0", "1.3.0"]))), None, "an older release still listed");

        for base in [Base::Missing, Base::Bad, based_on("1.3")] {
            let unread = looked(Fake { base: base.clone(), ..Fake::ready() });
            assert_eq!(unread.support_line(Some(&dropped)), None, "{base:?}");
        }
    }

    /// The base is read again once each pass and each undo ended, so the line goes with an update to a listed release
    /// and comes back with its Undo. Another folder starts with nothing to say until its own look.
    #[test]
    fn the_support_line_follows_the_base_through_an_update_and_its_undo() {
        let fake = Fake { base: based_on("1.3.0"), after_update: Some(based_on("1.4.0")), after_undo: Some(based_on("1.3.0")), ..Fake::ready() };
        let feed = listing(&["1.4.0"]);
        let mut opened = looked(fake);
        assert_eq!(opened.support_line(Some(&feed)), Some(notice::UNSUPPORTED));
        follow(&mut opened, Some((1, &feed)), true);
        assert_eq!(opened.stage(), &Stage::Ended(Outcome::Updated(report())));
        assert_eq!(opened.support_line(Some(&feed)), None, "the folder is at 1.4.0 now");
        opened.undo_update(true);
        settle(&mut opened);
        assert!(matches!(opened.stage(), Stage::Undid(UndoOutcome::Undone(_))), "{:?}", opened.stage());
        assert_eq!(opened.support_line(Some(&feed)), Some(notice::UNSUPPORTED), "Undo took it back to 1.3.0");

        opened.opened(PathBuf::from("/Users/sam/Other search"));
        assert_eq!(opened.support_line(Some(&feed)), None, "About speaks of the open folder only");
    }

    #[test]
    fn a_working_stage_shown_for_a_preview_keeps_working() {
        let fake = Arc::new(Fake::ready());
        let mut desk = desk(&fake, "0.2.0").showing(Stage::Working(Phase::Downloading { received: 0 }), false);
        desk.follow(None, true);
        assert!(desk.is_working());
        assert_eq!(desk.notice().unwrap().words, "Updating your job search folder to go with Jobdesk 0.2.0…");
    }

    #[test]
    fn a_stopped_update_rolls_back_at_an_open_with_no_feed() {
        let rolled_back = Reverted { reverted: vec!["CLAUDE.md".into()], kept: Vec::new() };
        let recovering = |fake: &Arc<Fake>| {
            let mut opened = desk(fake, "0.2.0");
            assert!(opened.recover(PathBuf::from(FOLDER), true));
            assert_eq!(opened.stage(), &Stage::Working(Phase::Checking));
            settle(&mut opened);
            opened
        };
        let fake = Arc::new(Fake { recover: Ok(Some(rolled_back.clone())), ..Fake::ready() });
        assert_eq!(recovering(&fake).stage(), &Stage::Ended(Outcome::Recovered(rolled_back)));
        assert_eq!(fake.asked(), ["recover", "exists", "git", "base", "can undo"], "a look at the folder for About, no download, no update");

        let opened = recovering(&Arc::new(Fake::ready()));
        assert_eq!(opened.stage(), &Stage::Idle, "no update had stopped, so there is nothing to say");
        assert!(opened.can_undo(), "the worker looked whether the last update can be undone");

        let fake = Arc::new(Fake { recover: Err(UpdateError::Kept(vec!["CLAUDE.md".into()])), ..Fake::ready() });
        assert_eq!(recovering(&fake).stage(), &Stage::Ended(Outcome::RollbackFailed(Failure::Kept(vec!["CLAUDE.md".into()]))));
    }

    #[test]
    fn a_worker_that_panics_still_ends_so_runs_are_not_blocked() {
        let fake = Arc::new(Fake { panics: true, ..Fake::ready() });
        let mut desk = desk(&fake, "0.2.0");
        desk.start(PathBuf::from(FOLDER), &feed(), true);
        settle(&mut desk);
        assert_eq!(desk.stage(), &Stage::Ended(Outcome::Failed(Failure::Disk)));
        assert!(!desk.can_undo());
        desk.undo(PathBuf::from(FOLDER), true);
        settle(&mut desk);
        assert_eq!(desk.stage(), &Stage::Undid(UndoOutcome::Failed(Failure::Disk)));
        assert!(!desk.is_working());
    }

    #[test]
    fn undo_runs_on_a_worker() {
        let fake = Arc::new(Fake { undoable: false, ..Fake::ready() });
        let mut desk = desk(&fake, "0.2.0");
        desk.undo(PathBuf::from(FOLDER), true);
        assert_eq!(desk.stage(), &Stage::Working(Phase::Undoing));
        settle(&mut desk);
        assert_eq!(desk.stage(), &Stage::Undid(UndoOutcome::Undone(Reverted { reverted: vec!["CLAUDE.md".into()], kept: Vec::new() })));
        assert!(!desk.can_undo(), "as the worker found it after the undo");
        assert_eq!(fake.asked(), ["undo", "base", "can undo"], "the base is read again for About");
    }

    #[test]
    fn each_undo_error_reads_in_the_updates_words() {
        for (error, outcome) in [
            (UpdateError::NothingToUndo, UndoOutcome::NothingToUndo),
            (UpdateError::NoBase, UndoOutcome::NothingToUndo),
            (UpdateError::Busy, UndoOutcome::Failed(Failure::Busy)),
            (UpdateError::Interrupted, UndoOutcome::Failed(Failure::Busy)),
            (UpdateError::BadJournal, UndoOutcome::NothingToUndo),
            (UpdateError::Disk, UndoOutcome::Failed(Failure::Disk)),
            (UpdateError::Unnoted, UndoOutcome::Failed(Failure::Unnoted)),
        ] {
            let fake = Arc::new(Fake { undo: Err(error.clone()), ..Fake::ready() });
            assert_eq!(undo(&fake.ports(), Path::new(FOLDER)), outcome, "{error:?}");
        }
    }
}
