use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::onboarding::domain::{Failure, MAX_ZIP, SourceFetcher};
use crate::shared_kernel::https::{Fetch, HttpsFailure, Transport, read_failure};
use crate::shared_kernel::own_folder::{claim, clear_unclaimed, new_private_file};

/// Progress is reported at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Downloads the release zip, and the helpers' files, over the one https client into `onboarding/<pid>/` of the data
/// folder.
pub struct HttpsFetcher {
    transport: Arc<dyn Transport>,
    /// `<data>/onboarding`.
    dir: PathBuf,
}

impl HttpsFetcher {
    pub fn new(transport: Arc<dyn Transport>, dir: PathBuf) -> Self {
        Self { transport, dir }
    }

    /// Downloads `url` as `fetch` does, but fails with `TooLarge` once more than `limit` bytes arrive.
    pub fn fetch_at_most(&self, url: &str, name: &str, limit: u64, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64)) -> Result<PathBuf, Failure> {
        let own = claim(&self.dir).map_err(|_| Failure::Disk)?;
        let part = own.join(format!("{name}.part"));
        match self.download(url, &part, limit, cancelled, progress) {
            Ok(()) => Ok(part),
            Err(failure) => {
                let _ = fs::remove_file(&part);
                Err(failure)
            }
        }
    }

    fn download(&self, url: &str, part: &Path, limit: u64, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64)) -> Result<(), Failure> {
        let mut file = new_private_file(part).map_err(|_| Failure::Disk)?;
        let mut body = self.transport.open(url, Fetch::Download).map_err(failure_of)?;
        let mut buffer = vec![0; 64 * 1024];
        let mut received = 0;
        let mut told = Instant::now();
        loop {
            if cancelled() {
                return Err(Failure::Cancelled);
            }
            let read = match body.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(failure_of(read_failure(&error))),
            };
            received += read as u64;
            if received > limit {
                return Err(Failure::TooLarge);
            }
            file.write_all(&buffer[..read]).map_err(|_| Failure::Disk)?;
            if told.elapsed() >= PROGRESS_EVERY {
                told = Instant::now();
                progress(received);
            }
        }
        file.sync_all().map_err(|_| Failure::Disk)?;
        progress(received);
        Ok(())
    }
}

impl SourceFetcher for HttpsFetcher {
    fn fetch(&self, url: &str, name: &str, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64)) -> Result<PathBuf, Failure> {
        self.fetch_at_most(url, name, MAX_ZIP, cancelled, progress)
    }

    fn clear_leftovers(&self) {
        clear_unclaimed(&self.dir, &[]);
    }
}

/// No server text is kept: a cut or a timeout reads as offline, as the screen says it.
fn failure_of(failure: HttpsFailure) -> Failure {
    match failure {
        HttpsFailure::TimedOut | HttpsFailure::Unreachable | HttpsFailure::Interrupted => Failure::Offline,
        HttpsFailure::Refused(code) => Failure::Refused(code),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::shared_kernel::https::Body;

    const URL: &str = "https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.7.5";

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
    fn the_zip_lands_in_this_jobdesks_own_folder() {
        let data = tempfile::tempdir().unwrap();
        let fetcher = HttpsFetcher::new(Arc::new(Serves(Ok(b"zip".to_vec()))), data.path().join("onboarding"));
        let mut heard = Vec::new();
        let part = fetcher.fetch(URL, "ai-job-search-v1.7.5.zip", &|| false, &mut |received| heard.push(received)).unwrap();
        assert_eq!(part, data.path().join("onboarding").join(std::process::id().to_string()).join("ai-job-search-v1.7.5.zip.part"));
        assert_eq!(fs::read(&part).unwrap(), b"zip");
        assert_eq!(heard.last(), Some(&3));
    }

    #[test]
    fn a_failed_or_cancelled_download_leaves_no_part_file() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("onboarding");
        for (serves, cancelled, failure) in [
            (Serves(Err(HttpsFailure::Unreachable)), false, Failure::Offline),
            (Serves(Err(HttpsFailure::Refused(404))), false, Failure::Refused(404)),
            (Serves(Ok(b"zip".to_vec())), true, Failure::Cancelled),
        ] {
            let fetcher = HttpsFetcher::new(Arc::new(serves), dir.clone());
            assert_eq!(fetcher.fetch(URL, "ai-job-search-v1.7.5.zip", &|| cancelled, &mut |_| {}), Err(failure));
            assert!(files(&dir).is_empty(), "{failure:?}");
        }
    }

    #[test]
    fn a_download_past_its_limit_stops_and_leaves_nothing() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("onboarding");
        let fetcher = HttpsFetcher::new(Arc::new(Serves(Ok(b"four".to_vec()))), dir.clone());
        assert_eq!(fetcher.fetch_at_most(URL, "bun.zip", 3, &|| false, &mut |_| {}), Err(Failure::TooLarge));
        assert!(files(&dir).is_empty());
        assert!(fetcher.fetch_at_most(URL, "bun.zip", 4, &|| false, &mut |_| {}).is_ok(), "the limit itself is allowed");
    }

    #[test]
    fn the_next_open_removes_part_files_no_jobdesk_holds() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("onboarding");
        let fetcher = HttpsFetcher::new(Arc::new(Serves(Ok(b"zip".to_vec()))), dir.clone());
        let own = fetcher.fetch(URL, "ai-job-search-v1.7.5.zip", &|| false, &mut |_| {}).unwrap();
        let crashed = dir.join("1").join("ai-job-search-v1.7.5.zip.part");
        fs::create_dir_all(crashed.parent().unwrap()).unwrap();
        fs::write(&crashed, "half").unwrap();
        fetcher.clear_leftovers();
        assert!(!crashed.exists(), "a crash midway is cleared");
        assert!(own.exists(), "this Jobdesk's own download stays");
    }
}
