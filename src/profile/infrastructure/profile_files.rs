use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::profile::domain::{ProfileFile, ProfileStore};
use crate::shared_kernel::LoadError;

/// Added to a file's name for the copy a save writes before it replaces the file.
const SAVE_SUFFIX: &str = ".jobdesk-save";

/// The profile files of one checkout. A save of one block is the only write Jobdesk makes to the checkout.
pub struct ProfileFiles {
    root: PathBuf,
}

impl ProfileFiles {
    pub fn in_workspace(root: &Path) -> Self {
        Self { root: root.to_owned() }
    }

    fn path(&self, file: ProfileFile) -> PathBuf {
        file.relative_path().split('/').fold(self.root.clone(), |path, part| path.join(part))
    }

    /// The file a save replaces, with every link resolved, checked to sit inside the root. A file that does not exist yet
    /// resolves through its folder. A link to nothing is refused.
    ///
    /// The check runs once, before the write. A folder swapped for a link between the two can still send the write out
    /// of the root: the standard library has no folder-relative open that would close that gap. So the check stops links
    /// that are in place when the user saves, not a link made during the save.
    fn target(&self, file: ProfileFile) -> Result<PathBuf, LoadError> {
        let failed = |error: io::Error| LoadError::new(file.name(), error);
        let gone = |why: &str| {
            let why = why.to_owned();
            move |error: io::Error| if error.kind() == ErrorKind::NotFound { LoadError::new(file.name(), &why) } else { failed(error) }
        };
        let root = fs::canonicalize(&self.root).map_err(gone("is in a job search folder that is not there, so it was not saved"))?;
        let path = self.path(file);
        let target = match fs::symlink_metadata(&path) {
            Ok(_) => fs::canonicalize(&path).map_err(gone("links to a file that is not there, so it was not saved"))?,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let folder = path.parent().ok_or_else(|| LoadError::new(file.name(), "has no folder"))?;
                let missing = format!("cannot be created, because its folder {} is not there", folder_of(file));
                fs::canonicalize(folder).map_err(gone(&missing))?.join(file.name())
            }
            Err(error) => return Err(failed(error)),
        };
        if !target.starts_with(&root) {
            return Err(LoadError::new(file.name(), "is outside your job search folder, so it was not saved"));
        }
        if let Ok(metadata) = fs::metadata(&target) {
            if metadata.permissions().readonly() {
                return Err(LoadError::new(file.name(), "is read-only, so it was not saved"));
            }
            #[cfg(unix)]
            if let Some(why) = not_yours(&target, &metadata, current_user()) {
                return Err(LoadError::new(file.name(), why));
            }
        }
        Ok(target)
    }
}

#[cfg(unix)]
fn current_user() -> libc::uid_t {
    // SAFETY: geteuid takes nothing and always succeeds.
    unsafe { libc::geteuid() }
}

/// Why `user` may not replace the file at `target`, if they may not. The rename puts a new file in its place, owned by
/// whoever saves, so a file owned by another user would quietly change hands, and a file `user` cannot write in place
/// would be replaced all the same, because the rename needs only the folder.
#[cfg(unix)]
fn not_yours(target: &Path, metadata: &fs::Metadata, user: libc::uid_t) -> Option<&'static str> {
    use std::os::unix::fs::MetadataExt;
    if metadata.uid() != user {
        return Some("belongs to another user, so it was not saved");
    }
    // The mode bits can allow a write that an access list denies, so the file is opened for writing, which changes nothing.
    if metadata.is_file() && OpenOptions::new().write(true).open(target).is_err() {
        return Some("cannot be written by your user, so it was not saved");
    }
    None
}

/// The folder of `file` from the root, as the user would look for it.
fn folder_of(file: ProfileFile) -> &'static str {
    let path = file.relative_path();
    path.rsplit_once('/').map_or(".", |(folder, _)| folder)
}

/// Writes a copy beside `target`, then renames it over `target`, so a crash leaves the old file or the new one, never half.
/// The copy is removed when anything fails.
fn replace(target: &Path, text: &str) -> io::Result<()> {
    let (Some(folder), Some(name)) = (target.parent(), target.file_name()) else { return Err(ErrorKind::InvalidInput.into()) };
    let mut copy_name = name.to_owned();
    copy_name.push(SAVE_SUFFIX);
    let copy = folder.join(copy_name);
    // A copy left by a crash goes first. Removing a link removes the link, not the file it points to.
    match fs::remove_file(&copy) {
        Err(error) if error.kind() != ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let result = write_copy(&copy, target, |file| file.write_all(text.as_bytes())).and_then(|()| fs::rename(&copy, target));
    if result.is_err() {
        let _ = fs::remove_file(&copy);
    }
    result?;
    // The rename lives in the folder, which a crash could still lose. Windows cannot open a folder this way.
    // Best effort: some mounts refuse to sync a folder, and the file is already in place.
    #[cfg(unix)]
    let _ = fs::File::open(folder).and_then(|folder| folder.sync_all());
    Ok(())
}

/// `create_new` never follows a link, so the copy is a new file in the folder. It keeps the permission bits of `target`,
/// set only once `write` has put the text in. Anything else tied to the old file is lost with it: its other hard links
/// keep the old text, and its ACLs and extended attributes are not copied.
fn write_copy(copy: &Path, target: &Path, write: impl FnOnce(&mut fs::File) -> io::Result<()>) -> io::Result<()> {
    let kept = fs::metadata(target).ok().map(|metadata| metadata.permissions());
    let mut file = create_copy(copy, kept.is_some())?;
    write(&mut file)?;
    if let Some(permissions) = kept {
        file.set_permissions(permissions)?;
    }
    // On disk before the rename, or a power loss could keep the rename and lose the text.
    file.sync_all()
}

/// Creates the copy, empty. When it `replaces` a file, on Unix only its owner may open it until the text is in, so the
/// text of a private file is never open to other users, not even while it is written. A new file gets the usual mode.
fn create_copy(copy: &Path, replaces: bool) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if replaces {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = replaces;
    options.open(copy)
}

impl ProfileStore for ProfileFiles {
    fn read(&self, file: ProfileFile) -> Result<Option<String>, LoadError> {
        let bytes = match fs::read(self.path(file)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(LoadError::new(file.name(), error)),
        };
        String::from_utf8(bytes).map(Some).map_err(|_| LoadError::new(file.name(), "not UTF-8 text, re-save it as UTF-8"))
    }

    fn write(&self, file: ProfileFile, text: &str) -> Result<(), LoadError> {
        let target = self.target(file)?;
        replace(&target, text).map_err(|error| LoadError::new(file.name(), error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> (tempfile::TempDir, ProfileFiles) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude/skills/job-application-assistant")).unwrap();
        let files = ProfileFiles::in_workspace(dir.path());
        (dir, files)
    }

    #[test]
    fn a_missing_file_reads_as_none_and_bad_bytes_as_an_error() {
        let (dir, files) = files();
        assert_eq!(files.read(ProfileFile::Candidate), Ok(None));
        fs::write(dir.path().join(ProfileFile::Behavioral.relative_path()), b"Caf\xe9\n").unwrap();
        let error = files.read(ProfileFile::Behavioral).unwrap_err();
        assert_eq!(error.source, "02-behavioral-profile.md");
        assert!(error.reason.contains("UTF-8"));
    }

    #[test]
    fn a_write_replaces_the_file_and_leaves_no_copy() {
        let (dir, files) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        files.write(ProfileFile::Candidate, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        let left: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().flatten().map(|entry| entry.file_name()).collect();
        assert_eq!(left, [std::ffi::OsString::from("01-candidate-profile.md")]);
    }

    #[test]
    fn a_stale_copy_is_replaced_and_a_failed_rename_removes_the_copy() {
        let (dir, files) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        let copy = path.with_file_name("01-candidate-profile.md.jobdesk-save");
        fs::write(&copy, "stale").unwrap();
        fs::write(&path, "old").unwrap();
        files.write(ProfileFile::Candidate, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(!copy.exists());
        // A folder where the file should be: the rename fails, and the copy goes.
        let folder = dir.path().join(ProfileFile::Behavioral.relative_path());
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("inside"), "x").unwrap();
        assert!(files.write(ProfileFile::Behavioral, "new").is_err());
        assert!(!folder.with_file_name("02-behavioral-profile.md.jobdesk-save").exists());
    }

    #[test]
    fn a_read_only_file_is_not_saved() {
        let (dir, files) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        assert!(files.write(ProfileFile::Candidate, "new").unwrap_err().reason.contains("read-only"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
    }

    /// The rename would hand the file to the current user, so a file someone else owns is not saved.
    #[cfg(unix)]
    #[test]
    fn a_file_owned_by_another_user_is_not_saved() {
        let (dir, _) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!(not_yours(&path, &metadata, current_user().wrapping_add(1)), Some("belongs to another user, so it was not saved"));
        assert_eq!(not_yours(&path, &metadata, current_user()), None);
    }

    /// Write bits for the group alone do not read as read-only, yet the owner cannot write the file in place.
    #[cfg(unix)]
    #[test]
    fn a_file_its_owner_cannot_write_is_not_saved() {
        use std::os::unix::fs::PermissionsExt;
        if current_user() == 0 {
            return;
        }
        let (dir, files) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o464)).unwrap();
        assert!(!fs::metadata(&path).unwrap().permissions().readonly());
        assert_eq!(files.write(ProfileFile::Candidate, "new").unwrap_err().reason, "cannot be written by your user, so it was not saved");
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
    }

    #[cfg(unix)]
    #[test]
    fn a_private_file_stays_private() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, files) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        files.write(ProfileFile::Candidate, "new").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    /// Puts the process umask back when dropped, even when the test fails.
    #[cfg(unix)]
    struct Umask(libc::mode_t);

    #[cfg(unix)]
    impl Drop for Umask {
        fn drop(&mut self) {
            // SAFETY: umask takes a mask and always succeeds.
            unsafe { libc::umask(self.0) };
        }
    }

    /// Another user who opens the copy while the text goes in would keep reading it, so it is private until then. The
    /// umask is `022` for the write, so a copy opened with the default mode would read `644`, not `600`, whatever the
    /// umask of the shell. Files other tests make meanwhile may get `022` too, and none of them checks a default mode.
    #[cfg(unix)]
    #[test]
    fn the_copy_is_private_until_its_text_is_in() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, _) = files();
        let target = dir.path().join("01-candidate-profile.md");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        let copy = dir.path().join("01-candidate-profile.md.jobdesk-save");
        let mut while_written = Vec::new();
        {
            // SAFETY: umask takes a mask and always succeeds.
            let _umask = Umask(unsafe { libc::umask(0o022) });
            write_copy(&copy, &target, |file| {
                while_written.push(file.metadata()?.permissions().mode() & 0o777);
                file.write_all(b"new")?;
                while_written.push(file.metadata()?.permissions().mode() & 0o777);
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(while_written, [0o600, 0o600]);
        // The target's own bits come back once the text is in.
        assert_eq!(fs::metadata(&copy).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(fs::read_to_string(&copy).unwrap(), "new");
    }

    #[test]
    fn a_missing_folder_says_which() {
        let (dir, files) = files();
        let error = files.write(ProfileFile::SearchQueries, "new").unwrap_err();
        assert_eq!(error.reason, "cannot be created, because its folder .claude/skills/job-scraper is not there");
        let gone = ProfileFiles::in_workspace(&dir.path().join("moved"));
        assert_eq!(gone.write(ProfileFile::Candidate, "new").unwrap_err().reason, "is in a job search folder that is not there, so it was not saved");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_the_root_is_followed_and_kept() {
        let (dir, files) = files();
        let real = dir.path().join("profile-notes.md");
        fs::write(&real, "old").unwrap();
        let link = dir.path().join(ProfileFile::Candidate.relative_path());
        std::os::unix::fs::symlink(&real, &link).unwrap();
        files.write(ProfileFile::Candidate, "new").unwrap();
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_nothing_or_a_linked_folder_outside_is_refused() {
        let (dir, files) = files();
        let outside = tempfile::tempdir().unwrap();
        let link = dir.path().join(ProfileFile::Candidate.relative_path());
        std::os::unix::fs::symlink(outside.path().join("gone.md"), &link).unwrap();
        assert_eq!(files.write(ProfileFile::Candidate, "new").unwrap_err().reason, "links to a file that is not there, so it was not saved");
        assert!(!outside.path().join("gone.md").exists());
        // The folder of a file that does not exist yet is a link out of the root.
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".claude/skills/job-scraper")).unwrap();
        assert!(files.write(ProfileFile::SearchQueries, "new").unwrap_err().reason.contains("outside"));
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
