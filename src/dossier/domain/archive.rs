use std::path::{Path, PathBuf};

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
/// Both separators count on every system, so a `..\` path is refused on macOS too.
/// Refused on every system: an empty path, a leading `/` or `\` (which covers a UNC start `\\`),
/// a `..` part, and any `:` anywhere. A `:` marks a drive (`C:`), which on Windows replaces
/// the path built so far, or an alternate data stream (`a.pdf:stream`).
pub fn within_workspace(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = relative.trim();
    if relative.is_empty() || relative.contains(':') || relative.starts_with(['/', '\\']) {
        return None;
    }
    let mut path = root.to_path_buf();
    for part in relative.split(['/', '\\']) {
        match part {
            ".." => return None,
            "" | "." => {}
            part => path.push(part),
        }
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_paths_cannot_leave_the_workspace() {
        let root = Path::new("/w");
        assert_eq!(within_workspace(root, "cv/main_a.tex"), Some(PathBuf::from("/w/cv/main_a.tex")));
        assert_eq!(within_workspace(root, "cv\\main_a.tex"), Some(PathBuf::from("/w/cv/main_a.tex")));
        assert_eq!(within_workspace(root, "cv/Résumé 2024 (final).tex"), Some(PathBuf::from("/w/cv/Résumé 2024 (final).tex")));
        for bad in [
            "../secret.pdf",
            "/etc/passwd",
            "cv/../../x",
            "  ",
            "..\\secret.pdf",
            "cv\\..\\..\\x",
            "C:x",
            "C:\\x",
            "\\\\host\\share\\x",
            "cv\\C:\\x",
            "cv/C:/x",
            "a/z:/x",
            "file.pdf:stream",
        ] {
            assert_eq!(within_workspace(root, bad), None, "{bad}");
        }
    }
}
