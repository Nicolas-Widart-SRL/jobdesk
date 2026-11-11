//! Updating a job search folder to a newer ai-job-search release: each file merged three ways, between the release
//! setup kept in the folder's `.jobdesk/`, the folder as the user left it, and the new release. An update comes only
//! with a Jobdesk update: the new release is the one the signed feed pins, and only for the Jobdesk the feed names as
//! the latest.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;
use std::sync::Arc;

use crate::folder_update::application::FolderUpdate;
use crate::folder_update::infrastructure::{DiskFolder, IdleFolder};
use crate::shared_kernel::download::HttpsFetcher;
use crate::shared_kernel::https::HttpsTransport;

/// Folder updates over the disk and GitHub. `data` is the app's data folder: the release downloads into this Jobdesk's
/// own folder of its `onboarding/`, as setup's does, and is deleted once read. `current` is the version this Jobdesk
/// runs, and `repaint` wakes the window when the worker has news. Nothing here starts `claude` or writes a run log.
///
/// No update, undo or rollback may run while a task or a Claude session is open: see `FolderUpdate`.
pub fn folder_update(data: &Path, current: &str, repaint: Arc<dyn Fn() + Send + Sync>) -> FolderUpdate {
    let fetcher = HttpsFetcher::new(Arc::new(HttpsTransport::new(current)), data.join("onboarding"));
    let folder = Arc::new(DiskFolder::default());
    FolderUpdate::new(folder.clone(), folder.clone(), Arc::new(fetcher), folder, current, repaint)
}

/// Folder updates that read and write no folder and reach no network, for tests and previews: every folder is gone,
/// so nothing is ever updated. Give it a stage to show with `FolderUpdate::showing`.
pub fn idle_folder_update(current: &str) -> FolderUpdate {
    let idle = Arc::new(IdleFolder);
    FolderUpdate::new(idle.clone(), idle.clone(), idle.clone(), idle, current, Arc::new(|| {}))
}
