use std::fs;
use std::io::Write;
use std::path::Path;

use crate::updates::domain::{ApplyFailed, ApplyFailure, Version};
use crate::updates::infrastructure::download::new_private_file;

/// The file in `updates/` that says why the last Restart to update did not apply. It outlives `clear` until it is read.
pub const APPLY_FAILED: &str = "apply-failed";

/// Empties `dir`, the downloads folder, but for the note of a failed apply. Errors are ignored: what stays is tried
/// again at the next launch. A link is removed and never followed, and so is `dir` itself when it is one.
pub fn clear(dir: &Path) {
    let Ok(meta) = fs::symlink_metadata(dir) else { return };
    if !meta.is_dir() {
        let _ = fs::remove_file(dir);
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if entry.file_name() == APPLY_FAILED {
            continue;
        }
        let path = entry.path();
        // The type of the entry itself, not of what a link points to.
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {
                let _ = fs::remove_dir_all(&path);
            }
            Ok(_) => {
                let _ = fs::remove_file(&path);
            }
            Err(_) => {}
        }
    }
}

/// Notes in `dir` that the release did not apply, for the next launch to say. Only a plain version and a word are written.
pub fn record_apply_failure(dir: &Path, failed: ApplyFailed) {
    if let Ok(mut file) = new_private_file(&dir.join(APPLY_FAILED)) {
        let _ = writeln!(file, "{} {}", failed.version, word(failed.failure));
    }
}

/// Why the last Restart to update did not apply, once: the note is deleted as it is read.
/// A note that is not a plain file, or that cannot be read, is deleted and gives `None`.
pub fn take_apply_failure(dir: &Path) -> Option<ApplyFailed> {
    let path = dir.join(APPLY_FAILED);
    let meta = fs::symlink_metadata(&path).ok()?;
    let text = if meta.is_file() && meta.len() <= 64 { fs::read_to_string(&path).ok() } else { None };
    let _ = if meta.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
    let (version, failure) = text?.trim().split_once(' ').map(|(version, failure)| (Version::parse(version), from_word(failure)))?;
    Some(ApplyFailed { version: version?, failure: failure? })
}

fn word(failure: ApplyFailure) -> &'static str {
    match failure {
        ApplyFailure::BadSignature => "signature",
        ApplyFailure::NotInstalled => "not-installed",
        ApplyFailure::Replace => "replace",
        ApplyFailure::Start => "start",
    }
}

fn from_word(word: &str) -> Option<ApplyFailure> {
    [ApplyFailure::BadSignature, ApplyFailure::NotInstalled, ApplyFailure::Replace, ApplyFailure::Start]
        .into_iter()
        .find(|failure| self::word(*failure) == word)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(failure: ApplyFailure) -> ApplyFailed {
        ApplyFailed { version: Version { major: 0, minor: 2, patch: 0 }, failure }
    }

    #[test]
    fn clear_deletes_only_inside_the_downloads_folder() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("updates");
        fs::create_dir_all(dir.join(".Jobdesk.app.update-1/Jobdesk.app")).unwrap();
        fs::write(dir.join("1-Jobdesk-v0.2.0-windows-x64-setup.exe"), "build").unwrap();
        fs::write(dir.join("2-Jobdesk-v0.2.0-windows-x64-setup.exe.part"), "part").unwrap();
        fs::write(data.path().join("settings.json"), "{}").unwrap();
        fs::create_dir(data.path().join("runs")).unwrap();
        fs::write(data.path().join("runs/r.jsonl"), "log").unwrap();
        clear(&dir);
        assert!(dir.is_dir(), "the folder itself stays");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        assert!(data.path().join("settings.json").exists() && data.path().join("runs/r.jsonl").exists());
    }

    #[cfg(unix)]
    #[test]
    fn clear_never_follows_a_link() {
        use std::os::unix::fs::symlink;
        let data = tempfile::tempdir().unwrap();
        let elsewhere = data.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::write(elsewhere.join("keep.txt"), "keep").unwrap();
        let dir = data.path().join("updates");
        fs::create_dir(&dir).unwrap();
        symlink(&elsewhere, dir.join("into-elsewhere")).unwrap();
        symlink(elsewhere.join("keep.txt"), dir.join("to-a-file")).unwrap();
        clear(&dir);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "the links themselves go");
        assert_eq!(fs::read_to_string(elsewhere.join("keep.txt")).unwrap(), "keep");

        let linked = data.path().join("linked-updates");
        symlink(&elsewhere, &linked).unwrap();
        clear(&linked);
        assert!(fs::symlink_metadata(&linked).is_err(), "a downloads folder that is a link is removed as a link");
        assert_eq!(fs::read_to_string(elsewhere.join("keep.txt")).unwrap(), "keep");
    }

    #[test]
    fn clear_on_a_missing_folder_does_nothing() {
        let data = tempfile::tempdir().unwrap();
        clear(&data.path().join("updates"));
        assert!(!data.path().join("updates").exists());
    }

    #[test]
    fn a_failed_apply_survives_clear_and_is_read_once() {
        let data = tempfile::tempdir().unwrap();
        let dir = data.path().join("updates");
        fs::create_dir(&dir).unwrap();
        for failure in [ApplyFailure::BadSignature, ApplyFailure::NotInstalled, ApplyFailure::Replace, ApplyFailure::Start] {
            record_apply_failure(&dir, failed(failure));
            clear(&dir);
            assert_eq!(take_apply_failure(&dir), Some(failed(failure)));
            assert_eq!(take_apply_failure(&dir), None);
        }
    }

    #[test]
    fn a_note_that_cannot_be_read_is_dropped() {
        let data = tempfile::tempdir().unwrap();
        for text in ["", "0.2.0", "0.2 replace", "0.2.0 melted", &"x".repeat(100)] {
            fs::write(data.path().join(APPLY_FAILED), text).unwrap();
            assert_eq!(take_apply_failure(data.path()), None, "{text:?}");
            assert!(!data.path().join(APPLY_FAILED).exists());
        }
        assert_eq!(take_apply_failure(&data.path().join("missing")), None);
    }
}
