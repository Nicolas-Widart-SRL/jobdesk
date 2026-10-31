//! The `ai-job-search` checkout this app views, its readiness and the changes made to it.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;
use std::sync::Arc;

use crate::workspace::application::{EnvironmentChecks, WorkspaceInspector, WorkspaceRoot, WorkspaceStatus, open_workspace};
pub use crate::workspace::infrastructure::ChangeWatcher;
use crate::workspace::infrastructure::{CheckoutInspector, ProcessProber};

pub fn open(path: &Path) -> Option<WorkspaceRoot> {
    open_workspace(path, Path::exists)
}

pub fn inspect(root: &Path) -> WorkspaceStatus {
    CheckoutInspector::in_workspace(root).status()
}

pub fn environment_checks(repaint: Arc<dyn Fn() + Send + Sync>) -> EnvironmentChecks {
    EnvironmentChecks::new(Arc::new(ProcessProber), repaint)
}
