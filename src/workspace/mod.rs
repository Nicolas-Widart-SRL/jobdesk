//! The `ai-job-search` checkout this app views, its readiness and the changes made to it.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;
use std::sync::Arc;

use crate::workspace::application::{EnvironmentChecks, TOOLS, ToolProber, WorkspaceInspector, WorkspaceRoot, WorkspaceStatus, open_workspace};
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

/// The first line the tool `name` of `TOOLS` prints about itself, found and run as the tool check does: on macOS,
/// `python3` as a task's own command runs it. `None` when it cannot run, or the check knows no tool of that name.
pub fn tool_version(name: &str) -> Option<String> {
    TOOLS.iter().find(|tool| tool.name == name).and_then(|tool| ProcessProber.probe(tool))
}

/// All `program` prints when run with `args` from the folder where the tool check finds `tool`, so both belong to one
/// install. When `tool` links elsewhere, the folder of the file it links to comes first. `None` when either is missing,
/// or it fails or takes too long.
pub fn output_beside(tool: &str, program: &str, args: &[&str]) -> Option<String> {
    ProcessProber.beside(tool, program, args)
}
