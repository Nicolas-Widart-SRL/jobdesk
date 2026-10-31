use std::path::Path;

use crate::workspace::domain::WorkspaceRoot as Root;

pub type WorkspaceRoot = Root;

pub fn open_workspace(path: &Path, exists: impl Fn(&Path) -> bool) -> Option<WorkspaceRoot> {
    Root::accept(path, exists)
}
