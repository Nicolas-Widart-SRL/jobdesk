use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use chrono::NaiveDate;

use crate::onboarding::application::HelperSetup;
use crate::onboarding::domain::{
    Contents, Failure, HELPERS, Helper, InstallPhase, Outcome, Phase, Place, Progress, ProgressStore, SourceFetcher, Step, Unpacker, suggested_folder,
};
use crate::shared_kernel::installed_source::InstalledSource;
use crate::updates::application::{Failure as FeedFailure, SourcePinReader, source_zip_name, source_zip_url};

/// Where first-run setup is, as the screens show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Set up from scratch, or open an ai-job-search folder the user already has. With a step done, the welcome back.
    Welcome,
    /// Save location: the folder setup will use, and whether it may.
    Place { folder: PathBuf, place: Place },
    /// The folder is being made and filled. `version` is known once the feed has been read. Once Cancel is pressed,
    /// `cancelling` holds until the worker has stopped and cleaned up.
    Download { folder: PathBuf, version: Option<String>, phase: Phase, cancelling: bool },
    /// The download stopped at `at`, and the folder is still empty. Try again starts afresh.
    Failed { folder: PathBuf, failure: Failure, version: Option<String>, at: Phase },
    /// ai-job-search is in the folder.
    Done { folder: PathBuf, version: String },
    /// The Computer step: each helper's line, in `HELPERS` order, and what runs now.
    Computer { folder: PathBuf, lines: [HelperLine; 3], work: Work },
    /// The Folder and Computer steps are done: the folder opens as the job search.
    Finished { folder: PathBuf },
}

/// Where one helper stands on the Computer step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperLine {
    /// Not looked at yet.
    Next,
    Checking,
    /// Its programs ran when setup checked.
    Here,
    /// Setup found it missing, and installs it on Install what is missing.
    Missing,
    /// Installing, with how far it has got once the install says.
    Installing(Option<InstallPhase>),
    Installed,
    Failed(Failure),
}

impl HelperLine {
    /// Whether the helper works now.
    pub fn ready(self) -> bool {
        matches!(self, Self::Here | Self::Installed)
    }
}

/// What the Computer step runs now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    Checking,
    /// Installing what was missing. Once Cancel is pressed, `cancelling` holds until the worker has stopped.
    Installing {
        cancelling: bool,
    },
    /// Nothing runs: the lines say what is left.
    Idle,
}

/// What the worker tells the window, tagged with the attempt it belongs to.
enum Event {
    Version(String),
    Phase(Phase),
    Ended(Result<String, Failure>),
    Helper(Helper, HelperLine),
    HelpersEnded,
}

struct Attempt {
    id: u64,
    cancelled: Arc<AtomicBool>,
    worker: JoinHandle<()>,
}

/// The ports one attempt needs on its worker thread. `pins` is `None` in a build that can vouch for no release.
#[derive(Clone)]
struct Ports {
    pins: Option<Arc<dyn SourcePinReader>>,
    fetcher: Arc<dyn SourceFetcher>,
    unpacker: Arc<dyn Unpacker>,
    helpers: HelperSetup,
}

/// First-run setup for the welcome, the folder, the download and the helpers. One worker at a time fetches, checks and
/// unpacks the release, or checks and installs the helpers, away from the window's thread. The UI reads `stage` and
/// `progress`, and acts through the methods.
pub struct Onboarding {
    store: Arc<dyn ProgressStore>,
    ports: Ports,
    documents: PathBuf,
    repaint: Arc<dyn Fn() + Send + Sync>,
    progress: Progress,
    stage: Stage,
    attempt: Option<Attempt>,
    next_id: u64,
    events: Receiver<(u64, Event)>,
    sender: Sender<(u64, Event)>,
}

impl Onboarding {
    /// Reads the progress record and removes what a setup that stopped midway left: part files in the data folder,
    /// and staging folders beside the chosen folder. A folder the record does not have as made, but which holds the
    /// release of the version it records, is made: setup stopped after the release took its place, before the record
    /// said so. `pins` is `None` in a build with no update key, which offers only a folder the user already has.
    /// `documents` is the user's Documents folder.
    pub fn new(
        store: Arc<dyn ProgressStore>,
        pins: Option<Arc<dyn SourcePinReader>>,
        fetcher: Arc<dyn SourceFetcher>,
        unpacker: Arc<dyn Unpacker>,
        helpers: HelperSetup,
        documents: PathBuf,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        // A record that cannot be read is a setup that has not started: nothing of the user's depends on it.
        let mut progress = store.load().ok().flatten().unwrap_or_default();
        fetcher.clear_leftovers();
        if let Some(folder) = &progress.folder {
            unpacker.clear_leftovers(folder);
            let kept = || progress.version.is_some() && unpacker.installed(folder) == progress.version;
            if !progress.is_done(Step::Folder) && unpacker.look(folder) == Contents::AiJobSearch && kept() {
                progress.done.insert(Step::Folder);
                // One that cannot be written is found again at the next open.
                let _ = store.save(&progress);
            }
        }
        let (sender, events) = mpsc::channel();
        let ports = Ports { pins, fetcher, unpacker, helpers };
        Self { store, ports, documents, repaint, progress, stage: Stage::Welcome, attempt: None, next_id: 0, events, sender }
    }

    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    /// Whether this build can download ai-job-search: one with no update key vouches for no release.
    pub fn can_download(&self) -> bool {
        self.ports.pins.is_some()
    }

    /// Set up from scratch: Save location opens on the folder chosen before, or on a new `Job search` folder in
    /// Documents.
    pub fn start_from_scratch(&mut self) {
        if self.stage == Stage::Welcome {
            let folder = self.progress.folder.clone().unwrap_or_else(|| suggested_folder(&self.documents));
            self.look_at(folder);
        }
    }

    /// Continue setup, on the welcome back: the Computer step, as the record is cleared once it is done. A folder that
    /// no longer holds ai-job-search starts the Folder step again on it.
    pub fn resume(&mut self) {
        if self.stage != Stage::Welcome {
            return;
        }
        let Some(folder) = self.progress.folder.clone() else { return self.start_from_scratch() };
        if !self.progress.is_done(Step::Folder) || self.ports.unpacker.look(&folder) != Contents::AiJobSearch {
            self.progress.done.clear();
            return self.look_at(folder);
        }
        self.check_helpers(folder);
    }

    /// Back: from Save location to the welcome, from a failed download to Save location, and from the helper check
    /// to the folder made. Never while something runs.
    pub fn back(&mut self) {
        match &self.stage {
            Stage::Place { .. } => self.stage = Stage::Welcome,
            Stage::Failed { folder, .. } => {
                let folder = folder.clone();
                self.look_at(folder);
            }
            Stage::Computer { folder, work: Work::Idle, .. } => {
                let (folder, version) = (folder.clone(), self.progress.version.clone().unwrap_or_default());
                self.stage = Stage::Done { folder, version };
            }
            _ => {}
        }
    }

    /// Change…: the folder the user picked, looked at again.
    pub fn choose(&mut self, folder: PathBuf) {
        if matches!(self.stage, Stage::Place { .. } | Stage::Failed { .. }) {
            self.look_at(folder);
        }
    }

    /// Use this new folder: the folder suggested inside a chosen folder that holds files.
    pub fn use_suggestion(&mut self) {
        if let Stage::Place { place: Place::NotEmpty { suggest }, .. } = &self.stage {
            let suggest = suggest.clone();
            self.look_at(suggest);
        }
    }

    /// Save here, and Try again after a failure: keeps the folder in the record, then makes it, downloads the release,
    /// checks it and unpacks it on a worker. The folder is looked at again first, so one that filled meanwhile is
    /// refused.
    pub fn save_here(&mut self, today: NaiveDate) {
        let folder = match &self.stage {
            Stage::Place { folder, .. } | Stage::Failed { folder, .. } => folder.clone(),
            _ => return,
        };
        let place = Place::of(&folder, self.ports.unpacker.look(&folder));
        if !place.usable() {
            self.stage = Stage::Place { folder, place };
            return;
        }
        self.progress.folder = Some(folder.clone());
        self.save(today);
        self.start_download(folder);
    }

    /// Continue, once the folder is ready: the Computer step checks which helpers this computer has.
    pub fn go_on(&mut self) {
        if let Stage::Done { folder, .. } = &self.stage {
            let folder = folder.clone();
            self.check_helpers(folder);
        }
    }

    /// Install what is missing, and Try again after a helper failed: installs each helper missing or failed, in order,
    /// on a worker.
    pub fn install(&mut self) {
        let Stage::Computer { folder, lines, work: Work::Idle } = &self.stage else { return };
        let wanted: Vec<Helper> =
            HELPERS.into_iter().zip(lines).filter(|(_, line)| matches!(line, HelperLine::Missing | HelperLine::Failed(_))).map(|(helper, _)| helper).collect();
        if wanted.is_empty() {
            return;
        }
        let (folder, lines) = (folder.clone(), *lines);
        self.stage = Stage::Computer { folder, lines, work: Work::Installing { cancelling: false } };
        let helpers = self.ports.helpers.clone();
        self.spawn(move |cancelled, tell| {
            for helper in wanted {
                tell(Event::Helper(helper, HelperLine::Installing(None)));
                let outcome = helpers.install(helper, cancelled, &mut |phase| tell(Event::Helper(helper, HelperLine::Installing(Some(phase)))));
                let line = match outcome {
                    Outcome::AlreadyHere => HelperLine::Here,
                    Outcome::Installed => HelperLine::Installed,
                    Outcome::CouldNotInstall(Failure::Cancelled) => HelperLine::Missing,
                    Outcome::CouldNotInstall(failure) => HelperLine::Failed(failure),
                };
                tell(Event::Helper(helper, line));
                if cancelled() {
                    break;
                }
            }
            tell(Event::HelpersEnded);
        });
    }

    /// Cancel: stops the worker. The stage holds until the worker has stopped, then goes back to Save location, or to
    /// the helpers as they were checked.
    pub fn cancel(&mut self) {
        let Some(attempt) = &self.attempt else { return };
        match &mut self.stage {
            Stage::Download { cancelling, .. } | Stage::Computer { work: Work::Installing { cancelling }, .. } => {
                attempt.cancelled.store(true, Ordering::SeqCst);
                *cancelling = true;
            }
            _ => {}
        }
    }

    /// Continue, once every helper works: the Computer step is done. Continue without PDFs, when only LaTeX does not
    /// work, ends it too, with LaTeX left out, which the tool check then finds missing. For now the record is then
    /// cleared, as nothing follows these steps yet.
    pub fn finish(&mut self, without_pdfs: bool) {
        let Stage::Computer { folder, lines, work: Work::Idle } = &self.stage else { return };
        let ready = |helper: Helper| HELPERS.into_iter().zip(lines).any(|(each, line)| each == helper && line.ready());
        let may = if without_pdfs { ready(Helper::Python) && ready(Helper::Bun) && !ready(Helper::Latex) } else { lines.iter().all(|line| line.ready()) };
        if !may {
            return;
        }
        let folder = folder.clone();
        // A record that cannot be removed costs only a welcome back at the next open, which checks the helpers again.
        let _ = self.store.clear();
        self.stage = Stage::Finished { folder };
    }

    /// Stops the worker, if one runs, and waits until it has, so nothing setup does outlives it: the download, the
    /// unpacking or an install ends short and cleans up as Cancel does. The window then leaves setup.
    pub fn stop(&mut self) {
        if let Some(attempt) = self.attempt.take() {
            attempt.cancelled.store(true, Ordering::SeqCst);
            let _ = attempt.worker.join();
        }
    }

    /// Takes what the worker said since the last call. Call it every frame.
    pub fn poll(&mut self, today: NaiveDate) {
        while let Ok((id, event)) = self.events.try_recv() {
            if self.attempt.as_ref().is_none_or(|attempt| attempt.id != id) {
                continue;
            }
            match (event, &mut self.stage) {
                (Event::Version(found), Stage::Download { version, .. }) => *version = Some(found),
                (Event::Phase(next), Stage::Download { phase, .. }) => *phase = next,
                (Event::Ended(end), Stage::Download { folder, version, phase, .. }) => {
                    let (folder, version, at) = (folder.clone(), version.clone(), *phase);
                    self.attempt = None;
                    self.end(folder, end, version, at, today);
                }
                (Event::Helper(helper, line), Stage::Computer { lines, .. }) => {
                    if let Some(index) = HELPERS.iter().position(|each| *each == helper) {
                        lines[index] = line;
                    }
                }
                (Event::HelpersEnded, Stage::Computer { work, .. }) => {
                    *work = Work::Idle;
                    self.attempt = None;
                }
                _ => {}
            }
        }
    }

    /// Whether a worker is still running, so the window keeps polling.
    pub fn is_working(&self) -> bool {
        self.attempt.is_some()
    }

    fn end(&mut self, folder: PathBuf, end: Result<String, Failure>, version: Option<String>, at: Phase, today: NaiveDate) {
        match end {
            Ok(version) => {
                self.progress.done.insert(Step::Folder);
                self.progress.version = Some(version.clone());
                self.save(today);
                self.stage = Stage::Done { folder, version };
            }
            Err(Failure::Cancelled) => self.look_at(folder),
            Err(failure) => self.stage = Stage::Failed { folder, failure, version, at },
        }
    }

    fn look_at(&mut self, folder: PathBuf) {
        let place = Place::of(&folder, self.ports.unpacker.look(&folder));
        self.stage = Stage::Place { folder, place };
    }

    /// Keeps the record. One that cannot be written costs only the resume at the next open, so setup goes on with what
    /// it holds in memory.
    fn save(&mut self, today: NaiveDate) {
        self.progress.stopped = Some(today);
        let _ = self.store.save(&self.progress);
    }

    /// Runs `work` on a worker as a new attempt. What it tells reaches `poll`, and wakes the window.
    fn spawn(&mut self, work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Event)) + Send + 'static) {
        let id = self.next_id;
        self.next_id += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        let (sender, repaint, stop) = (self.sender.clone(), Arc::clone(&self.repaint), Arc::clone(&cancelled));
        let worker = thread::spawn(move || {
            let tell = |event: Event| {
                let _ = sender.send((id, event));
                repaint();
            };
            work(&|| stop.load(Ordering::SeqCst), &tell);
        });
        self.attempt = Some(Attempt { id, cancelled, worker });
    }

    /// Makes the folder and fills it on a worker, which keeps the record as it goes: the version once the feed has
    /// pinned it, and the folder made once the release has taken its place, before the window hears of it.
    fn start_download(&mut self, folder: PathBuf) {
        self.stage = Stage::Download { folder: folder.clone(), version: None, phase: Phase::Made, cancelling: false };
        let (ports, store, progress) = (self.ports.clone(), Arc::clone(&self.store), self.progress.clone());
        self.spawn(move |cancelled, tell| {
            let end = set_up(&ports, &Recorder { store, progress }, &folder, cancelled, tell);
            tell(Event::Ended(end));
        });
    }

    /// Checks each helper in turn on a worker. Nothing is installed until Install what is missing.
    fn check_helpers(&mut self, folder: PathBuf) {
        self.stage = Stage::Computer { folder, lines: [HelperLine::Next; 3], work: Work::Checking };
        let helpers = self.ports.helpers.clone();
        self.spawn(move |cancelled, tell| {
            for helper in HELPERS {
                if cancelled() {
                    break;
                }
                tell(Event::Helper(helper, HelperLine::Checking));
                let line = if helpers.is_here(helper) { HelperLine::Here } else { HelperLine::Missing };
                tell(Event::Helper(helper, line));
            }
            tell(Event::HelpersEnded);
        });
    }
}

/// The record as the worker keeps it, from the one the window held when the download started.
struct Recorder {
    store: Arc<dyn ProgressStore>,
    progress: Progress,
}

impl Recorder {
    /// Records `version`, and the folder as made when `made`. One that cannot be written costs only the resume at the
    /// next open, as the window holds the same in memory once it hears.
    fn record(&self, version: &str, made: bool) {
        let mut progress = self.progress.clone();
        progress.version = Some(version.to_owned());
        if made {
            progress.done.insert(Step::Folder);
        }
        let _ = self.store.save(&progress);
    }
}

/// Makes the folder, reads the pin, downloads the release, checks it and unpacks it. Gives the version unpacked.
fn set_up(ports: &Ports, recorder: &Recorder, folder: &Path, cancelled: &dyn Fn() -> bool, tell: &dyn Fn(Event)) -> Result<String, Failure> {
    ports.unpacker.make(folder)?;
    tell(Event::Phase(Phase::Made));
    let pin = match ports.pins.as_ref().map(|pins| pins.read()) {
        Some(Ok(Some(pin))) => pin,
        // No update key, or a feed that answered but vouches for nothing this copy can trust.
        None | Some(Ok(None)) => return Err(Failure::NoPin),
        Some(Err(failure)) => return Err(pin_failure(failure)),
    };
    let version = pin.version.to_string();
    // So the next open knows which release a folder filled before the window heard is.
    recorder.record(&version, false);
    tell(Event::Version(version.clone()));
    if cancelled() {
        return Err(Failure::Cancelled);
    }
    tell(Event::Phase(Phase::Downloading { received: 0 }));
    let zip = ports.fetcher.fetch(&source_zip_url(&pin.version), &source_zip_name(&pin.version), cancelled, &mut |received| {
        tell(Event::Phase(Phase::Downloading { received }));
    })?;
    let source = InstalledSource { version: version.clone(), sha256: pin.sha256 };
    ports.unpacker.unpack(&zip, &source, folder, cancelled, &mut |phase| tell(Event::Phase(phase)))?;
    // The release has taken its place: quitting before the window hears must not leave the folder unfinished.
    recorder.record(&version, true);
    Ok(version)
}

/// Why the feed could not give a pin, in setup's words: only a feed that could not be reached is `Offline`.
fn pin_failure(failure: FeedFailure) -> Failure {
    match failure {
        FeedFailure::TimedOut | FeedFailure::Unreachable | FeedFailure::Interrupted => Failure::Offline,
        FeedFailure::Refused(code) => Failure::Refused(code),
        FeedFailure::Disk => Failure::Disk,
        // The feed answered, but with nothing that reads as a pin.
        FeedFailure::BadFeed | FeedFailure::NoBuild | FeedFailure::TooLarge => Failure::NoPin,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::onboarding::domain::{HelperInstaller, HelperProbe};
    use crate::shared_kernel::LoadError;
    use crate::updates::application::SourcePin;

    const HASH: &str = "0000000000000000000000000000000000000000000000000000000000000001";

    #[derive(Default)]
    struct Memory(Mutex<Option<Progress>>);

    impl ProgressStore for Memory {
        fn load(&self) -> Result<Option<Progress>, LoadError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&self, progress: &Progress) -> Result<(), LoadError> {
            *self.0.lock().unwrap() = Some(progress.clone());
            Ok(())
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

    fn pinned() -> Pins {
        Pins(Ok(Some(SourcePin::parse("1.7.5", HASH).unwrap())))
    }

    /// A fetcher that never reaches the network: it waits for Cancel when `hold` is set, and otherwise hands back a
    /// path that only the fake unpacker reads.
    #[derive(Default)]
    struct Fetcher {
        hold: bool,
        asked: Mutex<Vec<String>>,
        /// Set once a fetch has returned.
        ended: AtomicBool,
    }

    impl SourceFetcher for Fetcher {
        fn fetch(&self, url: &str, name: &str, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64)) -> Result<PathBuf, Failure> {
            self.asked.lock().unwrap().push(format!("{url} {name}"));
            progress(10);
            while self.hold && !cancelled() {
                thread::sleep(Duration::from_millis(5));
            }
            // A little later than the cancel, so a caller that does not wait for the worker sees it still running.
            thread::sleep(Duration::from_millis(20));
            self.ended.store(true, Ordering::SeqCst);
            if cancelled() {
                return Err(Failure::Cancelled);
            }
            Ok(PathBuf::from("/fictional/onboarding/1/release.zip.part"))
        }
        fn clear_leftovers(&self) {}
    }

    /// Folders by what they hold, and the hash the release must have. An unpacked folder holds ai-job-search, and
    /// keeps the version unpacked.
    struct Folders {
        contents: Mutex<Contents>,
        installed: Mutex<Option<String>>,
        cleared: Mutex<Vec<PathBuf>>,
        unpacked: Mutex<Vec<String>>,
    }

    impl Folders {
        fn holding(contents: Contents) -> Self {
            Folders { contents: Mutex::new(contents), installed: Mutex::default(), cleared: Mutex::default(), unpacked: Mutex::default() }
        }
    }

    impl Unpacker for Folders {
        fn look(&self, _folder: &Path) -> Contents {
            *self.contents.lock().unwrap()
        }
        fn make(&self, _folder: &Path) -> Result<(), Failure> {
            match *self.contents.lock().unwrap() {
                Contents::Missing | Contents::Empty => Ok(()),
                _ => Err(Failure::NotEmpty),
            }
        }
        fn unpack(
            &self,
            _zip: &Path,
            source: &InstalledSource,
            folder: &Path,
            _cancelled: &dyn Fn() -> bool,
            phase: &mut dyn FnMut(Phase),
        ) -> Result<(), Failure> {
            phase(Phase::Checking);
            assert_eq!(source.version, "1.7.5", "the pinned version goes with its hash");
            if source.sha256 != HASH {
                return Err(Failure::Mismatch);
            }
            phase(Phase::Unpacking);
            self.unpacked.lock().unwrap().push(folder.display().to_string());
            *self.contents.lock().unwrap() = Contents::AiJobSearch;
            *self.installed.lock().unwrap() = Some(source.version.clone());
            Ok(())
        }
        fn installed(&self, _folder: &Path) -> Option<String> {
            self.installed.lock().unwrap().clone()
        }
        fn clear_leftovers(&self, folder: &Path) {
            self.cleared.lock().unwrap().push(folder.to_owned());
        }
    }

    /// The helpers whose programs run.
    #[derive(Default)]
    struct Here(Mutex<BTreeSet<Helper>>);

    impl HelperProbe for Here {
        fn is_here(&self, helper: Helper) -> bool {
            self.0.lock().unwrap().contains(&helper)
        }
    }

    /// Installs a helper into `here`, fails it with its scripted failure, or waits for Cancel when `hold` is set.
    #[derive(Default)]
    struct Installer {
        here: Arc<Here>,
        fails: Mutex<BTreeMap<Helper, Failure>>,
        hold: bool,
    }

    impl HelperInstaller for Installer {
        fn install(&self, helper: Helper, _: &dyn Fn() -> bool, cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(InstallPhase)) -> Result<(), Failure> {
            phase(InstallPhase::Downloading { received: 1, total: 4 });
            while self.hold && !cancelled() {
                thread::sleep(Duration::from_millis(5));
            }
            if cancelled() {
                return Err(Failure::Cancelled);
            }
            if let Some(failure) = self.fails.lock().unwrap().get(&helper) {
                return Err(*failure);
            }
            self.here.0.lock().unwrap().insert(helper);
            Ok(())
        }
        fn clear_leftovers(&self) {}
    }

    fn installer(here: &[Helper]) -> Arc<Installer> {
        Arc::new(Installer { here: Arc::new(Here(Mutex::new(here.iter().copied().collect()))), ..Installer::default() })
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    fn onboarding(store: Arc<Memory>, pins: Pins, fetcher: Arc<Fetcher>, folders: Arc<Folders>) -> Onboarding {
        with_helpers(store, pins, fetcher, folders, installer(&[]))
    }

    fn with_helpers(store: Arc<Memory>, pins: Pins, fetcher: Arc<Fetcher>, folders: Arc<Folders>, installer: Arc<Installer>) -> Onboarding {
        let helpers = HelperSetup::new(Arc::clone(&installer.here) as Arc<dyn HelperProbe>, installer);
        Onboarding::new(store, Some(Arc::new(pins)), fetcher, folders, helpers, PathBuf::from("/Users/sam/Documents"), Arc::new(|| {}))
    }

    /// Polls until the worker has ended, or fails the test after two seconds.
    fn settle(onboarding: &mut Onboarding) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while onboarding.is_working() {
            assert!(Instant::now() < deadline, "the worker never ended");
            onboarding.poll(today());
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// Polls until `seen` holds of the stage, or fails the test after two seconds.
    fn wait_for(onboarding: &mut Onboarding, seen: impl Fn(&Stage) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !seen(onboarding.stage()) {
            assert!(Instant::now() < deadline, "never reached: {:?}", onboarding.stage());
            onboarding.poll(today());
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn save_here_makes_downloads_and_unpacks_then_records_the_folder() {
        let (store, fetcher, folders) = (Arc::new(Memory::default()), Arc::new(Fetcher::default()), Arc::new(Folders::holding(Contents::Missing)));
        let mut onboarding = onboarding(Arc::clone(&store), pinned(), Arc::clone(&fetcher), Arc::clone(&folders));
        onboarding.start_from_scratch();
        let folder = PathBuf::from("/Users/sam/Documents/Job search");
        assert_eq!(onboarding.stage(), &Stage::Place { folder: folder.clone(), place: Place::New });
        onboarding.save_here(today());
        settle(&mut onboarding);
        assert_eq!(onboarding.stage(), &Stage::Done { folder: folder.clone(), version: "1.7.5".into() });
        assert_eq!(
            *fetcher.asked.lock().unwrap(),
            ["https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.7.5 ai-job-search-v1.7.5.zip"]
        );
        assert_eq!(*folders.unpacked.lock().unwrap(), ["/Users/sam/Documents/Job search"]);
        let saved = store.load().unwrap().unwrap();
        assert_eq!(saved.folder, Some(folder));
        assert!(saved.is_done(Step::Folder));
        assert_eq!(saved.version.as_deref(), Some("1.7.5"));
        assert_eq!(saved.stopped, Some(today()));
    }

    #[test]
    fn a_folder_with_files_is_refused_with_a_new_folder_inside_it() {
        let folders = Arc::new(Folders::holding(Contents::Files));
        let mut onboarding = onboarding(Arc::default(), pinned(), Arc::default(), Arc::clone(&folders));
        onboarding.start_from_scratch();
        onboarding.choose(PathBuf::from("/Users/sam/Desktop"));
        let suggest = PathBuf::from("/Users/sam/Desktop/Job search");
        assert_eq!(onboarding.stage(), &Stage::Place { folder: PathBuf::from("/Users/sam/Desktop"), place: Place::NotEmpty { suggest: suggest.clone() } });
        onboarding.save_here(today());
        assert!(!onboarding.is_working(), "Save here does nothing with a refused folder");
        *folders.contents.lock().unwrap() = Contents::Missing;
        onboarding.use_suggestion();
        assert_eq!(onboarding.stage(), &Stage::Place { folder: suggest, place: Place::New });
    }

    #[test]
    fn a_folder_that_filled_since_it_was_chosen_is_looked_at_again() {
        let folders = Arc::new(Folders::holding(Contents::Empty));
        let mut onboarding = onboarding(Arc::default(), pinned(), Arc::default(), Arc::clone(&folders));
        onboarding.start_from_scratch();
        *folders.contents.lock().unwrap() = Contents::AiJobSearch;
        onboarding.save_here(today());
        assert!(!onboarding.is_working());
        assert!(matches!(onboarding.stage(), Stage::Place { place: Place::AlreadyAiJobSearch, .. }));
    }

    #[test]
    fn no_pin_and_no_feed_each_say_why_and_try_again_starts_afresh() {
        let cases = [
            (Pins(Ok(None)), Failure::NoPin),
            (Pins(Err(FeedFailure::Unreachable)), Failure::Offline),
            (Pins(Err(FeedFailure::TimedOut)), Failure::Offline),
            // GitHub refusing or rate limiting the feed is not a lost connection.
            (Pins(Err(FeedFailure::Refused(403))), Failure::Refused(403)),
            (Pins(Err(FeedFailure::Refused(503))), Failure::Refused(503)),
            (Pins(Err(FeedFailure::BadFeed)), Failure::NoPin),
        ];
        for (pins, failure) in cases {
            let fetcher = Arc::new(Fetcher::default());
            let mut onboarding = onboarding(Arc::default(), pins, Arc::clone(&fetcher), Arc::new(Folders::holding(Contents::Missing)));
            onboarding.start_from_scratch();
            onboarding.save_here(today());
            settle(&mut onboarding);
            assert!(matches!(onboarding.stage(), Stage::Failed { failure: found, .. } if *found == failure), "{:?}", onboarding.stage());
            assert!(fetcher.asked.lock().unwrap().is_empty(), "nothing is downloaded without a pin");
            onboarding.save_here(today());
            assert!(onboarding.is_working(), "Try again starts a new attempt");
            settle(&mut onboarding);
        }
    }

    #[test]
    fn a_build_without_an_update_key_downloads_nothing() {
        let fetcher = Arc::new(Fetcher::default());
        let helpers = HelperSetup::new(Arc::new(Here::default()), installer(&[]));
        let folders = Arc::new(Folders::holding(Contents::Missing));
        let documents = PathBuf::from("/Users/sam/Documents");
        let mut onboarding = Onboarding::new(Arc::new(Memory::default()), None, Arc::clone(&fetcher) as _, folders, helpers, documents, Arc::new(|| {}));
        assert!(!onboarding.can_download());
        onboarding.start_from_scratch();
        onboarding.save_here(today());
        settle(&mut onboarding);
        assert!(matches!(onboarding.stage(), Stage::Failed { failure: Failure::NoPin, .. }));
        assert!(fetcher.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn a_mismatch_fails_at_the_check_and_nothing_is_recorded_as_done() {
        let store = Arc::new(Memory::default());
        let pins = Pins(Ok(Some(SourcePin::parse("1.7.5", &HASH.replace('1', "2")).unwrap())));
        let folders = Arc::new(Folders::holding(Contents::Missing));
        let mut onboarding = onboarding(Arc::clone(&store), pins, Arc::default(), Arc::clone(&folders));
        onboarding.start_from_scratch();
        onboarding.save_here(today());
        settle(&mut onboarding);
        assert!(
            matches!(onboarding.stage(), Stage::Failed { failure: Failure::Mismatch, version: Some(version), at: Phase::Checking, .. } if version == "1.7.5"),
            "{:?}",
            onboarding.stage()
        );
        assert!(folders.unpacked.lock().unwrap().is_empty());
        assert!(!store.load().unwrap().unwrap().is_done(Step::Folder));
        onboarding.back();
        assert!(matches!(onboarding.stage(), Stage::Place { .. }), "Back leads to Save location");
    }

    #[test]
    fn cancel_holds_until_the_worker_stops_then_goes_back_to_the_folder() {
        let fetcher = Arc::new(Fetcher { hold: true, ..Fetcher::default() });
        let mut onboarding = onboarding(Arc::default(), pinned(), fetcher, Arc::new(Folders::holding(Contents::Missing)));
        onboarding.start_from_scratch();
        onboarding.save_here(today());
        onboarding.cancel();
        assert!(matches!(onboarding.stage(), Stage::Download { cancelling: true, .. }));
        settle(&mut onboarding);
        assert!(matches!(onboarding.stage(), Stage::Place { place: Place::New, .. }), "{:?}", onboarding.stage());
    }

    #[test]
    fn the_next_open_clears_leftovers_beside_the_recorded_folder_and_offers_it_again() {
        let folder = PathBuf::from("/Users/sam/Job search");
        let store = Arc::new(Memory(Mutex::new(Some(Progress { folder: Some(folder.clone()), ..Progress::default() }))));
        let folders = Arc::new(Folders::holding(Contents::Empty));
        let mut onboarding = onboarding(store, pinned(), Arc::default(), Arc::clone(&folders));
        assert_eq!(*folders.cleared.lock().unwrap(), std::slice::from_ref(&folder));
        onboarding.start_from_scratch();
        assert_eq!(onboarding.stage(), &Stage::Place { folder, place: Place::Empty });
    }

    /// A setup with its folder made, on the helper check once it has ended.
    fn on_the_computer_step(store: Arc<Memory>, installer: Arc<Installer>) -> Onboarding {
        let mut onboarding = with_helpers(store, pinned(), Arc::default(), Arc::new(Folders::holding(Contents::Missing)), installer);
        onboarding.start_from_scratch();
        onboarding.save_here(today());
        settle(&mut onboarding);
        onboarding.go_on();
        settle(&mut onboarding);
        onboarding
    }

    fn lines(onboarding: &Onboarding) -> [HelperLine; 3] {
        match onboarding.stage() {
            Stage::Computer { lines, .. } => *lines,
            stage => panic!("not on the Computer step: {stage:?}"),
        }
    }

    #[test]
    fn the_helpers_are_checked_then_only_the_missing_ones_install_and_continue_clears_the_record() {
        let store = Arc::new(Memory::default());
        let mut onboarding = on_the_computer_step(Arc::clone(&store), installer(&[Helper::Python]));
        assert_eq!(lines(&onboarding), [HelperLine::Here, HelperLine::Missing, HelperLine::Missing]);
        onboarding.finish(false);
        assert!(matches!(onboarding.stage(), Stage::Computer { .. }), "Continue waits for every helper");
        onboarding.install();
        settle(&mut onboarding);
        assert_eq!(lines(&onboarding), [HelperLine::Here, HelperLine::Installed, HelperLine::Installed]);
        onboarding.finish(false);
        assert_eq!(onboarding.stage(), &Stage::Finished { folder: PathBuf::from("/Users/sam/Documents/Job search") });
        assert_eq!(store.load().unwrap(), None, "the record is cleared");
    }

    #[test]
    fn a_failed_latex_may_be_left_out_but_python_or_bun_may_not() {
        let latex = installer(&[]);
        latex.fails.lock().unwrap().insert(Helper::Latex, Failure::Offline);
        let mut onboarding = on_the_computer_step(Arc::default(), latex);
        onboarding.install();
        settle(&mut onboarding);
        assert_eq!(lines(&onboarding), [HelperLine::Installed, HelperLine::Installed, HelperLine::Failed(Failure::Offline)]);
        onboarding.finish(false);
        assert!(matches!(onboarding.stage(), Stage::Computer { .. }));
        onboarding.finish(true);
        assert!(matches!(onboarding.stage(), Stage::Finished { .. }));

        let bun = installer(&[]);
        bun.fails.lock().unwrap().insert(Helper::Bun, Failure::Mismatch);
        let mut onboarding = on_the_computer_step(Arc::default(), Arc::clone(&bun));
        onboarding.install();
        settle(&mut onboarding);
        onboarding.finish(true);
        assert!(matches!(onboarding.stage(), Stage::Computer { .. }), "Bun is needed");
        bun.fails.lock().unwrap().clear();
        onboarding.install();
        settle(&mut onboarding);
        assert_eq!(lines(&onboarding), [HelperLine::Installed; 3], "Try again installs only what failed");
    }

    #[test]
    fn cancel_stops_the_install_and_leaves_the_helper_missing() {
        let held = Arc::new(Installer { hold: true, ..Installer::default() });
        let mut onboarding = on_the_computer_step(Arc::default(), held);
        onboarding.install();
        wait_for(&mut onboarding, |stage| matches!(stage, Stage::Computer { lines: [HelperLine::Installing(Some(_)), ..], .. }));
        onboarding.cancel();
        assert!(matches!(onboarding.stage(), Stage::Computer { work: Work::Installing { cancelling: true }, .. }));
        settle(&mut onboarding);
        let folder = PathBuf::from("/Users/sam/Documents/Job search");
        assert_eq!(onboarding.stage(), &Stage::Computer { folder, lines: [HelperLine::Missing; 3], work: Work::Idle });
        onboarding.back();
        assert!(matches!(onboarding.stage(), Stage::Done { version, .. } if version == "1.7.5"), "Back leads to the folder made");
    }

    #[test]
    fn continue_setup_picks_up_at_the_first_step_not_done() {
        let folder = PathBuf::from("/Users/sam/Job search");
        let made = Progress { folder: Some(folder.clone()), done: BTreeSet::from([Step::Folder]), ..Progress::default() };
        let store = Arc::new(Memory(Mutex::new(Some(made.clone()))));
        let mut set_up = with_helpers(store, pinned(), Arc::default(), Arc::new(Folders::holding(Contents::AiJobSearch)), installer(&HELPERS));
        set_up.resume();
        settle(&mut set_up);
        assert_eq!(lines(&set_up), [HelperLine::Here; 3]);

        // A folder that no longer holds ai-job-search starts the Folder step again on it.
        let store = Arc::new(Memory(Mutex::new(Some(made))));
        let mut onboarding = onboarding(store, pinned(), Arc::default(), Arc::new(Folders::holding(Contents::Missing)));
        onboarding.resume();
        assert_eq!(onboarding.stage(), &Stage::Place { folder, place: Place::New });
        assert!(!onboarding.progress().is_done(Step::Folder));
    }

    /// Polls the store, never the setup, until `seen` holds of the record, or fails the test after two seconds.
    fn wait_for_record(store: &Memory, seen: impl Fn(&Progress) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !store.load().unwrap().is_some_and(|progress| seen(&progress)) {
            assert!(Instant::now() < deadline, "never recorded: {:?}", store.load());
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn the_worker_records_the_folder_made_before_the_window_hears() {
        let store = Arc::new(Memory::default());
        let mut onboarding = onboarding(Arc::clone(&store), pinned(), Arc::default(), Arc::new(Folders::holding(Contents::Missing)));
        onboarding.start_from_scratch();
        onboarding.save_here(today());
        wait_for_record(&store, |progress| progress.is_done(Step::Folder));
        let saved = store.load().unwrap().unwrap();
        assert_eq!(saved.version.as_deref(), Some("1.7.5"));
        assert_eq!(saved.folder, Some(PathBuf::from("/Users/sam/Documents/Job search")));
        assert!(matches!(onboarding.stage(), Stage::Download { .. }), "not polled yet: {:?}", onboarding.stage());
        settle(&mut onboarding);
        assert!(matches!(onboarding.stage(), Stage::Done { .. }));
    }

    #[test]
    fn a_setup_that_stopped_before_the_window_heard_picks_up_at_the_computer_step() {
        let folder = PathBuf::from("/Users/sam/Job search");
        // The record as the worker leaves it once the feed pinned 1.7.5, before the release took the folder's place.
        let pinned_only = Progress { folder: Some(folder.clone()), version: Some("1.7.5".into()), stopped: Some(today()), ..Progress::default() };
        let filled = |kept: Option<&str>, contents: Contents| {
            let folders = Folders::holding(contents);
            *folders.installed.lock().unwrap() = kept.map(str::to_owned);
            Arc::new(folders)
        };
        let store = Arc::new(Memory(Mutex::new(Some(pinned_only.clone()))));
        let mut resumed = with_helpers(Arc::clone(&store), pinned(), Arc::default(), filled(Some("1.7.5"), Contents::AiJobSearch), installer(&HELPERS));
        assert!(resumed.progress().is_done(Step::Folder), "the release of the recorded version is in the folder");
        assert!(store.load().unwrap().unwrap().is_done(Step::Folder), "and the record says so now");
        resumed.resume();
        settle(&mut resumed);
        assert_eq!(lines(&resumed), [HelperLine::Here; 3], "on the Computer step");

        let unmade = [
            (Some(pinned_only.clone()), filled(Some("1.7.4"), Contents::AiJobSearch), "another version is kept"),
            (Some(pinned_only.clone()), filled(None, Contents::AiJobSearch), "no release is kept"),
            (Some(pinned_only.clone()), filled(Some("1.7.5"), Contents::Files), "the folder is not ai-job-search"),
            (Some(Progress { version: None, ..pinned_only }), filled(Some("1.7.5"), Contents::AiJobSearch), "no version recorded"),
        ];
        for (record, folders, why) in unmade {
            let mut onboarding = onboarding(Arc::new(Memory(Mutex::new(record))), pinned(), Arc::default(), folders);
            assert!(!onboarding.progress().is_done(Step::Folder), "{why}");
            onboarding.resume();
            assert!(matches!(onboarding.stage(), Stage::Place { .. }), "{why}: {:?}", onboarding.stage());
        }
    }

    /// A record that can be read but never written.
    struct Unwritable;

    impl ProgressStore for Unwritable {
        fn load(&self) -> Result<Option<Progress>, LoadError> {
            Ok(None)
        }
        fn save(&self, _: &Progress) -> Result<(), LoadError> {
            Err(LoadError::new("onboarding.json", "read-only disk"))
        }
        fn clear(&self) -> Result<(), LoadError> {
            Err(LoadError::new("onboarding.json", "read-only disk"))
        }
    }

    #[test]
    fn a_record_that_cannot_be_written_keeps_setup_going_from_memory() {
        let helpers = HelperSetup::new(Arc::new(Here::default()), installer(&[]));
        let (folders, documents) = (Arc::new(Folders::holding(Contents::Missing)), PathBuf::from("/Users/sam/Documents"));
        let mut onboarding =
            Onboarding::new(Arc::new(Unwritable), Some(Arc::new(pinned())), Arc::new(Fetcher::default()), folders, helpers, documents, Arc::new(|| {}));
        onboarding.start_from_scratch();
        onboarding.save_here(today());
        settle(&mut onboarding);
        assert!(matches!(onboarding.stage(), Stage::Done { .. }), "{:?}", onboarding.stage());
        assert!(onboarding.progress().is_done(Step::Folder));
        assert_eq!(onboarding.progress().version.as_deref(), Some("1.7.5"));
        onboarding.go_on();
        assert!(matches!(onboarding.stage(), Stage::Computer { .. }));
    }

    #[test]
    fn stop_cancels_the_worker_and_waits_for_it() {
        let fetcher = Arc::new(Fetcher { hold: true, ..Fetcher::default() });
        let folders = Arc::new(Folders::holding(Contents::Missing));
        let mut onboarding = onboarding(Arc::default(), pinned(), Arc::clone(&fetcher), Arc::clone(&folders));
        onboarding.start_from_scratch();
        onboarding.save_here(today());
        wait_for(&mut onboarding, |stage| matches!(stage, Stage::Download { phase: Phase::Downloading { .. }, .. }));
        onboarding.stop();
        assert!(fetcher.ended.load(Ordering::SeqCst), "the download has ended by the time stop returns");
        assert!(!onboarding.is_working());
        assert!(folders.unpacked.lock().unwrap().is_empty());
        onboarding.stop();
    }
}
