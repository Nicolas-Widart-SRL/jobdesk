use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use crate::portals::domain::{Manifest, SearchPlan, parse_manifest, parse_search_plan};
use crate::shared_kernel::LoadError;
use crate::shared_kernel::visible_text::visible_chars;

const SKILLS: &str = ".agents/skills";
const SEARCH_FILE: &str = ".claude/skills/job-scraper/search-queries.md";
/// A `SKILL.md` is read up to this size, so a huge file cannot stall the snapshot.
const MANIFEST_LIMIT: u64 = 256 * 1024;

/// The portal folders of a checkout and its search file.
pub struct PortalFolder {
    root: PathBuf,
}

impl PortalFolder {
    pub fn in_workspace(root: &Path) -> Self {
        Self { root: root.to_owned() }
    }

    /// Each portal folder with its manifest. No `.agents/skills` means none. A folder whose name starts with a dot, is
    /// not UTF-8 or holds hidden characters is left out, and so is one without a readable `SKILL.md`.
    pub fn installed(&self) -> Result<Vec<(String, Manifest)>, LoadError> {
        let skills = self.root.join(SKILLS);
        let entries = match fs::read_dir(&skills) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(LoadError::new(SKILLS, error)),
        };
        Ok(entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let shown = !name.starts_with('.') && visible_chars(&name).eq(name.chars());
                let text = shown.then(|| read_manifest(&entry.path().join("SKILL.md")))??;
                Some((name.clone(), parse_manifest(&text, &name)))
            })
            .collect())
    }

    /// The shared search terms. No search file means none.
    pub fn search_plan(&self) -> Result<SearchPlan, LoadError> {
        match fs::read_to_string(self.root.join(SEARCH_FILE)) {
            Ok(text) => Ok(parse_search_plan(&text)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(SearchPlan::default()),
            Err(error) => Err(LoadError::new(SEARCH_FILE, error)),
        }
    }
}

/// The start of a `SKILL.md`, or `None` when it cannot be read. A character cut at the limit reads as a replacement.
fn read_manifest(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path).ok()?.take(MANIFEST_LIMIT).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn folders(installed: &[(String, Manifest)]) -> Vec<&str> {
        let mut names: Vec<&str> = installed.iter().map(|(name, _)| name.as_str()).collect();
        names.sort();
        names
    }

    #[test]
    fn reads_each_portal_folder_with_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, ".agents/skills/northfield-search/SKILL.md", "---\nenabled: false  # off for now\n---\n\n# Northfield Search Skill\n");
        write(root, ".agents/skills/alder-search/SKILL.md", "# Alder Search Skill\n");
        write(root, ".agents/skills/.hidden-search/SKILL.md", "# Hidden Search Skill\n");
        write(root, ".agents/skills/zero\u{200B}width-search/SKILL.md", "# Zero Search Skill\n");
        write(root, ".agents/skills/empty-search/README.md", "no manifest\n");
        write(root, ".agents/skills/stray.md", "a file, not a folder\n");

        let installed = PortalFolder::in_workspace(root).installed().unwrap();

        assert_eq!(folders(&installed), ["alder-search", "northfield-search"]);
        let northfield = &installed.iter().find(|(name, _)| name == "northfield-search").unwrap().1;
        assert_eq!((northfield.name.as_str(), northfield.enabled), ("Northfield", false));
    }

    #[test]
    fn a_large_manifest_is_read_up_to_its_limit() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("# Alder Search Skill\n\n{}\n\nQuery syntax: an exact phrase.\n", "x".repeat(MANIFEST_LIMIT as usize));
        write(dir.path(), ".agents/skills/alder-search/SKILL.md", &text);
        let installed = PortalFolder::in_workspace(dir.path()).installed().unwrap();
        assert_eq!(installed[0].1.name, "Alder");
        assert_eq!(installed[0].1.quotes, crate::portals::domain::Quotes::Unknown, "the line past the limit is not read");
    }

    #[test]
    fn no_skills_folder_and_no_search_file_read_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let folder = PortalFolder::in_workspace(dir.path());
        assert_eq!(folder.installed(), Ok(Vec::new()));
        assert_eq!(folder.search_plan(), Ok(SearchPlan::default()));
    }

    #[test]
    fn a_skills_path_that_is_no_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".agents/skills", "a file");
        let error = PortalFolder::in_workspace(dir.path()).installed().unwrap_err();
        assert_eq!(error.source, ".agents/skills");
    }

    #[test]
    fn a_search_file_that_cannot_be_read_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(SEARCH_FILE)).unwrap();
        assert_eq!(PortalFolder::in_workspace(dir.path()).search_plan().unwrap_err().source, SEARCH_FILE);
        fs::remove_dir(dir.path().join(SEARCH_FILE)).unwrap();
        fs::write(dir.path().join(SEARCH_FILE), b"## Query Categories\n\n### Priority 1: Caf\xe9\n").unwrap();
        assert!(PortalFolder::in_workspace(dir.path()).search_plan().is_err(), "a file that is not UTF-8");
        write(dir.path(), SEARCH_FILE, "## Query Categories\n\n### Priority 1: Cafe\n");
        assert_eq!(PortalFolder::in_workspace(dir.path()).search_plan().unwrap().priorities[0].name, "Cafe");
    }
}
