use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use chrono::NaiveDate;

use crate::onboarding::domain::{Failure, Phase, Place, Progress, ProgressStore, SourceFetcher, Step, Unpacker, suggested_folder};
use crate::updates::application::{SourcePinReader, source_zip_name, source_zip_url};

/// Where first-run setup is, as the screens show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// Set up from scratch, or open an ai-job-search folder the user already has.
    Welcome,
    /// Save location: the folder setup will use, and whether it may.
    Place { folder: PathBuf, place: Place },
    /// The folder is being made and filled. `version` is known once the feed has been read. Once Cancel is pressed,
    /// `cancelling` holds until the worker has stopped and cleaned up.
    Download { folder: PathBuf, version: Option<String>, phase: Phase, cancelling: bool },
    /// The download stopped, and the folder is still empty. Try again starts afresh.
    Failed { folder: PathBuf, failure: Failure },
    /// ai-job-search is in the folder.
    Done { folder: PathBuf, version: String },
}

/// What the worker tells the window, tagged with the attempt it belongs to.
enum Event {
    Version(String),
    Phase(Phase),
    Ended(Result<String, Failure>),
}

struct Attempt {
    id: u64,
    cancelled: Arc<AtomicBool>,
}

/// The ports one attempt needs on its worker thread.
#[derive(Clone)]
struct Ports {
    pins: Arc<dyn SourcePinReader>,
    fetcher: Arc<dyn SourceFetcher>,
    unpacker: Arc<dyn Unpacker>,
}

/// First-run setup for the welcome, the folder and the download. One worker at a time fetches, checks and unpacks
/// the release, away from the window's thread. The UI reads `stage` and `progress`, and acts through the methods.
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
    /// and staging folders beside the chosen folder. `documents` is the user's Documents folder.
    pub fn new(
        store: Arc<dyn ProgressStore>,
        pins: Arc<dyn SourcePinReader>,
        fetcher: Arc<dyn SourceFetcher>,
        unpacker: Arc<dyn Unpacker>,
        documents: PathBuf,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        // A record that cannot be read is a setup that has not started: nothing of the user's depends on it.
        let progress = store.load().ok().flatten().unwrap_or_default();
        fetcher.clear_leftovers();
        if let Some(folder) = &progress.folder {
            unpacker.clear_leftovers(folder);
        }
        let (sender, events) = mpsc::channel();
        let ports = Ports { pins, fetcher, unpacker };
        Self { store, ports, documents, repaint, progress, stage: Stage::Welcome, attempt: None, next_id: 0, events, sender }
    }

    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    /// Set up from scratch: Save location opens on the folder chosen before, or on a new `Job search` folder in
    /// Documents.
    pub fn start_from_scratch(&mut self) {
        if self.stage == Stage::Welcome {
            let folder = self.progress.folder.clone().unwrap_or_else(|| suggested_folder(&self.documents));
            self.look_at(folder);
        }
    }

    /// Back, from Save location to the welcome.
    pub fn back(&mut self) {
        if matches!(self.stage, Stage::Place { .. }) {
            self.stage = Stage::Welcome;
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
        self.start(folder);
    }

    /// Cancel: stops the worker. The stage holds until the worker has stopped, then goes back to Save location.
    pub fn cancel(&mut self) {
        if let (Some(attempt), Stage::Download { cancelling, .. }) = (&self.attempt, &mut self.stage) {
            attempt.cancelled.store(true, Ordering::SeqCst);
            *cancelling = true;
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
                (Event::Ended(end), Stage::Download { folder, .. }) => {
                    let folder = folder.clone();
                    self.attempt = None;
                    self.end(folder, end, today);
                }
                _ => {}
            }
        }
    }

    /// Whether a worker is still running, so the window keeps polling.
    pub fn is_working(&self) -> bool {
        self.attempt.is_some()
    }

    fn end(&mut self, folder: PathBuf, end: Result<String, Failure>, today: NaiveDate) {
        match end {
            Ok(version) => {
                self.progress.done.insert(Step::Folder);
                self.progress.version = Some(version.clone());
                self.save(today);
                self.stage = Stage::Done { folder, version };
            }
            Err(Failure::Cancelled) => self.look_at(folder),
            Err(failure) => self.stage = Stage::Failed { folder, failure },
        }
    }

    fn look_at(&mut self, folder: PathBuf) {
        let place = Place::of(&folder, self.ports.unpacker.look(&folder));
        self.stage = Stage::Place { folder, place };
    }

    /// Keeps the record. One that cannot be written costs only the resume at the next open, so setup goes on.
    fn save(&mut self, today: NaiveDate) {
        self.progress.stopped = Some(today);
        let _ = self.store.save(&self.progress);
    }

    fn start(&mut self, folder: PathBuf) {
        let id = self.next_id;
        self.next_id += 1;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.attempt = Some(Attempt { id, cancelled: Arc::clone(&cancelled) });
        self.stage = Stage::Download { folder: folder.clone(), version: None, phase: Phase::Made, cancelling: false };
        let (ports, sender, repaint) = (self.ports.clone(), self.sender.clone(), Arc::clone(&self.repaint));
        thread::spawn(move || {
            let tell = |event: Event| {
                let _ = sender.send((id, event));
                repaint();
            };
            let end = set_up(&ports, &folder, &|| cancelled.load(Ordering::SeqCst), &tell);
            tell(Event::Ended(end));
        });
    }
}

/// Makes the folder, reads the pin, downloads the release, checks it and unpacks it. Gives the version unpacked.
fn set_up(ports: &Ports, folder: &Path, cancelled: &dyn Fn() -> bool, tell: &dyn Fn(Event)) -> Result<String, Failure> {
    ports.unpacker.make(folder)?;
    tell(Event::Phase(Phase::Made));
    let pin = match ports.pins.read() {
        Ok(Some(pin)) => pin,
        // A feed that answered but vouches for nothing this copy can trust.
        Ok(None) => return Err(Failure::NoPin),
        Err(_) => return Err(Failure::Offline),
    };
    let version = pin.version.to_string();
    tell(Event::Version(version.clone()));
    if cancelled() {
        return Err(Failure::Cancelled);
    }
    tell(Event::Phase(Phase::Downloading { received: 0 }));
    let zip = ports.fetcher.fetch(&source_zip_url(&pin.version), &source_zip_name(&pin.version), cancelled, &mut |received| {
        tell(Event::Phase(Phase::Downloading { received }));
    })?;
    ports.unpacker.unpack(&zip, &pin.sha256, folder, cancelled, &mut |phase| tell(Event::Phase(phase)))?;
    Ok(version)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::onboarding::domain::Contents;
    use crate::shared_kernel::LoadError;
    use crate::updates::application::{Failure as FeedFailure, SourcePin};

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
    }

    impl SourceFetcher for Fetcher {
        fn fetch(&self, url: &str, name: &str, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64)) -> Result<PathBuf, Failure> {
            self.asked.lock().unwrap().push(format!("{url} {name}"));
            progress(10);
            while self.hold && !cancelled() {
                thread::sleep(Duration::from_millis(5));
            }
            if cancelled() {
                return Err(Failure::Cancelled);
            }
            Ok(PathBuf::from("/fictional/onboarding/1/release.zip.part"))
        }
        fn clear_leftovers(&self) {}
    }

    /// Folders by what they hold, and the hash the release must have.
    struct Folders {
        contents: Mutex<Contents>,
        cleared: Mutex<Vec<PathBuf>>,
        unpacked: Mutex<Vec<String>>,
    }

    impl Folders {
        fn holding(contents: Contents) -> Self {
            Folders { contents: Mutex::new(contents), cleared: Mutex::default(), unpacked: Mutex::default() }
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
        fn unpack(&self, _zip: &Path, sha256: &str, folder: &Path, _cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(Phase)) -> Result<(), Failure> {
            phase(Phase::Checking);
            if sha256 != HASH {
                return Err(Failure::Mismatch);
            }
            phase(Phase::Unpacking);
            self.unpacked.lock().unwrap().push(folder.display().to_string());
            Ok(())
        }
        fn clear_leftovers(&self, folder: &Path) {
            self.cleared.lock().unwrap().push(folder.to_owned());
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()
    }

    fn onboarding(store: Arc<Memory>, pins: Pins, fetcher: Arc<Fetcher>, folders: Arc<Folders>) -> Onboarding {
        Onboarding::new(store, Arc::new(pins), fetcher, folders, PathBuf::from("/Users/sam/Documents"), Arc::new(|| {}))
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
        for (pins, failure) in [(Pins(Ok(None)), Failure::NoPin), (Pins(Err(FeedFailure::Unreachable)), Failure::Offline)] {
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
    fn a_mismatch_fails_and_nothing_is_recorded_as_done() {
        let store = Arc::new(Memory::default());
        let pins = Pins(Ok(Some(SourcePin::parse("1.7.5", &HASH.replace('1', "2")).unwrap())));
        let folders = Arc::new(Folders::holding(Contents::Missing));
        let mut onboarding = onboarding(Arc::clone(&store), pins, Arc::default(), Arc::clone(&folders));
        onboarding.start_from_scratch();
        onboarding.save_here(today());
        settle(&mut onboarding);
        assert!(matches!(onboarding.stage(), Stage::Failed { failure: Failure::Mismatch, .. }));
        assert!(folders.unpacked.lock().unwrap().is_empty());
        assert!(!store.load().unwrap().unwrap().is_done(Step::Folder));
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
}
