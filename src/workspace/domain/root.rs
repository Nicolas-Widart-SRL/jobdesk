use std::path::{Path, PathBuf};

/// The file whose presence makes a folder an `ai-job-search` checkout.
pub const MARKER: &str = ".claude/commands/apply.md";

/// A folder confirmed to be an `ai-job-search` checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRoot(PathBuf);

impl WorkspaceRoot {
    pub fn accept(path: &Path, exists: impl Fn(&Path) -> bool) -> Option<Self> {
        exists(&path.join(MARKER)).then(|| Self(path.to_owned()))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn name(&self) -> String {
        self.0.file_name().map_or_else(|| self.0.display().to_string(), |name| name.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_the_command_marker() {
        assert!(WorkspaceRoot::accept(Path::new("/w"), |p| p.ends_with("apply.md")).is_some());
        assert!(WorkspaceRoot::accept(Path::new("/w"), |_| false).is_none());
    }
}
