use std::path::{Component, Path, PathBuf};

use chrono::NaiveDate;

/// What identifies an application's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRef {
    pub folder: String,
    pub company: String,
    pub cv_file: String,
    pub cover_letter_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostingText {
    pub text: String,
    pub guard: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedText {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchNote {
    pub fetched_on: Option<NaiveDate>,
    pub is_stale: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PdfDocument {
    pub label: String,
    pub path: PathBuf,
    /// Seconds since the epoch. A rebuilt PDF renders again.
    pub modified: u64,
}

/// Everything on disk about one application.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArchiveContents {
    pub posting: Option<PostingText>,
    pub prep_packs: Vec<NamedText>,
    pub follow_ups: Vec<NamedText>,
    pub research: Option<ResearchNote>,
    pub documents: Vec<PdfDocument>,
}

pub trait ArchiveRepository {
    fn load(&self, application: &ApplicationRef, today: NaiveDate) -> ArchiveContents;
}

/// Tracker cells are free text, so a stored path may only point inside the workspace.
pub fn within_workspace(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative.trim());
    let is_plain = !relative.trim().is_empty() && path.components().all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    is_plain.then(|| root.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_paths_cannot_leave_the_workspace() {
        let root = Path::new("/w");
        assert_eq!(within_workspace(root, "cv/main_a.tex"), Some(PathBuf::from("/w/cv/main_a.tex")));
        for bad in ["../secret.pdf", "/etc/passwd", "cv/../../x", "  "] {
            assert_eq!(within_workspace(root, bad), None, "{bad}");
        }
    }
}
