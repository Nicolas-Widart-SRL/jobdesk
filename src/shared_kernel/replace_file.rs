use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

/// How many times a replace tries on Windows before it gives up. Saves run on the UI thread, so the wait stays short.
const ATTEMPTS: u32 = 5;
/// How long a replace waits between two tries.
const PAUSE: Duration = Duration::from_millis(40);

/// Renames `from` over `to`. On Windows the rename fails while another program holds `to` open, such as an antivirus
/// scan, the search indexer, a sync client or an editor, and the hold is usually gone within a moment, so a refusal for
/// that reason is tried again, up to 5 times 40 ms apart. Elsewhere the first error is the answer.
///
/// Only for files whose last writer wins anyway. A file that is compared before it is replaced must not wait here, or a
/// program that writes it during the wait would be overwritten unseen.
pub fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    let held = |error: &io::Error| cfg!(windows) && is_transient(error.raw_os_error(), is_writable_file(to));
    retried(|| std::fs::rename(from, to), held, ATTEMPTS, PAUSE)
}

/// Whether `path` is an existing file that its read-only attribute does not lock.
fn is_writable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && !metadata.permissions().readonly())
}

/// Whether a Windows error from a rename names a file another program holds, which clears in a moment. A sharing
/// violation (32) and a lock violation (33) do. Access denied (5) does only while the target is a writable file: a
/// read-only attribute, an access list that denies the write and a folder in the target's place never clear.
fn is_transient(code: Option<i32>, target_is_writable_file: bool) -> bool {
    match code {
        Some(32 | 33) => true,
        Some(5) => target_is_writable_file,
        _ => false,
    }
}

/// Runs `op` until it succeeds, fails with an error `is_transient` does not name, or has run `attempts` times, waiting
/// `pause` between two runs. Returns the last result.
fn retried(mut op: impl FnMut() -> io::Result<()>, is_transient: impl Fn(&io::Error) -> bool, attempts: u32, pause: Duration) -> io::Result<()> {
    let mut tried = 1;
    loop {
        match op() {
            Err(error) if tried < attempts && is_transient(&error) => {
                tried += 1;
                thread::sleep(pause);
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use super::*;

    fn busy() -> io::Error {
        io::Error::new(ErrorKind::PermissionDenied, "busy")
    }

    fn is_busy(error: &io::Error) -> bool {
        error.kind() == ErrorKind::PermissionDenied
    }

    #[test]
    fn a_transient_error_is_tried_again_until_it_clears() {
        let mut calls = 0;
        let result = retried(
            || {
                calls += 1;
                if calls <= 2 { Err(busy()) } else { Ok(()) }
            },
            is_busy,
            10,
            Duration::ZERO,
        );
        assert!(result.is_ok());
        assert_eq!(calls, 3);
    }

    #[test]
    fn an_error_that_is_not_transient_gives_up_at_once() {
        let mut calls = 0;
        let result = retried(
            || {
                calls += 1;
                Err(io::Error::new(ErrorKind::NotFound, "gone"))
            },
            is_busy,
            10,
            Duration::ZERO,
        );
        assert_eq!(result.unwrap_err().kind(), ErrorKind::NotFound);
        assert_eq!(calls, 1);
    }

    #[test]
    fn a_transient_error_stops_at_the_attempt_limit() {
        let mut calls = 0;
        let result = retried(
            || {
                calls += 1;
                Err(busy())
            },
            is_busy,
            4,
            Duration::ZERO,
        );
        assert_eq!(result.unwrap_err().kind(), ErrorKind::PermissionDenied);
        assert_eq!(calls, 4);
    }

    #[test]
    fn a_held_file_is_transient_and_a_refusal_that_never_clears_is_not() {
        assert!(is_transient(Some(32), false));
        assert!(is_transient(Some(33), false));
        assert!(is_transient(Some(5), true));
        assert!(!is_transient(Some(5), false));
        assert!(!is_transient(Some(2), true));
        assert!(!is_transient(Some(183), true));
        assert!(!is_transient(None, true));
    }

    #[test]
    fn only_an_existing_file_without_the_read_only_attribute_is_writable() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, "text").unwrap();
        assert!(is_writable_file(&file));
        assert!(!is_writable_file(dir.path()));
        assert!(!is_writable_file(&dir.path().join("missing")));
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions).unwrap();
        assert!(!is_writable_file(&file));
    }

    #[test]
    fn a_file_is_renamed_over_another() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("new"), dir.path().join("old"));
        std::fs::write(&from, "new").unwrap();
        std::fs::write(&to, "old").unwrap();
        rename_over(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "new");
        assert!(!from.exists());
    }
}
