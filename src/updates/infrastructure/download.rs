use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::updates::domain::{DownloadEnd, DownloadId, Failure, Platform, Release, asset_url, percent};
use crate::updates::infrastructure::http::{Fetch, Transport, read_failure};
use crate::updates::infrastructure::signature::{UpdateKey, signed_by};

/// The largest build Jobdesk downloads, whatever the feed says.
pub const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;
/// Progress is reported at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
/// A download that brings less than `SLOWEST` bytes in a whole `PACE_WINDOW` has stalled, even if each read answers in
/// time, so a server that drips a byte at a time cannot hold the download forever.
const PACE_WINDOW: Duration = Duration::from_secs(60);
const SLOWEST: u64 = 16 * 1024;

/// One download and what it owns: its number, its files, and the switch that stops it.
/// Its worker and a later `cancel` share it, so whichever comes second deletes what the download kept.
pub struct Job {
    pub id: DownloadId,
    pub release: Release,
    pub platform: Platform,
    /// `<data>/updates`.
    pub dir: PathBuf,
    cancelled: AtomicBool,
    /// The finished file, once its signature checked and it has its final name.
    kept: Mutex<Option<PathBuf>>,
}

impl Job {
    pub fn new(id: DownloadId, release: Release, platform: Platform, dir: PathBuf) -> Arc<Job> {
        Arc::new(Job { id, release, platform, dir, cancelled: AtomicBool::new(false), kept: Mutex::new(None) })
    }

    fn asset(&self) -> String {
        self.platform.asset_name(&self.release.version)
    }

    pub fn part_path(&self) -> PathBuf {
        self.dir.join(self.id.part_name(&self.asset()))
    }

    pub fn file_path(&self) -> PathBuf {
        self.dir.join(self.id.file_name(&self.asset()))
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Stops the download, or deletes its file when it already finished. A worker still running deletes its own part
    /// file when it sees the switch, so this never races it for a path, and it touches no other download's files.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(file) = self.kept.lock().unwrap_or_else(PoisonError::into_inner).take() {
            let _ = fs::remove_file(file);
        }
    }

    /// Downloads the build from the address built from its version, checks its signature, and keeps it under its
    /// final name. `progress` hears `(done, total)` at most every 100 ms. Whatever ends it short, its part file goes.
    pub fn run(&self, transport: &dyn Transport, key: Option<&UpdateKey>, progress: &mut dyn FnMut(u64, u64)) -> DownloadEnd {
        let part = self.part_path();
        let end = self.fetch(&part, transport, key, progress);
        if !matches!(end, DownloadEnd::Done(_)) {
            let _ = fs::remove_file(&part);
        }
        end
    }

    fn fetch(&self, part: &Path, transport: &dyn Transport, key: Option<&UpdateKey>, progress: &mut dyn FnMut(u64, u64)) -> DownloadEnd {
        let size = self.release.size;
        let failed = |done: u64, failure: Failure| DownloadEnd::Failed { percent: percent(done, size), failure };
        if size == 0 || size > MAX_DOWNLOAD {
            return failed(0, Failure::TooLarge);
        }
        // A build that cannot be checked is never kept.
        let Some(key) = key else { return DownloadEnd::BadSignature };
        let Ok(mut file) = private_dir(&self.dir).and_then(|()| new_private_file(part)) else { return failed(0, Failure::Disk) };
        let mut body = match transport.open(&asset_url(&self.release.version, self.platform), Fetch::Download) {
            Ok(body) => body,
            Err(failure) => return failed(0, failure),
        };
        let mut buffer = vec![0; 64 * 1024];
        let mut done = 0;
        let mut told = Instant::now();
        let mut pace = Pace::new(told);
        loop {
            if self.is_cancelled() {
                return DownloadEnd::Cancelled;
            }
            let read = match body.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return failed(done, read_failure(&error)),
            };
            if done + read as u64 > size {
                return failed(done, Failure::TooLarge);
            }
            if file.write_all(&buffer[..read]).is_err() {
                return failed(done, Failure::Disk);
            }
            done += read as u64;
            if pace.stalled(Instant::now(), read as u64) {
                return failed(done, Failure::TimedOut);
            }
            if told.elapsed() >= PROGRESS_EVERY {
                told = Instant::now();
                progress(done, size);
            }
        }
        if done < size {
            return failed(done, Failure::Interrupted);
        }
        if file.sync_all().is_err() {
            return failed(done, Failure::Disk);
        }
        drop(file);
        match signed_by(key, part, &self.release.signature, &self.asset()) {
            Ok(true) => {}
            Ok(false) => return DownloadEnd::BadSignature,
            Err(_) => return failed(done, Failure::Disk),
        }
        self.keep(part)
    }

    /// Gives the checked file its final name, unless a cancel came first. Holding `kept` while it decides means a
    /// cancel either stops it here or finds the file and deletes it.
    fn keep(&self, part: &Path) -> DownloadEnd {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        if self.is_cancelled() {
            return DownloadEnd::Cancelled;
        }
        let file = self.file_path();
        let _ = fs::remove_file(&file);
        if fs::rename(part, &file).is_err() {
            return DownloadEnd::Failed { percent: 100, failure: Failure::Disk };
        }
        *kept = Some(file.clone());
        DownloadEnd::Done(file)
    }
}

/// How much a download brought in its current window of `PACE_WINDOW`.
struct Pace {
    since: Instant,
    bytes: u64,
}

impl Pace {
    fn new(now: Instant) -> Self {
        Self { since: now, bytes: 0 }
    }

    /// Counts `read` bytes at `now`, and says whether the window that just closed brought too little.
    fn stalled(&mut self, now: Instant, read: u64) -> bool {
        self.bytes += read;
        if now.duration_since(self.since) < PACE_WINDOW {
            return false;
        }
        let stalled = self.bytes < SLOWEST;
        *self = Pace::new(now);
        stalled
    }
}

/// The whole of a downloaded build, read once, so the bytes that are checked are the bytes that are applied.
/// A file larger than any download may be is an error.
pub fn read_build(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(MAX_DOWNLOAD + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_DOWNLOAD {
        return Err(io::Error::other("larger than any download"));
    }
    Ok(bytes)
}

/// `<data>/updates`, readable only by the user, as the run logs are.
pub fn private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

/// A new file only the user can read. Whatever had the name before goes first, and a link there is removed, not followed.
pub fn new_private_file(path: &Path) -> io::Result<File> {
    let _ = fs::remove_file(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

#[cfg(test)]
pub mod tests {
    use std::io::Cursor;
    use std::sync::mpsc::{self, Receiver};

    use super::*;
    use crate::updates::domain::Version;
    use crate::updates::infrastructure::http::Body;
    use crate::updates::infrastructure::signature::tests::{fixture, fixture_key, fixture_signature};

    pub const WINDOWS: &str = "Jobdesk-v0.2.0-windows-x64-setup.exe";

    pub fn release(size: u64) -> Release {
        Release { version: Version { major: 0, minor: 2, patch: 0 }, released: None, notes: String::new(), size, signature: fixture_signature(WINDOWS) }
    }

    pub fn windows_build() -> Vec<u8> {
        fs::read(fixture(WINDOWS)).unwrap()
    }

    /// Serves `bytes` for the Windows build's address and checks that is the address asked for.
    pub struct Serves(pub Vec<u8>);

    impl Transport for Serves {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, Failure> {
            assert_eq!(url, "https://github.com/Nicolas-Widart-SRL/jobdesk/releases/download/v0.2.0/Jobdesk-v0.2.0-windows-x64-setup.exe");
            assert_eq!(fetch, Fetch::Download);
            Ok(Box::new(Cursor::new(self.0.clone())))
        }
    }

    /// Serves its first bytes, then waits for a word before it serves the rest, so a test can act mid-download.
    pub struct Pauses {
        pub first: Vec<u8>,
        pub rest: Vec<u8>,
        pub go: Mutex<Option<Receiver<()>>>,
    }

    struct PausedBody {
        first: Cursor<Vec<u8>>,
        rest: Cursor<Vec<u8>>,
        go: Option<Receiver<()>>,
    }

    impl Read for PausedBody {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let read = self.first.read(buf)?;
            if read > 0 {
                return Ok(read);
            }
            if let Some(go) = self.go.take() {
                let _ = go.recv();
            }
            self.rest.read(buf)
        }
    }

    impl Transport for Pauses {
        fn open(&self, _url: &str, _fetch: Fetch) -> Result<Body, Failure> {
            let go = self.go.lock().unwrap().take();
            Ok(Box::new(PausedBody { first: Cursor::new(self.first.clone()), rest: Cursor::new(self.rest.clone()), go }))
        }
    }

    fn job(dir: &Path, size: u64, id: u64) -> Arc<Job> {
        Job::new(DownloadId(id), release(size), Platform::WindowsX64, dir.to_owned())
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            fs::read_dir(dir).map(|entries| entries.flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn a_signed_build_is_kept_under_its_own_name() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("updates");
        let bytes = windows_build();
        let job = job(&dir, bytes.len() as u64, 3);
        let end = job.run(&Serves(bytes.clone()), Some(&fixture_key()), &mut |_, _| {});
        let file = dir.join("3-Jobdesk-v0.2.0-windows-x64-setup.exe");
        assert_eq!(end, DownloadEnd::Done(file.clone()));
        assert_eq!(fs::read(&file).unwrap(), bytes);
        assert_eq!(names(&dir), ["3-Jobdesk-v0.2.0-windows-x64-setup.exe"]);
    }

    #[cfg(unix)]
    #[test]
    fn the_downloads_are_readable_only_by_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("updates");
        let bytes = windows_build();
        let DownloadEnd::Done(file) = job(&dir, bytes.len() as u64, 1).run(&Serves(bytes), Some(&fixture_key()), &mut |_, _| {}) else { panic!() };
        assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(file).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_flipped_byte_is_a_bad_signature_and_nothing_is_kept() {
        let data = tempfile::tempdir().unwrap();
        let mut bytes = windows_build();
        bytes[5] ^= 1;
        let job = job(data.path(), bytes.len() as u64, 1);
        assert_eq!(job.run(&Serves(bytes), Some(&fixture_key()), &mut |_, _| {}), DownloadEnd::BadSignature);
        assert_eq!(names(data.path()), Vec::<String>::new(), "the file and its .part are deleted");
    }

    #[test]
    fn with_no_key_nothing_is_downloaded() {
        let data = tempfile::tempdir().unwrap();
        let bytes = windows_build();
        assert_eq!(job(data.path(), bytes.len() as u64, 1).run(&Serves(bytes), None, &mut |_, _| {}), DownloadEnd::BadSignature);
        assert_eq!(names(data.path()), Vec::<String>::new());
    }

    #[test]
    fn a_build_larger_than_the_feed_said_is_too_large() {
        let data = tempfile::tempdir().unwrap();
        let bytes = windows_build();
        let end = job(data.path(), bytes.len() as u64 - 1, 1).run(&Serves(bytes), Some(&fixture_key()), &mut |_, _| {});
        assert!(matches!(end, DownloadEnd::Failed { failure: Failure::TooLarge, .. }), "{end:?}");
        assert_eq!(names(data.path()), Vec::<String>::new());
    }

    #[test]
    fn a_build_shorter_than_the_feed_said_is_interrupted() {
        let data = tempfile::tempdir().unwrap();
        let bytes = windows_build();
        let size = bytes.len() as u64 * 2;
        let end = job(data.path(), size, 1).run(&Serves(bytes), Some(&fixture_key()), &mut |_, _| {});
        assert_eq!(end, DownloadEnd::Failed { percent: 50, failure: Failure::Interrupted });
        assert_eq!(names(data.path()), Vec::<String>::new());
    }

    #[test]
    fn a_feed_size_over_the_limit_is_refused_before_any_request() {
        struct Never;
        impl Transport for Never {
            fn open(&self, _url: &str, _fetch: Fetch) -> Result<Body, Failure> {
                panic!("nothing is requested")
            }
        }
        let data = tempfile::tempdir().unwrap();
        for size in [0, MAX_DOWNLOAD + 1] {
            assert_eq!(
                job(data.path(), size, 1).run(&Never, Some(&fixture_key()), &mut |_, _| {}),
                DownloadEnd::Failed { percent: 0, failure: Failure::TooLarge }
            );
        }
    }

    #[test]
    fn an_unreachable_build_says_why() {
        struct Missing;
        impl Transport for Missing {
            fn open(&self, _url: &str, _fetch: Fetch) -> Result<Body, Failure> {
                Err(Failure::Refused(404))
            }
        }
        let data = tempfile::tempdir().unwrap();
        assert_eq!(
            job(data.path(), 10, 1).run(&Missing, Some(&fixture_key()), &mut |_, _| {}),
            DownloadEnd::Failed { percent: 0, failure: Failure::Refused(404) }
        );
        assert_eq!(names(data.path()), Vec::<String>::new());
    }

    #[test]
    fn a_download_that_drips_in_stalls() {
        let start = Instant::now();
        let mut pace = Pace::new(start);
        assert!(!pace.stalled(start + Duration::from_secs(30), 10));
        assert!(pace.stalled(start + PACE_WINDOW, 10), "20 bytes in a minute is a stall");
        let next = start + PACE_WINDOW;
        assert!(!pace.stalled(next + Duration::from_secs(1), SLOWEST));
        assert!(!pace.stalled(next + PACE_WINDOW, 0), "a window that brought enough is no stall");
    }

    #[test]
    fn a_build_is_read_whole_or_not_at_all() {
        let data = tempfile::tempdir().unwrap();
        assert!(read_build(&data.path().join("missing")).is_err());
        assert_eq!(read_build(&fixture(WINDOWS)).unwrap(), windows_build());
    }

    #[test]
    fn cancel_mid_download_removes_the_part_file() {
        let data = tempfile::tempdir().unwrap();
        let bytes = windows_build();
        let (go, wait) = mpsc::channel();
        let pauses = Pauses { first: bytes[..10].to_vec(), rest: bytes[10..].to_vec(), go: Mutex::new(Some(wait)) };
        let job = job(data.path(), bytes.len() as u64, 7);
        let worker = {
            let job = Arc::clone(&job);
            std::thread::spawn(move || job.run(&pauses, Some(&fixture_key()), &mut |_, _| {}))
        };
        while !job.part_path().exists() {
            std::thread::yield_now();
        }
        job.cancel();
        // The worker may have seen the cancel before it read again, and then it no longer listens.
        let _ = go.send(());
        assert_eq!(worker.join().unwrap(), DownloadEnd::Cancelled);
        assert_eq!(names(data.path()), Vec::<String>::new());
    }

    #[test]
    fn cancel_after_the_end_deletes_only_that_download() {
        let data = tempfile::tempdir().unwrap();
        let bytes = windows_build();
        let first = job(data.path(), bytes.len() as u64, 1);
        let second = job(data.path(), bytes.len() as u64, 2);
        first.run(&Serves(bytes.clone()), Some(&fixture_key()), &mut |_, _| {});
        second.run(&Serves(bytes), Some(&fixture_key()), &mut |_, _| {});
        first.cancel();
        assert_eq!(names(data.path()), ["2-Jobdesk-v0.2.0-windows-x64-setup.exe"]);
        first.cancel();
        assert_eq!(names(data.path()), ["2-Jobdesk-v0.2.0-windows-x64-setup.exe"], "a second cancel finds nothing more to delete");
    }

    #[test]
    fn a_cancelled_job_never_keeps_its_file() {
        let data = tempfile::tempdir().unwrap();
        let bytes = windows_build();
        let job = job(data.path(), bytes.len() as u64, 1);
        job.cancel();
        assert_eq!(job.run(&Serves(bytes), Some(&fixture_key()), &mut |_, _| {}), DownloadEnd::Cancelled);
        assert_eq!(names(data.path()), Vec::<String>::new());
    }

    #[test]
    fn a_link_where_the_part_file_goes_is_replaced_not_followed() {
        let data = tempfile::tempdir().unwrap();
        let outside = data.path().join("outside");
        fs::write(&outside, "kept").unwrap();
        let dir = data.path().join("updates");
        fs::create_dir(&dir).unwrap();
        let bytes = windows_build();
        let job = job(&dir, bytes.len() as u64, 1);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, job.part_path()).unwrap();
        assert!(matches!(job.run(&Serves(bytes), Some(&fixture_key()), &mut |_, _| {}), DownloadEnd::Done(_)));
        assert_eq!(fs::read_to_string(&outside).unwrap(), "kept");
    }
}
