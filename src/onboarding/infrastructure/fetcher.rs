//! The release zip, and the helpers' files, download over the shared `HttpsFetcher` into `onboarding/<pid>/` of the data
//! folder.

use std::path::PathBuf;

use crate::onboarding::domain::{Failure, MAX_ZIP, SourceFetcher};
use crate::shared_kernel::download::{DownloadFailure, HttpsFetcher};

impl SourceFetcher for HttpsFetcher {
    fn fetch(&self, url: &str, name: &str, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64)) -> Result<PathBuf, Failure> {
        Ok(self.fetch_at_most(url, name, MAX_ZIP, cancelled, progress)?)
    }

    fn clear_leftovers(&self) {
        HttpsFetcher::clear_leftovers(self);
    }
}

/// Each download failure in setup's words: a cut or a timeout reads as offline, as the screen says it.
impl From<DownloadFailure> for Failure {
    fn from(failure: DownloadFailure) -> Self {
        match failure {
            DownloadFailure::Offline => Failure::Offline,
            DownloadFailure::Refused(code) => Failure::Refused(code),
            DownloadFailure::TooLarge => Failure::TooLarge,
            DownloadFailure::Disk => Failure::Disk,
            DownloadFailure::Cancelled => Failure::Cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;
    use std::sync::Arc;

    use super::*;
    use crate::shared_kernel::https::{Body, Fetch, HttpsFailure, Transport};

    const URL: &str = "https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.7.5";

    struct Serves(Result<Vec<u8>, HttpsFailure>);

    impl Transport for Serves {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure> {
            assert_eq!((url, fetch), (URL, Fetch::Download));
            self.0.clone().map(|bytes| Box::new(Cursor::new(bytes)) as Body)
        }
    }

    #[test]
    fn the_zip_lands_in_this_jobdesks_own_folder() {
        let data = tempfile::tempdir().unwrap();
        let fetcher = HttpsFetcher::new(Arc::new(Serves(Ok(b"zip".to_vec()))), data.path().join("onboarding"));
        let part = fetcher.fetch(URL, "ai-job-search-v1.7.5.zip", &|| false, &mut |_| {}).unwrap();
        assert_eq!(part, data.path().join("onboarding").join(std::process::id().to_string()).join("ai-job-search-v1.7.5.zip.part"));
        assert_eq!(fs::read(&part).unwrap(), b"zip");
    }

    #[test]
    fn each_download_failure_reads_in_setups_words() {
        let data = tempfile::tempdir().unwrap();
        for (serves, cancelled, failure) in [
            (Serves(Err(HttpsFailure::Unreachable)), false, Failure::Offline),
            (Serves(Err(HttpsFailure::Interrupted)), false, Failure::Offline),
            (Serves(Err(HttpsFailure::Refused(404))), false, Failure::Refused(404)),
            (Serves(Ok(b"zip".to_vec())), true, Failure::Cancelled),
        ] {
            let fetcher = HttpsFetcher::new(Arc::new(serves), data.path().join("onboarding"));
            assert_eq!(fetcher.fetch(URL, "ai-job-search-v1.7.5.zip", &|| cancelled, &mut |_| {}), Err(failure));
        }
        assert_eq!(Failure::from(DownloadFailure::TooLarge), Failure::TooLarge);
        assert_eq!(Failure::from(DownloadFailure::Disk), Failure::Disk);
    }
}
