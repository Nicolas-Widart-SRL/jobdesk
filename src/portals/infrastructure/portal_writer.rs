use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
use std::path::{Component, Path, PathBuf};

use crate::portals::domain::{ChangeError, Edit, PortalStore, set_enabled};
use crate::portals::infrastructure::portal_folder::SKILLS;
use crate::shared_kernel::LoadError;
use crate::shared_kernel::safe_save::{Refusals, refusal, replace_through_copy};
use crate::shared_kernel::visible_text::all_visible;

const MANIFEST: &str = "SKILL.md";
/// A `SKILL.md` larger than this is not edited, so a huge file cannot stall the screen. A real one is a few KiB.
const EDIT_LIMIT: u64 = 1024 * 1024;

/// The writes to the portal folders of one checkout: the `enabled:` lines of a `SKILL.md`, and a portal folder moved to
/// the Trash. A portal folder is never deleted, only moved to the Trash. The only files removed are Jobdesk's own
/// temporary copies of a `SKILL.md`: the copy of a switch that failed, and copies an interrupted switch left, as a
/// profile save removes its own.
pub struct PortalWriter {
    root: PathBuf,
}

impl PortalWriter {
    pub fn in_workspace(root: &Path) -> Self {
        Self { root: root.to_owned() }
    }

    /// The portal folder named `folder` in `.agents/skills`, checked before any write: one plain name that is not a dot
    /// folder, spelled exactly as `.agents/skills` lists it, a `.agents/skills` that resolves inside the root, and a real
    /// folder holding a regular `SKILL.md`, neither one a link. The exact spelling matters on a disk that ignores case:
    /// there `LinkedIn-Search` would reach the built-in `linkedin-search`, which only that exact name is known as.
    ///
    /// The check runs once, before the write, as a profile save's does: a folder swapped for a link between the two is
    /// not caught, because the standard library has no folder-relative open that would close that gap.
    fn portal(&self, folder: &str) -> Result<PathBuf, LoadError> {
        let refused = |why: &str| LoadError::new(folder, why);
        let failed = |error: io::Error| LoadError::new(folder, error);
        if !is_folder_name(folder) {
            return Err(refused("is not a portal folder name, so nothing changed"));
        }
        let root = fs::canonicalize(&self.root).map_err(failed)?;
        let skills = fs::canonicalize(self.root.join(SKILLS)).map_err(failed)?;
        if !skills.starts_with(&root) {
            return Err(refused("is outside your job search folder, so nothing changed"));
        }
        if !fs::read_dir(&skills).map_err(failed)?.flatten().any(|entry| entry.file_name() == *folder) {
            return Err(refused("is not a folder in .agents/skills by that exact name, so nothing changed"));
        }
        let path = skills.join(folder);
        if !fs::symlink_metadata(&path).map_err(failed)?.file_type().is_dir() {
            return Err(refused("is a link or not a folder, so nothing changed"));
        }
        if !fs::symlink_metadata(path.join(MANIFEST)).map_err(failed)?.file_type().is_file() {
            return Err(refused("has a SKILL.md that is a link or not a file, so nothing changed"));
        }
        Ok(path)
    }
}

impl PortalStore for PortalWriter {
    /// Reads `SKILL.md` fresh, edits its `enabled:` lines and replaces it as a profile save does: through a copy beside
    /// it, synced, then renamed with no wait for a file another program holds, since the text came from that one read.
    fn set_enabled(&self, folder: &str, on: bool) -> Result<(), ChangeError> {
        let file = self.portal(folder).map_err(ChangeError::Failed)?.join(MANIFEST);
        let source = format!("{folder}/{MANIFEST}");
        let failed = |error: io::Error| ChangeError::Failed(LoadError::new(&source, error));
        let bytes = read_whole(&file).map_err(failed)?;
        let text = String::from_utf8(bytes).map_err(|_| ChangeError::Failed(LoadError::new(&source, "is not UTF-8 text, so nothing changed")))?;
        match set_enabled(&text, on) {
            Edit::Unchanged => Ok(()),
            Edit::NoFrontmatter => Err(ChangeError::NoFrontmatter),
            Edit::Changed(text) => {
                let metadata = fs::metadata(&file).map_err(failed)?;
                if let Some(why) = refusal(&file, &metadata, &REFUSALS) {
                    return Err(ChangeError::Failed(LoadError::new(&source, why)));
                }
                replace_through_copy(&file, &text).map_err(failed)
            }
        }
    }

    /// Moves the portal folder to the Trash, never deleting it, so the user can put it back.
    fn trash(&self, folder: &str) -> Result<(), ChangeError> {
        let path = self.portal(folder).map_err(ChangeError::Failed)?;
        move_to_trash(&path).map_err(|why| ChangeError::Failed(LoadError::new(folder, why)))?;
        if fs::symlink_metadata(&path).is_ok() {
            return Err(ChangeError::Failed(LoadError::new(folder, "is still there after the move to the Trash")));
        }
        Ok(())
    }
}

/// Whether `name` is one visible path part that is not a dot folder: no separator, no `.` or `..`, no drive.
fn is_folder_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    let one_part = matches!((parts.next(), parts.next()), (Some(Component::Normal(part)), None) if part == name);
    one_part && !name.starts_with('.') && !name.contains(['/', '\\', ':']) && all_visible(name)
}

/// The whole file, refused past `EDIT_LIMIT`.
fn read_whole(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(EDIT_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > EDIT_LIMIT {
        return Err(io::Error::new(ErrorKind::FileTooLarge, "is over 1 MiB, too large to edit"));
    }
    Ok(bytes)
}

/// Why a switch refuses a `SKILL.md`, as a profile save refuses a file. Unlike a profile save, a switch refuses a
/// `SKILL.md` gone since it read it, because writing its edit would put back a portal another program removed.
const REFUSALS: Refusals = Refusals {
    read_only: "is read-only, so nothing changed",
    not_yours: "belongs to another user, so nothing changed",
    cannot_write: "cannot be written, or is open in another program, so nothing changed",
    gone: Some("is no longer there, so nothing changed"),
};

/// Moves `path` to the Trash through `NSFileManager`'s `trashItemAtURL`, never through Finder and AppleScript, so macOS
/// asks for no Automation permission.
#[cfg(target_os = "macos")]
fn move_to_trash(path: &Path) -> Result<(), String> {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut context = trash::TrashContext::new();
    context.set_delete_method(DeleteMethod::NsFileManager);
    context.delete(path).map_err(|error| error.to_string())
}

/// Moves `path` to the Recycle Bin through the shell's `IFileOperation`, with undo allowed.
#[cfg(windows)]
fn move_to_trash(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|error| error.to_string())
}

/// Jobdesk ships for macOS and Windows only.
#[cfg(not(any(target_os = "macos", windows)))]
fn move_to_trash(_path: &Path) -> Result<(), String> {
    Err("cannot be moved to the Trash on this system, so nothing changed".to_owned())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::portals::domain::parse_manifest;

    const NORTHFIELD: &str = "---\nname: northfield-search\nenabled: false  # Northfield demo portal - ships opt-in\n---\n\n# Northfield Search Skill\n\nSearch Northfield's board. Set enabled: false to skip it.\n";

    fn workspace() -> (tempfile::TempDir, PortalWriter) {
        let dir = tempfile::tempdir().unwrap();
        portal(dir.path(), "northfield-search", NORTHFIELD);
        let writer = PortalWriter::in_workspace(dir.path());
        (dir, writer)
    }

    fn portal(root: &Path, folder: &str, text: &str) -> PathBuf {
        let path = root.join(SKILLS).join(folder);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join(MANIFEST), text).unwrap();
        path
    }

    fn manifest(root: &Path, folder: &str) -> String {
        fs::read_to_string(root.join(SKILLS).join(folder).join(MANIFEST)).unwrap()
    }

    fn refused(result: Result<(), ChangeError>) -> String {
        match result {
            Err(ChangeError::Failed(error)) => error.reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_switch_round_trips_and_keeps_the_prose() {
        let (dir, writer) = workspace();
        writer.set_enabled("northfield-search", true).unwrap();
        let on = manifest(dir.path(), "northfield-search");
        assert_eq!(on, NORTHFIELD.replace("enabled: false  #", "enabled: true  #"));
        assert!(parse_manifest(&on, "northfield-search").enabled);
        writer.set_enabled("northfield-search", false).unwrap();
        assert_eq!(manifest(dir.path(), "northfield-search"), NORTHFIELD);
        let names: Vec<_> = fs::read_dir(dir.path().join(SKILLS).join("northfield-search")).unwrap().flatten().map(|entry| entry.file_name()).collect();
        assert_eq!(names, ["SKILL.md"], "no copy is left behind");
    }

    /// A copy a switch cut off by a crash left over an hour ago holds the file's text, so the next switch removes it. A
    /// fresh copy may be a live switch's, so it stays.
    #[test]
    fn a_copy_a_crash_left_goes_with_the_next_switch() {
        let (dir, writer) = workspace();
        let folder = dir.path().join(SKILLS).join("northfield-search");
        let peer = std::process::id().wrapping_add(1);
        let stale = folder.join(format!("SKILL.md.jobdesk-save-{peer}-3"));
        let fresh = folder.join(format!("SKILL.md.jobdesk-save-{peer}-4"));
        for (copy, ago) in [(&stale, 2 * 60 * 60), (&fresh, 30 * 60)] {
            fs::write(copy, NORTHFIELD).unwrap();
            File::options().write(true).open(copy).unwrap().set_modified(SystemTime::now() - Duration::from_secs(ago)).unwrap();
        }
        writer.set_enabled("northfield-search", true).unwrap();
        assert!(!stale.exists());
        assert!(fresh.exists());
    }

    #[test]
    fn a_file_already_as_asked_is_not_written() {
        let (dir, writer) = workspace();
        let file = dir.path().join(SKILLS).join("northfield-search").join(MANIFEST);
        let modified = fs::metadata(&file).unwrap().modified().unwrap();
        writer.set_enabled("northfield-search", false).unwrap();
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
    }

    #[test]
    fn off_without_a_frontmatter_writes_nothing() {
        let (dir, writer) = workspace();
        portal(dir.path(), "alder-search", "# Alder Search Skill\n");
        assert_eq!(writer.set_enabled("alder-search", false), Err(ChangeError::NoFrontmatter));
        assert_eq!(writer.set_enabled("alder-search", true), Ok(()));
        assert_eq!(manifest(dir.path(), "alder-search"), "# Alder Search Skill\n");
    }

    #[test]
    fn a_name_that_is_not_one_plain_folder_is_refused() {
        let (dir, writer) = workspace();
        portal(dir.path(), ".hidden-search", NORTHFIELD);
        for name in [
            "..",
            ".",
            "",
            ".hidden-search",
            "../northfield-search",
            "northfield-search/",
            "a/b",
            "a\\b",
            "c:northfield",
            "north\u{200B}field",
            "northfield\u{FE00}-search",
        ] {
            assert!(refused(writer.set_enabled(name, true)).contains("not a portal folder name"), "{name:?}");
            assert!(refused(writer.trash(name)).contains("not a portal folder name"), "{name:?}");
        }
        assert_eq!(manifest(dir.path(), ".hidden-search"), NORTHFIELD);
    }

    /// On a disk that ignores case another spelling would reach the folder, and the built-in check knows only the exact
    /// name, so a name `.agents/skills` does not list exactly is refused, on any disk.
    #[test]
    fn a_name_in_another_case_is_refused() {
        let (dir, writer) = workspace();
        portal(dir.path(), "linkedin-search", NORTHFIELD);
        for name in ["Northfield-Search", "NORTHFIELD-SEARCH", "LinkedIn-Search"] {
            assert!(refused(writer.set_enabled(name, true)).contains("exact name"), "{name}");
            assert!(refused(writer.trash(name)).contains("exact name"), "{name}");
        }
        assert_eq!(manifest(dir.path(), "northfield-search"), NORTHFIELD);
        assert_eq!(manifest(dir.path(), "linkedin-search"), NORTHFIELD);
        assert_eq!(writer.set_enabled("northfield-search", true), Ok(()), "the exact name still works");
    }

    #[test]
    fn a_missing_folder_or_manifest_is_an_error() {
        let (dir, writer) = workspace();
        fs::create_dir_all(dir.path().join(SKILLS).join("empty-search")).unwrap();
        assert!(matches!(writer.set_enabled("gone-search", true), Err(ChangeError::Failed(_))));
        assert!(matches!(writer.trash("empty-search"), Err(ChangeError::Failed(_))));
        assert!(dir.path().join(SKILLS).join("empty-search").exists(), "a folder without SKILL.md is no portal to remove");
    }

    #[test]
    fn a_large_or_non_utf8_manifest_is_not_edited() {
        let (dir, writer) = workspace();
        let large = format!("{NORTHFIELD}{}", "x".repeat(EDIT_LIMIT as usize));
        portal(dir.path(), "large-search", &large);
        assert!(refused(writer.set_enabled("large-search", true)).contains("too large"));
        let path = portal(dir.path(), "latin-search", "");
        fs::write(path.join(MANIFEST), b"---\nenabled: false\n---\nCaf\xe9\n").unwrap();
        assert!(refused(writer.set_enabled("latin-search", true)).contains("UTF-8"));
    }

    /// A `SKILL.md` another program removed after the switch read it is refused, so the switch never puts it back.
    #[test]
    fn a_manifest_gone_before_the_write_is_refused() {
        let (dir, _writer) = workspace();
        let file = dir.path().join(SKILLS).join("northfield-search").join(MANIFEST);
        let metadata = fs::metadata(&file).unwrap();
        fs::remove_file(&file).unwrap();
        assert_eq!(refusal(&file, &metadata, &REFUSALS), Some("is no longer there, so nothing changed"));
    }

    #[test]
    fn a_read_only_manifest_is_refused() {
        let (dir, writer) = workspace();
        let file = dir.path().join(SKILLS).join("northfield-search").join(MANIFEST);
        let mut permissions = fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions).unwrap();
        assert!(refused(writer.set_enabled("northfield-search", true)).contains("read-only"));
        assert_eq!(manifest(dir.path(), "northfield-search"), NORTHFIELD);
    }

    #[cfg(unix)]
    #[test]
    fn a_switch_keeps_the_permission_bits() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, writer) = workspace();
        let file = dir.path().join(SKILLS).join("northfield-search").join(MANIFEST);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        writer.set_enabled("northfield-search", true).unwrap();
        assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_folder_is_refused_and_kept() {
        let (dir, writer) = workspace();
        let outside = tempfile::tempdir().unwrap();
        let target = portal(outside.path(), "elsewhere-search", NORTHFIELD);
        let link = dir.path().join(SKILLS).join("linked-search");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(refused(writer.set_enabled("linked-search", true)).contains("link"));
        assert!(refused(writer.trash("linked-search")).contains("link"));
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(target.join(MANIFEST)).unwrap(), NORTHFIELD);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_manifest_is_refused() {
        let (dir, writer) = workspace();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join(MANIFEST);
        fs::write(&target, NORTHFIELD).unwrap();
        let folder = dir.path().join(SKILLS).join("linked-search");
        fs::create_dir_all(&folder).unwrap();
        std::os::unix::fs::symlink(&target, folder.join(MANIFEST)).unwrap();
        assert!(refused(writer.set_enabled("linked-search", true)).contains("SKILL.md that is a link"));
        assert!(refused(writer.trash("linked-search")).contains("SKILL.md that is a link"));
        assert_eq!(fs::read_to_string(&target).unwrap(), NORTHFIELD);
        assert!(folder.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_skills_folder_outside_the_root_is_refused() {
        let outside = tempfile::tempdir().unwrap();
        portal(outside.path(), "northfield-search", NORTHFIELD);
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".agents")).unwrap();
        std::os::unix::fs::symlink(outside.path().join(SKILLS), dir.path().join(SKILLS)).unwrap();
        let writer = PortalWriter::in_workspace(dir.path());
        assert!(refused(writer.set_enabled("northfield-search", true)).contains("outside"));
        assert!(refused(writer.trash("northfield-search")).contains("outside"));
        assert_eq!(manifest(outside.path(), "northfield-search"), NORTHFIELD);
    }

    /// Moves a temporary folder to the real Trash, so it runs only on demand:
    /// `cargo test portal_writer -- --ignored`.
    #[test]
    #[ignore = "moves a temporary folder to the real Trash"]
    fn a_removal_moves_the_folder_to_the_trash() {
        let (dir, writer) = workspace();
        writer.trash("northfield-search").unwrap();
        assert!(fs::symlink_metadata(dir.path().join(SKILLS).join("northfield-search")).is_err());
        assert!(dir.path().join(SKILLS).exists(), "only the portal folder moves");
    }
}
