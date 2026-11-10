use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use crate::profile::domain::{ProfileFile, ProfileStore};
use crate::shared_kernel::LoadError;
use crate::shared_kernel::safe_save::{Refusals, refusal, replace_through_copy};

/// The profile files of one checkout. A save of one block is the only write the profile makes to the checkout.
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
        if let Some(why) = fs::metadata(&target).ok().and_then(|metadata| refusal(&target, &metadata, &REFUSALS)) {
            return Err(LoadError::new(file.name(), why));
        }
        Ok(target)
    }
}

#[cfg(unix)]
const CANNOT_WRITE: &str = "cannot be written by your user, so it was not saved";
#[cfg(not(unix))]
const CANNOT_WRITE: &str = "cannot be written, or is open in another program, so it was not saved";

/// Why a save refuses a file it may not replace. A file gone between the checks is saved anew.
const REFUSALS: Refusals = Refusals {
    read_only: "is read-only, so it was not saved",
    not_yours: "belongs to another user, so it was not saved",
    cannot_write: CANNOT_WRITE,
    gone: None,
};

/// The folder of `file` from the root, as the user would look for it.
fn folder_of(file: ProfileFile) -> &'static str {
    let path = file.relative_path();
    path.rsplit_once('/').map_or(".", |(folder, _)| folder)
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

    /// The save compared the block once, so the write never waits for a file another program holds.
    fn write(&self, file: ProfileFile, text: &str) -> Result<(), LoadError> {
        let target = self.target(file)?;
        replace_through_copy(&target, text).map_err(|error| LoadError::new(file.name(), error))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    #[cfg(windows)]
    use std::fs::OpenOptions;

    use super::*;
    #[cfg(unix)]
    use crate::shared_kernel::safe_save::current_user;

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

    /// The names in `folder`, sorted.
    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(folder).unwrap().flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    /// Another save's copy at a peer process's name could be renamed over the file at any moment, so a save never removes
    /// or reuses a fresh one: it writes a copy of its own. A file at the near-miss name without a number stays too.
    #[test]
    fn copies_of_other_saves_are_left_alone() {
        let (dir, files) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        let earlier = path.with_file_name("01-candidate-profile.md.jobdesk-save");
        fs::write(&earlier, "earlier save").unwrap();
        let peer = path.with_file_name(format!("01-candidate-profile.md.jobdesk-save-{}-0", std::process::id().wrapping_add(1)));
        fs::write(&peer, "peer save").unwrap();
        assert_eq!(files.write(ProfileFile::Candidate, "new"), Ok(()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(fs::read_to_string(&earlier).unwrap(), "earlier save");
        assert_eq!(fs::read_to_string(&peer).unwrap(), "peer save");
        let peer_name = peer.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(names(path.parent().unwrap()), ["01-candidate-profile.md", "01-candidate-profile.md.jobdesk-save", peer_name.as_str()]);
    }

    /// A link at the near-miss name without a number is neither followed nor removed.
    #[cfg(unix)]
    #[test]
    fn a_link_at_a_near_miss_copy_name_is_not_followed() {
        let (dir, files) = files();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("notes.md"), "outside").unwrap();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        let earlier = path.with_file_name("01-candidate-profile.md.jobdesk-save");
        std::os::unix::fs::symlink(outside.path().join("notes.md"), &earlier).unwrap();
        assert_eq!(files.write(ProfileFile::Candidate, "new"), Ok(()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(fs::symlink_metadata(&earlier).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(outside.path().join("notes.md")).unwrap(), "outside");
    }

    /// Sets the time `path` was last changed to `ago` before now.
    fn last_changed(path: &Path, ago: Duration) {
        fs::File::options().write(true).open(path).unwrap().set_modified(SystemTime::now() - ago).unwrap();
    }

    /// A copy an interrupted save left holds the profile's text, so a later save removes it after an hour. A fresh copy
    /// may be a live save's, and a name that only looks like a copy's, or a copy of another file, is not one.
    #[test]
    fn copies_left_over_an_hour_ago_are_removed() {
        let (dir, files) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        let peer = std::process::id().wrapping_add(1);
        let copy = |tail: String| path.with_file_name(format!("01-candidate-profile.md.jobdesk-save{tail}"));
        let interrupted = copy(format!("-{peer}-3"));
        let fresh = copy(format!("-{peer}-4"));
        let kept = [
            copy(String::new()),
            copy(format!("-{peer}-x")),
            copy(format!("-{peer}-3.bak")),
            copy("-+1-3".to_owned()),
            copy(format!("-{peer}-")),
            path.with_file_name(format!("02-behavioral-profile.md.jobdesk-save-{peer}-3")),
        ];
        for (file, ago) in [(&interrupted, 2 * 60 * 60), (&fresh, 30 * 60)].into_iter().chain(kept.iter().map(|file| (file, 2 * 60 * 60))) {
            fs::write(file, "text").unwrap();
            last_changed(file, Duration::from_secs(ago));
        }
        assert_eq!(files.write(ProfileFile::Candidate, "new"), Ok(()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(!interrupted.exists());
        assert!(fresh.exists());
        for file in &kept {
            assert!(file.exists(), "{} was removed", file.display());
        }
    }

    /// A save whose rename fails removes its own copy, and only that.
    #[test]
    fn a_failed_save_leaves_no_copy_of_its_own() {
        let (dir, files) = files();
        // A folder where the file should be: the rename fails, and the copy goes.
        let folder = dir.path().join(ProfileFile::Behavioral.relative_path());
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("inside"), "x").unwrap();
        let skills = folder.parent().unwrap();
        let peer = skills.join(format!("02-behavioral-profile.md.jobdesk-save-{}-0", std::process::id().wrapping_add(1)));
        fs::write(&peer, "peer save").unwrap();
        let before = names(skills);
        assert!(files.write(ProfileFile::Behavioral, "new").is_err());
        assert_eq!(names(skills), before);
        assert_eq!(fs::read_to_string(&peer).unwrap(), "peer save");
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

    /// A file gone between the checks is not a refusal: the save creates it.
    #[test]
    fn a_file_gone_before_the_write_is_saved() {
        let (dir, _) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        let metadata = fs::metadata(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(refusal(&path, &metadata, &REFUSALS), None);
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

    /// Another program holds the file with no sharing, as a sync client or an editor can, so the save is refused.
    #[cfg(windows)]
    #[test]
    fn a_file_another_program_holds_open_is_not_saved() {
        use std::os::windows::fs::OpenOptionsExt;
        let (dir, files) = files();
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        let held = OpenOptions::new().read(true).share_mode(0).open(&path).unwrap();
        assert_eq!(files.write(ProfileFile::Candidate, "new").unwrap_err().reason, CANNOT_WRITE);
        drop(held);
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
