//! A folder of the data folder that each running Jobdesk owns, `<dir>/<pid>`, so one window never deletes what another
//! is still writing. `updates` downloads into its own, and `onboarding` downloads the `ai-job-search` release into its own.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Mutex, PoisonError};

/// The file each running Jobdesk holds locked in its own folder, so another one leaves that folder alone.
pub const LOCK: &str = ".lock";

/// The folders this process has claimed, each with its open lock. Nothing is ever taken out, so each lock holds until
/// the process exits, through Restart to update's `finish` after the window has closed.
static CLAIMED: Mutex<Vec<(PathBuf, File)>> = Mutex::new(Vec::new());

/// This process's own folder in `dir`, `<dir>/<pid>`, locked until the process exits. Claiming the same folder again
/// gives it back as it is.
pub fn claim(dir: &Path) -> io::Result<PathBuf> {
    let own = dir.join(process::id().to_string());
    let mut claimed = CLAIMED.lock().unwrap_or_else(PoisonError::into_inner);
    if !claimed.iter().any(|(folder, _)| *folder == own) {
        let lock = lock_folder(&own)?;
        claimed.push((own.clone(), lock));
    }
    Ok(own)
}

/// `claim`, but a folder this process claimed whose lock file has gone since is made and locked again. Another Jobdesk's
/// cleanup can delete the folder in the moment between its making and its lock, and `claim` would keep giving back the
/// deleted path.
pub fn reclaim(dir: &Path) -> io::Result<PathBuf> {
    let own = dir.join(process::id().to_string());
    let mut claimed = CLAIMED.lock().unwrap_or_else(PoisonError::into_inner);
    if fs::symlink_metadata(own.join(LOCK)).is_ok_and(|meta| meta.is_file()) && claimed.iter().any(|(folder, _)| *folder == own) {
        return Ok(own);
    }
    // The old lock, on a file that has gone, keeps nothing out.
    claimed.retain(|(folder, _)| *folder != own);
    let lock = lock_folder(&own)?;
    claimed.push((own.clone(), lock));
    Ok(own)
}

/// Makes `folder` and locks it for as long as the returned file stays open.
pub fn lock_folder(folder: &Path) -> io::Result<File> {
    private_dir(folder)?;
    let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(folder.join(LOCK))?;
    lock.try_lock().map_err(io::Error::from)?;
    Ok(lock)
}

/// Whether a Jobdesk that runs now holds `folder`.
pub fn in_use(folder: &Path) -> bool {
    let lock = folder.join(LOCK);
    if !fs::symlink_metadata(&lock).is_ok_and(|meta| meta.is_file()) {
        return false;
    }
    File::open(&lock).is_ok_and(|file| matches!(file.try_lock(), Err(TryLockError::WouldBlock)))
}

/// Empties `dir`, but for the entries named in `keep` and the folders of Jobdesk windows that still run. Errors are
/// ignored: what stays is tried again at the next launch. A link is removed and never followed, and so is `dir` itself
/// when it is one.
pub fn clear_unclaimed(dir: &Path, keep: &[&str]) {
    let Ok(meta) = fs::symlink_metadata(dir) else { return };
    if !meta.is_dir() {
        let _ = fs::remove_file(dir);
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if keep.iter().any(|name| entry.file_name() == *name) {
            continue;
        }
        let path = entry.path();
        // The type of the entry itself, not of what a link points to.
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {
                if !in_use(&path) {
                    let _ = fs::remove_dir_all(&path);
                }
            }
            Ok(_) => {
                let _ = fs::remove_file(&path);
            }
            Err(_) => {}
        }
    }
}

/// `dir` and its parents, readable only by the user, as the run logs are.
pub fn private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

/// A new file only the user can read. Whatever had the name before goes first, and a link there is removed, not followed.
pub fn new_private_file(path: &Path) -> io::Result<File> {
    let _ = fs::remove_file(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}
