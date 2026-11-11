//! Downloads a file over the one https client into this Jobdesk's own `<pid>` folder of a folder in the data folder.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::shared_kernel::https::{Fetch, HttpsFailure, Transport, read_failure};
use crate::shared_kernel::own_folder::{claim, clear_unclaimed, new_private_file};

/// Progress is reported at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Why a download stopped. Each caller says it in its own words, and no server text is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadFailure {
    /// No internet, a timeout, or a download cut short: a cut or a timeout reads as offline.
    Offline,
    /// The server answered with this status code.
    Refused(u16),
    /// More bytes arrived than the download may hold.
    TooLarge,
    /// A file could not be written on this computer.
    Disk,
    /// The caller cancelled it.
    Cancelled,
}

/// Downloads files over the one https client into this Jobdesk's own `<pid>` folder of `dir`.
pub struct HttpsFetcher {
    transport: Arc<dyn Transport>,
    /// A folder of the data folder, such as `<data>/onboarding`, whose `<pid>` folders say which Jobdesk still runs.
    dir: PathBuf,
}

impl HttpsFetcher {
    pub fn new(transport: Arc<dyn Transport>, dir: PathBuf) -> Self {
        Self { transport, dir }
    }

    /// Downloads `url` into a new part file named after `name`, in this Jobdesk's own folder of `dir`, and gives its
    /// path. It blocks, and stops soon after `cancelled` turns true. `progress` hears the bytes received at most every
    /// 100 ms. More than `limit` bytes is `TooLarge`. Whatever ends it short deletes the part file.
    pub fn fetch_at_most(
        &self,
        url: &str,
        name: &str,
        limit: u64,
        cancelled: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(u64),
    ) -> Result<PathBuf, DownloadFailure> {
        let own = claim(&self.dir).map_err(|_| DownloadFailure::Disk)?;
        let part = own.join(format!("{name}.part"));
        match self.download(url, &part, limit, cancelled, progress) {
            Ok(()) => Ok(part),
            Err(failure) => {
                let _ = fs::remove_file(&part);
                Err(failure)
            }
        }
    }

    /// Removes the part files a Jobdesk that no longer runs left. Links are removed, never followed.
    pub fn clear_leftovers(&self) {
        clear_unclaimed(&self.dir, &[]);
    }

    fn download(&self, url: &str, part: &Path, limit: u64, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64)) -> Result<(), DownloadFailure> {
        let mut file = new_private_file(part).map_err(|_| DownloadFailure::Disk)?;
        let mut body = self.transport.open(url, Fetch::Download).map_err(failure_of)?;
        let mut buffer = vec![0; 64 * 1024];
        let mut received = 0;
        let mut told = Instant::now();
        loop {
            if cancelled() {
                return Err(DownloadFailure::Cancelled);
            }
            let read = match body.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(failure_of(read_failure(&error))),
            };
            received += read as u64;
            if received > limit {
                return Err(DownloadFailure::TooLarge);
            }
            file.write_all(&buffer[..read]).map_err(|_| DownloadFailure::Disk)?;
            if told.elapsed() >= PROGRESS_EVERY {
                told = Instant::now();
                progress(received);
            }
        }
        file.sync_all().map_err(|_| DownloadFailure::Disk)?;
        progress(received);
        Ok(())
    }
}

/// No server text is kept: a cut or a timeout reads as offline.
fn failure_of(failure: HttpsFailure) -> DownloadFailure {
    match failure {
        HttpsFailure::TimedOut | HttpsFailure::Unreachable | HttpsFailure::Interrupted => DownloadFailure::Offline,
        HttpsFailure::Refused(code) => DownloadFailure::Refused(code),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::shared_kernel::https::Body;

    const URL: &str = "https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.7.5";
    const LIMIT: u64 = 1024;

    struct Serves(Result<Vec<u8>, HttpsFailure>);

    impl Transport for Serves {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure> {
            assert_eq!((url, fetch), (URL, Fetch::Download));
            self.0.clone().map(|bytes| Box::new(Cursor::new(bytes)) as Body)
        }
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut found = Vec::new();
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            if entry.file_type().unwrap().is_dir() {
                found.extend(files(&entry.path()));
            } else if entry.file_name() != ".lock" {
                found.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        found
    }

    #[test]
    fn the_file_lands_in_this_jobdesks_own_folder() {
        let data = tempfile::tempdir().unwrap();
        let fetcher = HttpsFetcher::new(Arc::new(Serves(Ok(b"zip".to_vec()))), data.path().join("onboarding"));
        let mut heard = Vec::new();
        let part = fetcher.fetch_at_most(URL, "ai-job-search-v1.7.5.zip", LIMIT, &|| false, &mut |received| heard.push(received)).unwrap();
        assert_eq!(part, data.path().join("onboarding").join(std::process::id().to_string()).join("ai-job-search-v1.7.5.zip.part"));
        assert_eq!(fs::read(&part).unwrap(), b"zip");
        assert_eq!(heard.last(), Some(&3));
    }

    #[test]
    fn a_failed_or_cancelled_download_leaves_no_part_file() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("onboarding");
        for (serves, cancelled, failure) in [
            (Serves(Err(HttpsFailure::Unreachable)), false, DownloadFailure::Offline),
            (Serves(Err(HttpsFailure::TimedOut)), false, DownloadFailure::Offline),
            (Serves(Err(HttpsFailure::Refused(404))), false, DownloadFailure::Refused(404)),
            (Serves(Ok(b"zip".to_vec())), true, DownloadFailure::Cancelled),
        ] {
            let fetcher = HttpsFetcher::new(Arc::new(serves), dir.clone());
            assert_eq!(fetcher.fetch_at_most(URL, "ai-job-search-v1.7.5.zip", LIMIT, &|| cancelled, &mut |_| {}), Err(failure));
            assert!(files(&dir).is_empty(), "{failure:?}");
        }
    }

    #[test]
    fn a_download_past_its_limit_stops_and_leaves_nothing() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("onboarding");
        let fetcher = HttpsFetcher::new(Arc::new(Serves(Ok(b"four".to_vec()))), dir.clone());
        assert_eq!(fetcher.fetch_at_most(URL, "bun.zip", 3, &|| false, &mut |_| {}), Err(DownloadFailure::TooLarge));
        assert!(files(&dir).is_empty());
        assert!(fetcher.fetch_at_most(URL, "bun.zip", 4, &|| false, &mut |_| {}).is_ok(), "the limit itself is allowed");
    }

    #[test]
    fn the_next_open_removes_part_files_no_jobdesk_holds() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("onboarding");
        let fetcher = HttpsFetcher::new(Arc::new(Serves(Ok(b"zip".to_vec()))), dir.clone());
        let own = fetcher.fetch_at_most(URL, "ai-job-search-v1.7.5.zip", LIMIT, &|| false, &mut |_| {}).unwrap();
        let crashed = dir.join("1").join("ai-job-search-v1.7.5.zip.part");
        fs::create_dir_all(crashed.parent().unwrap()).unwrap();
        fs::write(&crashed, "half").unwrap();
        fetcher.clear_leftovers();
        assert!(!crashed.exists(), "a crash midway is cleared");
        assert!(own.exists(), "this Jobdesk's own download stays");
    }
}
