//! The lock of a job search folder, `.jobdesk/.open`. Every Jobdesk that has the folder open holds it shared, from the
//! open until another folder opens or Jobdesk quits, and an update, an Undo or a rollback holds it alone. So none of
//! them runs while another Jobdesk has the folder open, whose tasks, Profile's Save and portal changes may write the
//! folder at any moment, and two of them never run on one folder at once.
//!
//! A folder with no `.jobdesk` has no base, so nothing updates it, and no `.jobdesk` is ever made for the lock.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::Path;

use crate::folder_update::domain::UpdateError;
use crate::folder_update::infrastructure::disk::is_real_dir;
use crate::shared_kernel::installed_source;

/// The lock file, in `.jobdesk`.
pub const OPEN: &str = ".open";

/// What trying to hold a folder open found.
#[derive(Debug)]
pub enum Open {
    /// This Jobdesk holds the folder open, shared with every other Jobdesk that has it open, for as long as the file
    /// stays open.
    Held(File),
    /// Another Jobdesk holds the folder alone: it updates, undoes or rolls it back now.
    Elsewhere,
    /// Nothing can be held: the folder's `.jobdesk` is not a real folder, or the lock file is a link, not a file or does
    /// not open.
    Nothing,
}

/// Holds `folder` open, shared with every other Jobdesk that has it open.
pub fn hold_open(folder: &Path) -> Open {
    let Ok(file) = open(&folder.join(installed_source::DIR)) else { return Open::Nothing };
    match file.try_lock_shared() {
        Ok(()) => Open::Held(file),
        Err(TryLockError::WouldBlock) => Open::Elsewhere,
        Err(TryLockError::Error(_)) => Open::Nothing,
    }
}

/// Holds the folder of `jobdesk` alone, for as long as the returned file stays open. `Busy` while any Jobdesk holds it,
/// open or alone, this one included: let go of this Jobdesk's own hold first.
pub fn hold_alone(jobdesk: &Path) -> Result<File, UpdateError> {
    let file = open(jobdesk).map_err(|_| UpdateError::Disk)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(UpdateError::Busy),
        Err(TryLockError::Error(_)) => Err(UpdateError::Disk),
    }
}

/// The lock file of `jobdesk`, made when missing and readable only by the user. A `.jobdesk` that is not a real folder,
/// or a lock file that is a link or anything but a file, is refused.
fn open(jobdesk: &Path) -> io::Result<File> {
    if !is_real_dir(jobdesk) {
        return Err(io::Error::other("no .jobdesk folder"));
    }
    let path = jobdesk.join(OPEN);
    if fs::symlink_metadata(&path).is_ok_and(|meta| !meta.is_file()) {
        return Err(io::Error::other("not a plain file"));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> tempfile::TempDir {
        let folder = tempfile::tempdir().unwrap();
        fs::create_dir(folder.path().join(installed_source::DIR)).unwrap();
        folder
    }

    #[test]
    fn any_number_of_jobdesks_hold_a_folder_open_and_none_holds_it_alone_meanwhile() {
        let folder = folder();
        let jobdesk = folder.path().join(installed_source::DIR);
        let Open::Held(first) = hold_open(folder.path()) else { panic!("not held") };
        let Open::Held(second) = hold_open(folder.path()) else { panic!("a second Jobdesk opens it too") };
        assert_eq!(hold_alone(&jobdesk).map(|_| ()), Err(UpdateError::Busy));
        drop(first);
        assert_eq!(hold_alone(&jobdesk).map(|_| ()), Err(UpdateError::Busy), "while one still has it open");
        drop(second);
        let alone = hold_alone(&jobdesk).unwrap();
        assert!(matches!(hold_open(folder.path()), Open::Elsewhere), "no Jobdesk holds it open while one holds it alone");
        assert_eq!(hold_alone(&jobdesk).map(|_| ()), Err(UpdateError::Busy));
        drop(alone);
        assert!(matches!(hold_open(folder.path()), Open::Held(_)));
    }

    #[test]
    fn a_folder_with_no_jobdesk_gets_none() {
        let folder = tempfile::tempdir().unwrap();
        assert!(matches!(hold_open(folder.path()), Open::Nothing));
        assert!(!folder.path().join(installed_source::DIR).exists(), "no .jobdesk is made for the lock");
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_lock_file_is_never_followed() {
        let folder = folder();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path().join("elsewhere"), folder.path().join(installed_source::DIR).join(OPEN)).unwrap();
        assert!(matches!(hold_open(folder.path()), Open::Nothing));
        assert_eq!(hold_alone(&folder.path().join(installed_source::DIR)).map(|_| ()), Err(UpdateError::Disk));
        assert!(!outside.path().join("elsewhere").exists());
    }
}
