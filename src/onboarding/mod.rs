//! First-run setup: the progress record, the job search folder, and the ai-job-search release downloaded into it,
//! checked against the SHA-256 that Jobdesk's signed release feed pins before anything is unpacked. Then, on macOS,
//! the helpers its commands call, installed in the home folder from files Jobdesk pins.

pub mod application;
mod domain;
mod infrastructure;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::onboarding::application::Onboarding;
#[cfg(target_os = "macos")]
use crate::onboarding::application::{Arch, HelperSetup};
#[cfg(target_os = "macos")]
use crate::onboarding::infrastructure::{HomeInstaller, ProgramProbe};
use crate::onboarding::infrastructure::{HttpsFetcher, ProgressFile, ZipUnpacker};
use crate::shared_kernel::https::HttpsTransport;

/// The folder in the app's data folder that holds each running Jobdesk's downloads, `onboarding/<pid>/`.
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

/// The Computer step: Python, Bun and LaTeX found and run the way the tool check does, and a missing one installed in
/// `home` from the files pinned for this copy's processor, downloaded into `data` first. It removes the half installs
/// a setup that stopped midway left. macOS only, and `None` on a processor setup has no files for.
#[cfg(target_os = "macos")]
pub fn helper_setup(data: &Path, current: &str, home: PathBuf) -> Option<HelperSetup> {
    let arch = Arch::of_this_build()?;
    let fetcher = HttpsFetcher::new(Arc::new(HttpsTransport::new(current)), downloads(data));
    let installer = HomeInstaller::new(fetcher, downloads(data), home, arch);
    Some(HelperSetup::new(Arc::new(ProgramProbe::new(crate::workspace::tool_version)), Arc::new(installer)))
}
