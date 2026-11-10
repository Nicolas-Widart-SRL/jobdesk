//! Replacing a text file in the user's checkout through a copy beside it, so a crash leaves the old file or the new one,
//! and the checks before it that refuse a file the user may not replace. A profile save and a portal switch both write
//! this way.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Added to a file's name, before the process id and a number, for the copy a save writes before it replaces the file.
const SAVE_SUFFIX: &str = ".jobdesk-save";

/// Writes a copy beside `target`, then renames it over `target`, so a crash leaves the old file or the new one, never half.
/// This save's copy is removed when anything fails. Of the other copies, only those `remove_stale_copies` finds go.
///
/// The caller has read the file and built `text` from what it found, so the rename never waits for a file another
/// program holds: a program that writes it during a wait would be overwritten unseen. A held file fails at once.
pub fn replace_through_copy(target: &Path, text: &str) -> io::Result<()> {
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
    #[cfg(not(unix))]
    let _ = folder;
    Ok(())
}

/// What a caller says for each file `refusal` refuses, in its own words.
pub struct Refusals {
    pub read_only: &'static str,
    /// The file belongs to another user. Unix only.
    pub not_yours: &'static str,
    pub cannot_write: &'static str,
    /// The file went between the caller's read and the check. `None` lets the save go on and create it again.
    pub gone: Option<&'static str>,
}

/// Why the file at `target`, with this `metadata`, may not be replaced, in the caller's `words`. The rename needs only
/// the folder, so it would quietly pass over a read-only file, a file another user owns, which would change hands, or a
/// file the user cannot write. A file gone by now is refused only when the caller has words for it.
pub fn refusal(target: &Path, metadata: &fs::Metadata, words: &Refusals) -> Option<&'static str> {
    if metadata.permissions().readonly() {
        return Some(words.read_only);
    }
    #[cfg(unix)]
    if owned_by_other(metadata, current_user()) {
        return Some(words.not_yours);
    }
    match write_probe(target, metadata) {
        Ok(()) => None,
        Err(error) if error.kind() == ErrorKind::NotFound => words.gone,
        Err(_) => Some(words.cannot_write),
    }
}

#[cfg(unix)]
pub(crate) fn current_user() -> libc::uid_t {
    // SAFETY: geteuid takes nothing and always succeeds.
    unsafe { libc::geteuid() }
}

/// Whether a file with this `metadata` belongs to someone other than `user`. The rename puts a new file in its place,
/// owned by whoever saves, so the file would quietly change hands.
#[cfg(unix)]
fn owned_by_other(metadata: &fs::Metadata, user: libc::uid_t) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.uid() != user
}

/// Whether the file at `target` can be written in place, which the rename would not notice, because it needs only the
/// folder. The mode bits or an access list can deny a write that the read-only flag does not show, so the file is opened
/// for writing, which changes nothing. On Windows an open that another program blocks fails the same way. A file gone by
/// now fails with `NotFound`.
fn write_probe(target: &Path, metadata: &fs::Metadata) -> io::Result<()> {
    if metadata.is_file() {
        OpenOptions::new().write(true).open(target)?;
    }
    Ok(())
}

/// How long a copy must sit unchanged before a later save takes it for one an interrupted save left. A save writes and
/// renames its copy in well under a second.
const STALE_COPY: Duration = Duration::from_secs(60 * 60);

/// Removes the copies of `target` that a save interrupted before its rename left, by power loss or a force quit: they
/// hold the file's text, and a `.gitignore` line that names the file does not match them. Only regular files named
/// exactly `<name>.jobdesk-save-<number>-<number>` and last changed over `STALE_COPY` before `now` go. A link is never
/// removed. Best effort: a copy that cannot be removed waits for the next save. A live save whose copy went this way
/// fails its rename and says so, so no text is lost unseen.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder holding `notes.md`, the file a save replaces.
    fn notes() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.md");
        fs::write(&target, "old").unwrap();
        (dir, target)
    }

    /// The names in `folder`, sorted.
    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(folder).unwrap().flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    /// Numbers 0, 1, 2 and on, so a test knows the names a save tries.
    fn from_zero() -> impl FnMut() -> u64 {
        let mut number = 0;
        move || {
            number += 1;
            number - 1
        }
    }

    #[test]
    fn a_save_replaces_the_file_and_leaves_no_copy() {
        let (dir, target) = notes();
        replace_through_copy(&target, "new").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(names(dir.path()), ["notes.md"]);
    }

    /// A name taken in this process, such as one a crash left under a reused process id, is skipped for the next.
    #[test]
    fn a_taken_copy_name_is_skipped() {
        let (dir, target) = notes();
        let taken = dir.path().join(format!("notes.md.jobdesk-save-{}-0", std::process::id()));
        fs::write(&taken, "another save").unwrap();
        let copy = write_copy(&target, from_zero(), |file| file.write_all(b"new")).unwrap();
        assert_eq!(copy, dir.path().join(format!("notes.md.jobdesk-save-{}-1", std::process::id())));
        assert_eq!(fs::read_to_string(&copy).unwrap(), "new");
        assert_eq!(fs::read_to_string(&taken).unwrap(), "another save");
    }

    /// A link at this process's first name, to nothing or to a file outside the folder, is skipped for the next number
    /// and never written through.
    #[cfg(unix)]
    #[test]
    fn a_link_at_a_copy_name_is_skipped_and_not_written_through() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("notes.md"), "outside").unwrap();
        let named = |target: &str, number: u64| dir.path().join(format!("{target}.jobdesk-save-{}-{number}", std::process::id()));
        for (target, to) in [("notes.md", "gone.md"), ("SKILL.md", "notes.md")] {
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

    /// A save that fails while it writes or renames removes its own copy, and only that.
    #[test]
    fn a_failed_save_leaves_no_copy_of_its_own() {
        let (dir, target) = notes();
        let peer = dir.path().join(format!("notes.md.jobdesk-save-{}-0", std::process::id().wrapping_add(1)));
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
        let folder = dir.path().join("SKILL.md");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("inside"), "x").unwrap();
        let before = names(dir.path());
        assert!(replace_through_copy(&folder, "new").is_err());
        assert_eq!(names(dir.path()), before);
        assert_eq!(fs::read_to_string(&peer).unwrap(), "peer save");
    }

    /// Sets the time `path` was last changed to `ago` before now.
    fn last_changed(path: &Path, ago: Duration) {
        fs::File::options().write(true).open(path).unwrap().set_modified(SystemTime::now() - ago).unwrap();
    }

    /// A link at a copy's name is never removed, however old the link is.
    #[cfg(unix)]
    #[test]
    fn an_old_link_at_a_copy_name_is_not_removed() {
        use std::os::unix::ffi::OsStrExt;
        let (dir, target) = notes();
        let outside = tempfile::tempdir().unwrap();
        let elsewhere = outside.path().join("elsewhere.md");
        fs::write(&elsewhere, "outside").unwrap();
        last_changed(&elsewhere, Duration::from_secs(2 * 60 * 60));
        let link = dir.path().join(format!("notes.md.jobdesk-save-{}-3", std::process::id().wrapping_add(1)));
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        let two_hours_ago = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() - 2 * 60 * 60;
        let time = libc::timeval { tv_sec: two_hours_ago as libc::time_t, tv_usec: 0 };
        let c_link = std::ffi::CString::new(link.as_os_str().as_bytes()).unwrap();
        // SAFETY: the path is a valid C string and the two times live through the call.
        assert_eq!(unsafe { libc::lutimes(c_link.as_ptr(), [time, time].as_ptr()) }, 0);
        let link_age = SystemTime::now().duration_since(fs::symlink_metadata(&link).unwrap().modified().unwrap()).unwrap();
        assert!(link_age > STALE_COPY);
        replace_through_copy(&target, "new").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "outside");
    }

    #[cfg(unix)]
    #[test]
    fn a_private_file_stays_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, target) = notes();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        replace_through_copy(&target, "new").unwrap();
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
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
    /// The rename would hand the file to the current user, so a file someone else owns is refused.
    #[cfg(unix)]
    #[test]
    fn a_file_owned_by_another_user_is_refused() {
        let (_dir, target) = notes();
        let metadata = fs::metadata(&target).unwrap();
        assert!(owned_by_other(&metadata, current_user().wrapping_add(1)));
        assert!(!owned_by_other(&metadata, current_user()));
    }

    /// A file gone by now is refused only when the caller has words for it.
    #[test]
    fn a_file_that_is_gone_is_refused_only_in_words_for_it() {
        let (_dir, target) = notes();
        let metadata = fs::metadata(&target).unwrap();
        fs::remove_file(&target).unwrap();
        let allowed = Refusals { read_only: "read-only here", not_yours: "not yours here", cannot_write: "not writable here", gone: None };
        assert_eq!(refusal(&target, &metadata, &allowed), None);
        let refused = Refusals { gone: Some("gone here"), ..allowed };
        assert_eq!(refusal(&target, &metadata, &refused), Some("gone here"));
    }

    #[test]
    fn a_refusal_is_in_the_callers_words() {
        let words = Refusals { read_only: "read-only here", not_yours: "not yours here", cannot_write: "not writable here", gone: None };
        let (_dir, target) = notes();
        assert_eq!(refusal(&target, &fs::metadata(&target).unwrap(), &words), None);
        let mut permissions = fs::metadata(&target).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&target, permissions).unwrap();
        assert_eq!(refusal(&target, &fs::metadata(&target).unwrap(), &words), Some("read-only here"));
    }

    #[cfg(unix)]
    #[test]
    fn the_copy_is_private_until_its_text_is_in() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, target) = notes();
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
}
