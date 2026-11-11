//! First-run setup: the progress record, the job search folder, and the ai-job-search release downloaded into it,
//! checked against the SHA-256 that Jobdesk's signed release feed pins before anything is unpacked. Then, on macOS,
//! the helpers its commands call, installed in Jobdesk's own folder of the home folder from files Jobdesk pins.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::NaiveDate;

#[cfg(target_os = "macos")]
use crate::onboarding::application::Arch;
use crate::onboarding::application::{HelperSetup, Onboarding, ProfileRun, Progress};
use crate::onboarding::infrastructure::{FileCopier, HttpsFetcher, ProgressFile, ZipUnpacker};
#[cfg(target_os = "macos")]
use crate::onboarding::infrastructure::{HomeInstaller, ProgramProbe};
use crate::shared_kernel::https::HttpsTransport;

/// The folder in the app's data folder that holds each running Jobdesk's downloads, `onboarding/<pid>/`.
fn downloads(data: &Path) -> PathBuf {
    data.join("onboarding")
}

/// First-run setup over the app's data folder `data`, where the record and the download live. `current` is the
/// version this copy runs, `documents` the user's Documents folder, `helpers` the Computer step, and `repaint` wakes
/// the window when a worker has news. It removes what a setup that stopped midway left. Nothing here starts `claude`
/// or writes a run log.
pub fn first_run(data: &Path, current: &str, documents: PathBuf, helpers: HelperSetup, repaint: Arc<dyn Fn() + Send + Sync>) -> Onboarding {
    let fetcher = HttpsFetcher::new(Arc::new(HttpsTransport::new(current)), downloads(data));
    let unpacker = ZipUnpacker::new(downloads(data), |folder| crate::workspace::open(folder).is_some());
    let store = Arc::new(ProgressFile::new(data));
    Onboarding::new(store, crate::updates::source_pins(current), Arc::new(fetcher), Arc::new(unpacker), helpers, documents, repaint)
}

/// The Profile chapter over the job search folder `folder` that first-run setup made, carrying on the record `progress`
/// that it holds, which stays in `data`. Its documents are copied into the folder's `documents/`, and its `/setup` runs
/// on the shell's `RunDesk`. `repaint` wakes the window when the worker that reads or copies the documents has news.
pub fn profile_run(data: &Path, folder: PathBuf, progress: Progress, repaint: Arc<dyn Fn() + Send + Sync>, today: NaiveDate) -> ProfileRun {
    ProfileRun::new(Arc::new(ProgressFile::new(data)), progress, folder, Arc::new(FileCopier::new(downloads(data))), repaint, today)
}

/// The Computer step: Python, Bun and LaTeX found and run the way the tool check does, so Python as a task runs
/// `python3`, and a missing one installed in `~/.jobdesk` under `home` from the files pinned for this copy's processor,
/// downloaded into `data` first. It removes the half installs a setup that stopped midway left. macOS only, and `None`
/// on a processor setup has no files for.
#[cfg(target_os = "macos")]
pub fn helper_setup(data: &Path, current: &str, home: PathBuf) -> Option<HelperSetup> {
    let arch = Arch::of_this_build()?;
    let fetcher = HttpsFetcher::new(Arc::new(HttpsTransport::new(current)), downloads(data));
    let installer = HomeInstaller::new(fetcher, downloads(data), home, arch);
    let probe = ProgramProbe::new(crate::workspace::tool_version, crate::workspace::output_beside);
    Some(HelperSetup::new(Arc::new(probe), Arc::new(installer)))
}
