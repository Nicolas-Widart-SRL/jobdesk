use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use crate::profile::domain::{ProfileFile, ProfileStore};
use crate::shared_kernel::LoadError;

/// Added to a file's name, before the process id and a number, for the copy a save writes before it replaces the file.
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
/// This save's copy is removed when anything fails. Of the other copies, only those `remove_stale_copies` finds go.
fn replace(target: &Path, text: &str) -> io::Result<()> {
    let folder = target.parent().ok_or(ErrorKind::InvalidInput)?;
    remove_stale_copies(target, SystemTime::now());
    let copy = write_copy(target, next_copy, |file| file.write_all(text.as_bytes()))?;
    if let Err(error) = fs::rename(&copy, target) {
        let _ = fs::remove_file(&copy);
        return Err(error);
    }
    // The rename lives in the folder, which a crash could still lose. Windows cannot open a folder this way.
    // Best effort: some mounts refuse to sync a folder, and the file is already in place.
    #[cfg(unix)]
    let _ = fs::File::open(folder).and_then(|folder| folder.sync_all());
    Ok(())
}

/// How long a copy must sit unchanged before a later save takes it for one an interrupted save left. A save writes and
/// renames its copy in well under a second.
const STALE_COPY: Duration = Duration::from_secs(60 * 60);

/// Removes the copies of `target` that a save interrupted before its rename left, by power loss or a force quit: they
/// hold the profile's text, and a `.gitignore` line that names the profile file does not match them. Only regular files
/// named exactly `<name>.jobdesk-save-<number>-<number>` and last changed over `STALE_COPY` before `now` go. A link is
/// never removed. Best effort: a copy that cannot be removed waits for the next save. A live save whose copy went this
/// way fails its rename and says so, so no text is lost unseen.
fn remove_stale_copies(target: &Path, now: SystemTime) {
    let (Some(folder), Some(name)) = (target.parent(), target.file_name().and_then(|name| name.to_str())) else { return };
    let Ok(entries) = fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let copy_name = entry.file_name();
        if !copy_name.to_str().is_some_and(|copy_name| is_copy_name(copy_name, name)) {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else { continue };
        let stale = metadata.modified().ok().and_then(|modified| now.duration_since(modified).ok()).is_some_and(|age| age > STALE_COPY);
        if metadata.file_type().is_file() && stale {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Whether `copy_name` is `<name>.jobdesk-save-<process id>-<number>`, digits only, as `create_copy` names a copy.
fn is_copy_name(copy_name: &str, name: &str) -> bool {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    copy_name
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(SAVE_SUFFIX))
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.split_once('-'))
        .is_some_and(|(process, number)| digits(process) && digits(number))
}

/// Numbers the copies this process writes, so two saves at once never pick the same name.
static COPIES: AtomicU64 = AtomicU64::new(0);

fn next_copy() -> u64 {
    COPIES.fetch_add(1, Ordering::Relaxed)
}

/// Writes the copy of one save and returns its path. It keeps the permission bits of `target`, set only once `write`
/// has put the text in. Anything else tied to the old file is lost with it: its other hard links keep the old text, and
/// its ACLs and extended attributes are not copied. The copy is removed when anything here fails.
fn write_copy(target: &Path, numbers: impl FnMut() -> u64, write: impl FnOnce(&mut fs::File) -> io::Result<()>) -> io::Result<PathBuf> {
    let kept = fs::metadata(target).ok().map(|metadata| metadata.permissions());
    let (copy, mut file) = create_copy(target, kept.is_some(), numbers)?;
    let written = (|| {
        write(&mut file)?;
        if let Some(permissions) = kept {
            file.set_permissions(permissions)?;
        }
        // On disk before the rename, or a power loss could keep the rename and lose the text.
        file.sync_all()
    })();
    // Closed first, because Windows cannot remove a file that is open.
    drop(file);
    if let Err(error) = written {
        let _ = fs::remove_file(&copy);
        return Err(error);
    }
    Ok(copy)
}

/// How many names a save tries for its copy before it gives up.
const COPY_NAMES: usize = 100;

/// Creates the copy, empty, as `<name>.jobdesk-save-<process id>-<number>`, taking numbers from `numbers` until a name
/// is free. `create_new` never follows a link and never opens a file that is there, so a copy another save is writing,
/// or one a crash left, is never opened or reused.
///
/// When it `replaces` a file, on Unix only its owner may open the copy until the text is in, so the text of a private
/// file is never open to other users, not even while it is written. A new file gets the usual mode.
fn create_copy(target: &Path, replaces: bool, mut numbers: impl FnMut() -> u64) -> io::Result<(PathBuf, fs::File)> {
    let (Some(folder), Some(name)) = (target.parent(), target.file_name()) else { return Err(ErrorKind::InvalidInput.into()) };
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if replaces {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = replaces;
    for _ in 0..COPY_NAMES {
        let mut copy_name = name.to_owned();
        copy_name.push(format!("{SAVE_SUFFIX}-{}-{}", std::process::id(), numbers()));
        let copy = folder.join(copy_name);
        match options.open(&copy) {
            Ok(file) => return Ok((copy, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(ErrorKind::AlreadyExists, "every name for the save's copy was taken"))
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

    /// A name taken in this process, such as one a crash left under a reused process id, is skipped for the next.
    #[test]
    fn a_taken_copy_name_is_skipped() {
        let (dir, _) = files();
        let target = dir.path().join("01-candidate-profile.md");
        fs::write(&target, "old").unwrap();
        let taken = dir.path().join(format!("01-candidate-profile.md.jobdesk-save-{}-0", std::process::id()));
        fs::write(&taken, "another save").unwrap();
        let copy = write_copy(&target, from_zero(), |file| file.write_all(b"new")).unwrap();
        assert_eq!(copy, dir.path().join(format!("01-candidate-profile.md.jobdesk-save-{}-1", std::process::id())));
        assert_eq!(fs::read_to_string(&copy).unwrap(), "new");
        assert_eq!(fs::read_to_string(&taken).unwrap(), "another save");
    }

    /// Numbers 0, 1, 2 and on, so a test knows the names a save tries.
    fn from_zero() -> impl FnMut() -> u64 {
        let mut number = 0;
        move || {
            number += 1;
            number - 1
        }
    }

    /// A link at this process's first name, to nothing or to a file outside the root, is skipped for the next number
    /// and never written through.
    #[cfg(unix)]
    #[test]
    fn a_link_at_a_copy_name_is_skipped_and_not_written_through() {
        let (dir, _) = files();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("notes.md"), "outside").unwrap();
        let named = |target: &str, number: u64| dir.path().join(format!("{target}.jobdesk-save-{}-{number}", std::process::id()));
        for (target, to) in [("01-candidate-profile.md", "gone.md"), ("02-behavioral-profile.md", "notes.md")] {
            fs::write(dir.path().join(target), "old").unwrap();
            std::os::unix::fs::symlink(outside.path().join(to), named(target, 0)).unwrap();
            let copy = write_copy(&dir.path().join(target), from_zero(), |file| file.write_all(b"new")).unwrap();
            assert_eq!(copy, named(target, 1));
            assert_eq!(fs::read_to_string(&copy).unwrap(), "new");
            assert!(fs::symlink_metadata(named(target, 0)).unwrap().file_type().is_symlink());
        }
        assert!(!outside.path().join("gone.md").exists());
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

    /// A link at a copy's name is never removed, however old the link is.
    #[cfg(unix)]
    #[test]
    fn an_old_link_at_a_copy_name_is_not_removed() {
        use std::os::unix::ffi::OsStrExt;
        let (dir, files) = files();
        let outside = tempfile::tempdir().unwrap();
        let notes = outside.path().join("notes.md");
        fs::write(&notes, "outside").unwrap();
        last_changed(&notes, Duration::from_secs(2 * 60 * 60));
        let path = dir.path().join(ProfileFile::Candidate.relative_path());
        fs::write(&path, "old").unwrap();
        let link = path.with_file_name(format!("01-candidate-profile.md.jobdesk-save-{}-3", std::process::id().wrapping_add(1)));
        std::os::unix::fs::symlink(&notes, &link).unwrap();
        let two_hours_ago = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() - 2 * 60 * 60;
        let time = libc::timeval { tv_sec: two_hours_ago as libc::time_t, tv_usec: 0 };
        let c_link = std::ffi::CString::new(link.as_os_str().as_bytes()).unwrap();
        // SAFETY: the path is a valid C string and the two times live through the call.
        assert_eq!(unsafe { libc::lutimes(c_link.as_ptr(), [time, time].as_ptr()) }, 0);
        let link_age = SystemTime::now().duration_since(fs::symlink_metadata(&link).unwrap().modified().unwrap()).unwrap();
        assert!(link_age > STALE_COPY);
        assert_eq!(files.write(ProfileFile::Candidate, "new"), Ok(()));
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&notes).unwrap(), "outside");
    }

    /// A save that fails while it writes or renames removes its own copy, and only that.
    #[test]
    fn a_failed_save_leaves_no_copy_of_its_own() {
        let (dir, files) = files();
        let target = dir.path().join("01-candidate-profile.md");
        fs::write(&target, "old").unwrap();
        let peer = dir.path().join(format!("01-candidate-profile.md.jobdesk-save-{}-0", std::process::id().wrapping_add(1)));
        fs::write(&peer, "peer save").unwrap();
        let before = names(dir.path());
        let failed = write_copy(&target, next_copy, |file| {
            file.write_all(b"half")?;
            Err(io::Error::other("disk full"))
        });
        assert_eq!(failed.unwrap_err().to_string(), "disk full");
        assert_eq!(names(dir.path()), before);
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
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
        let mut while_written = Vec::new();
        let copy = {
            // SAFETY: umask takes a mask and always succeeds.
            let _umask = Umask(unsafe { libc::umask(0o022) });
            write_copy(&target, next_copy, |file| {
                while_written.push(file.metadata()?.permissions().mode() & 0o777);
                file.write_all(b"new")?;
                while_written.push(file.metadata()?.permissions().mode() & 0o777);
                Ok(())
            })
            .unwrap()
        };
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
