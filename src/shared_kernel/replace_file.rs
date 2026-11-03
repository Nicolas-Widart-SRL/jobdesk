use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

/// How many times a replace tries on Windows before it gives up.
const ATTEMPTS: u32 = 10;
/// How long a replace waits between two tries.
const PAUSE: Duration = Duration::from_millis(50);

/// Renames `from` over `to`. On Windows the rename fails while another program holds `to` open, such as an antivirus
/// scan, the search indexer, a sync client or an editor, and the hold is usually gone within a moment, so a refusal for
/// that reason is tried again, up to 10 times 50 ms apart. Elsewhere the first error is the answer.
pub fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    retried(|| std::fs::rename(from, to), is_transient, ATTEMPTS, PAUSE)
}

/// Windows errors that name a file held by another program: access denied, a sharing violation and a lock violation.
#[cfg(windows)]
fn is_transient(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(not(windows))]
fn is_transient(_: &io::Error) -> bool {
    false
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
