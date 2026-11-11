use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::shared_kernel::https::Transport;
use crate::updates::domain::{DownloadEnd, DownloadId, Failure, Platform, Release, ServiceEvent, SourcePin, SourcePinReader, UpdateService};
use crate::updates::infrastructure::cleanup::claim;
use crate::updates::infrastructure::download::Job;
use crate::updates::infrastructure::feed::{read_feed, read_pin};
use crate::updates::infrastructure::signature::UpdateKey;

/// What every worker needs, shared with each thread it starts.
struct Shared {
    transport: Arc<dyn Transport>,
    /// `<data>/updates`.
    dir: PathBuf,
    platform: Option<Platform>,
    key: Option<UpdateKey>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

/// The feed and the builds on GitHub. Each check and each download runs on a thread of its own, and its answers come
/// back through a channel, with a repaint so the window shows them. It starts no `claude` process, uses no API key and
/// writes no run log, so neither Tasks nor Usage ever sees it.
pub struct GithubUpdates {
    shared: Arc<Shared>,
    sender: Sender<ServiceEvent>,
    events: Receiver<ServiceEvent>,
    /// The latest download, running or kept, until a cancel drops it.
    download: Option<Arc<Job>>,
    /// This process's own folder in `updates/`, claimed at the first download and locked until the process exits.
    own: Option<PathBuf>,
}

impl GithubUpdates {
    pub fn new(transport: Arc<dyn Transport>, dir: PathBuf, platform: Option<Platform>, key: Option<UpdateKey>, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, events) = mpsc::channel();
        Self { shared: Arc::new(Shared { transport, dir, platform, key, repaint }), sender, events, download: None, own: None }
    }
}

impl UpdateService for GithubUpdates {
    fn check(&mut self) {
        let shared = Arc::clone(&self.shared);
        let sender = self.sender.clone();
        thread::spawn(move || {
            let _ = sender.send(ServiceEvent::Checked(read_feed(shared.transport.as_ref(), shared.platform, shared.key.as_ref())));
            (shared.repaint)();
        });
    }

    fn download(&mut self, release: &Release, id: DownloadId) {
        // A new download replaces the one before, which deletes only its own files.
        self.cancel();
        let fail = |failure| {
            let _ = self.sender.send(ServiceEvent::Ended { id, end: DownloadEnd::Failed { percent: 0, failure } });
            (self.shared.repaint)();
        };
        let Some(platform) = self.shared.platform else { return fail(Failure::NoBuild) };
        if self.own.is_none() {
            self.own = claim(&self.shared.dir).ok();
        }
        let Some(own) = &self.own else { return fail(Failure::Disk) };
        let job = Job::new(id, release.clone(), platform, own.clone());
        self.download = Some(Arc::clone(&job));
        let shared = Arc::clone(&self.shared);
        let sender = self.sender.clone();
        thread::spawn(move || {
            let mut progress = |done, total| {
                let _ = sender.send(ServiceEvent::Progress { id, done, total });
                (shared.repaint)();
            };
            let end = job.run(shared.transport.as_ref(), shared.key.as_ref(), &mut progress);
            let _ = sender.send(ServiceEvent::Ended { id, end });
            (shared.repaint)();
        });
    }

    fn cancel(&mut self) {
        if let Some(job) = self.download.take() {
            job.cancel();
        }
    }

    fn poll(&mut self) -> Option<ServiceEvent> {
        self.events.try_recv().ok()
    }
}

/// The `ai-job-search` release the feed on GitHub pins, checked against the update key. Like the update check, it
/// starts no `claude` process, uses no API key and writes no run log.
pub struct GithubSourcePins {
    transport: Arc<dyn Transport>,
    key: Option<UpdateKey>,
}

impl GithubSourcePins {
    /// With no `key`, no pin can be checked, so none is ever read.
    pub fn new(transport: Arc<dyn Transport>, key: Option<UpdateKey>) -> Self {
        Self { transport, key }
    }
}

impl SourcePinReader for GithubSourcePins {
    fn read(&self) -> Result<Option<SourcePin>, Failure> {
        let Some(key) = &self.key else { return Ok(None) };
        read_pin(self.transport.as_ref(), key)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Sender};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::shared_kernel::https::{Body, Fetch, HttpsFailure};
    use crate::updates::domain::{FEED_URL, SignedPin, Version};
    use crate::updates::infrastructure::download::tests::{Serves, WINDOWS, release, windows_build};
    use crate::updates::infrastructure::feed::tests::{fixture_pin, fixture_pins};
    use crate::updates::infrastructure::signature::tests::{fixture, fixture_key};

    /// Holds every request until the test lets it through.
    struct Blocks {
        go: Mutex<Receiver<()>>,
        feed: Vec<u8>,
    }

    impl Transport for Blocks {
        fn open(&self, url: &str, _fetch: Fetch) -> Result<Body, HttpsFailure> {
            assert_eq!(url, FEED_URL);
            let _ = self.go.lock().unwrap().recv();
            Ok(Box::new(Cursor::new(self.feed.clone())))
        }
    }

    /// Says when the first request arrives and holds it until the test lets it through. Every later one is served at once.
    struct FirstWaits {
        calls: AtomicUsize,
        arrived: Mutex<Sender<()>>,
        go: Mutex<Receiver<()>>,
        serves: Serves,
    }

    impl Transport for FirstWaits {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.arrived.lock().unwrap().send(()).unwrap();
                let _ = self.go.lock().unwrap().recv();
            }
            self.serves.open(url, fetch)
        }
    }

    /// The folder this process downloads into.
    fn own(dir: &std::path::Path) -> PathBuf {
        dir.join(std::process::id().to_string())
    }

    fn service(transport: Arc<dyn Transport>, dir: PathBuf, repaints: Arc<AtomicUsize>) -> GithubUpdates {
        let repaint = Arc::new(move || {
            repaints.fetch_add(1, Ordering::SeqCst);
        });
        GithubUpdates::new(transport, dir, Some(Platform::WindowsX64), Some(fixture_key()), repaint)
    }

    /// Polls until the end of a download or the answer to a check, so a test never sleeps a fixed time.
    fn wait(service: &mut GithubUpdates) -> Vec<ServiceEvent> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            match service.poll() {
                Some(event) => {
                    let last = matches!(event, ServiceEvent::Checked(_) | ServiceEvent::Ended { .. });
                    seen.push(event);
                    if last {
                        return seen;
                    }
                }
                None => thread::yield_now(),
            }
        }
        panic!("no answer: {seen:?}");
    }

    #[test]
    fn check_returns_at_once_while_github_is_slow() {
        let (go, wait_for) = mpsc::channel();
        let feed = fs::read(fixture("latest.json")).unwrap();
        let repaints = Arc::new(AtomicUsize::new(0));
        let data = tempfile::tempdir().unwrap();
        let mut service = service(Arc::new(Blocks { go: Mutex::new(wait_for), feed }), data.path().join("updates"), Arc::clone(&repaints));
        let started = Instant::now();
        service.check();
        assert!(started.elapsed() < Duration::from_millis(50), "check took {:?}", started.elapsed());
        assert_eq!(service.poll(), None);
        go.send(()).unwrap();
        let answers: [ServiceEvent; 1] = wait(&mut service).try_into().unwrap();
        let [ServiceEvent::Checked(Ok(found))] = answers else { panic!("{answers:?}") };
        assert_eq!(found.version, Version { major: 0, minor: 2, patch: 0 });
        let pins: Vec<&SourcePin> = found.pins.iter().map(SignedPin::pin).collect();
        assert_eq!(pins, fixture_pins().iter().collect::<Vec<_>>(), "the check carries the pins it read, checked with the update key, newest first");
        // The worker repaints after it sends, so wait for that call.
        let deadline = Instant::now() + Duration::from_secs(10);
        while repaints.load(Ordering::SeqCst) < 1 && Instant::now() < deadline {
            thread::yield_now();
        }
        assert_eq!(repaints.load(Ordering::SeqCst), 1);
        assert!(!data.path().join("updates").exists(), "a check writes nothing");
    }

    #[test]
    fn a_download_ends_with_its_own_number_and_file() {
        let data = tempfile::tempdir().unwrap();
        let bytes = windows_build();
        let mut service = service(Arc::new(Serves(bytes.clone())), data.path().join("updates"), Arc::default());
        service.download(&release(bytes.len() as u64), DownloadId(4));
        let events = wait(&mut service);
        let file = own(&data.path().join("updates")).join("4-Jobdesk-v0.2.0-windows-x64-setup.exe");
        assert_eq!(events.last(), Some(&ServiceEvent::Ended { id: DownloadId(4), end: DownloadEnd::Done(file.clone()) }));
        assert!(events.iter().all(|event| matches!(event, ServiceEvent::Progress { id: DownloadId(4), .. } | ServiceEvent::Ended { .. })));
        assert_eq!(fs::read(file).unwrap(), fs::read(fixture(WINDOWS)).unwrap());
    }

    #[test]
    fn cancel_drops_a_kept_download() {
        let data = tempfile::tempdir().unwrap();
        let bytes = windows_build();
        let mut service = service(Arc::new(Serves(bytes.clone())), data.path().join("updates"), Arc::default());
        service.download(&release(bytes.len() as u64), DownloadId(1));
        let Some(ServiceEvent::Ended { end: DownloadEnd::Done(file), .. }) = wait(&mut service).pop() else { panic!() };
        service.cancel();
        assert!(!file.exists());
    }

    #[test]
    fn a_cancelled_download_winding_down_never_touches_the_next_one() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("updates");
        let bytes = windows_build();
        let (go, wait_for) = mpsc::channel();
        let (arrived, first_arrived) = mpsc::channel();
        let transport =
            Arc::new(FirstWaits { calls: AtomicUsize::new(0), arrived: Mutex::new(arrived), go: Mutex::new(wait_for), serves: Serves(bytes.clone()) });
        let mut service = service(transport, dir.clone(), Arc::default());
        service.download(&release(bytes.len() as u64), DownloadId(1));
        first_arrived.recv().unwrap();
        let first_part = own(&dir).join("1-Jobdesk-v0.2.0-windows-x64-setup.exe.part");
        assert!(first_part.exists());
        service.cancel();
        // The next download starts while the first still waits on the network, and finishes first.
        service.download(&release(bytes.len() as u64), DownloadId(2));
        let events = wait(&mut service);
        let second = own(&dir).join("2-Jobdesk-v0.2.0-windows-x64-setup.exe");
        assert_eq!(events.last(), Some(&ServiceEvent::Ended { id: DownloadId(2), end: DownloadEnd::Done(second.clone()) }));
        go.send(()).unwrap();
        let late = wait(&mut service);
        assert_eq!(late.last(), Some(&ServiceEvent::Ended { id: DownloadId(1), end: DownloadEnd::Cancelled }));
        assert_eq!(fs::read(&second).unwrap(), bytes, "the late end of the first leaves the second whole");
        assert!(!first_part.exists());
    }

    #[test]
    fn a_computer_with_no_build_downloads_nothing() {
        let data = tempfile::tempdir().unwrap();
        let mut service = GithubUpdates::new(Arc::new(Serves(Vec::new())), data.path().join("updates"), None, Some(fixture_key()), Arc::new(|| {}));
        service.download(&release(10), DownloadId(1));
        assert_eq!(wait(&mut service), [ServiceEvent::Ended { id: DownloadId(1), end: DownloadEnd::Failed { percent: 0, failure: Failure::NoBuild } }]);
        assert!(!data.path().join("updates").exists());
    }

    /// Serves the fixture feed, as GitHub would.
    struct Feed;

    impl Transport for Feed {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure> {
            assert_eq!((url, fetch), (FEED_URL, Fetch::Feed));
            Ok(Box::new(Cursor::new(fs::read(fixture("latest.json")).unwrap())))
        }
    }

    #[test]
    fn the_pin_reads_from_the_feed_with_the_update_key() {
        assert_eq!(GithubSourcePins::new(Arc::new(Feed), Some(fixture_key())).read(), Ok(Some(fixture_pin())));
    }

    #[test]
    fn a_check_with_no_update_key_carries_no_pins() {
        let data = tempfile::tempdir().unwrap();
        let mut service = GithubUpdates::new(Arc::new(Feed), data.path().join("updates"), Some(Platform::WindowsX64), None, Arc::new(|| {}));
        service.check();
        let answers: [ServiceEvent; 1] = wait(&mut service).try_into().unwrap();
        let [ServiceEvent::Checked(Ok(found))] = answers else { panic!("{answers:?}") };
        assert_eq!((found.version, found.pins), (Version { major: 0, minor: 2, patch: 0 }, Vec::new()));
    }

    #[test]
    fn a_build_with_no_update_key_has_no_pin_and_reads_nothing() {
        struct Never;
        impl Transport for Never {
            fn open(&self, _url: &str, _fetch: Fetch) -> Result<Body, HttpsFailure> {
                panic!("nothing is requested")
            }
        }
        assert_eq!(GithubSourcePins::new(Arc::new(Never), None).read(), Ok(None));
    }
}
