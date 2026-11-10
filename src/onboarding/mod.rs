//! First-run setup: the progress record, the job search folder, and the ai-job-search release downloaded into it,
//! checked against the SHA-256 that Jobdesk's signed release feed pins before anything is unpacked.

pub mod application;
mod domain;
mod infrastructure;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::onboarding::application::Onboarding;
use crate::onboarding::infrastructure::{HttpsFetcher, ProgressFile, ZipUnpacker};
use crate::shared_kernel::https::HttpsTransport;

/// The folder in the app's data folder that holds each running Jobdesk's download of the release, `onboarding/<pid>/`.
fn downloads(data: &Path) -> PathBuf {
    data.join("onboarding")
}

/// First-run setup over the app's data folder `data`, where the record and the download live. `current` is the
/// version this copy runs, `documents` the user's Documents folder, and `repaint` wakes the window when the worker
/// has news. It removes what a setup that stopped midway left. Nothing here starts `claude` or writes a run log.
pub fn first_run(data: &Path, current: &str, documents: PathBuf, repaint: Arc<dyn Fn() + Send + Sync>) -> Onboarding {
    let fetcher = HttpsFetcher::new(Arc::new(HttpsTransport::new(current)), downloads(data));
    let unpacker = ZipUnpacker::new(downloads(data), |folder| crate::workspace::open(folder).is_some());
    Onboarding::new(Arc::new(ProgressFile::new(data)), crate::updates::source_pins(current), Arc::new(fetcher), Arc::new(unpacker), documents, repaint)
}
