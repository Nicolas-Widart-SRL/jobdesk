use std::fs;
use std::path::Path;

use crate::settings::domain::DataSizes;

/// How much the app keeps in its data folder: the run logs in `runs/` and the tray's `intray/decisions.json`.
/// Only regular files count, so a link never adds the size of what it points to. A missing folder or file counts nothing.
pub fn measure(data: &Path) -> DataSizes {
    DataSizes { run_logs: run_logs(&data.join("runs")), decisions: regular_len(&data.join("intray").join("decisions.json")) }
}

/// The `.jsonl` files directly in the folder. Subfolders are not read.
fn run_logs(folder: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(folder) else { return 0 };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jsonl"))
        .map(|path| regular_len(&path))
        .fold(0, u64::saturating_add)
}

fn regular_len(path: &Path) -> u64 {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn run_logs_and_decisions_are_added_up() {
        let data = tempfile::tempdir().unwrap();
        write(&data.path().join("runs/20261001-101010-1.jsonl"), 100);
        write(&data.path().join("runs/20261002-101010-2.jsonl"), 250);
        write(&data.path().join("intray/decisions.json"), 40);
        assert_eq!(measure(data.path()), DataSizes { run_logs: 350, decisions: 40 });
        assert_eq!(measure(data.path()).total(), 390);
    }

    #[test]
    fn other_files_and_subfolders_do_not_count() {
        let data = tempfile::tempdir().unwrap();
        write(&data.path().join("runs/20261001-101010-1.jsonl"), 100);
        write(&data.path().join("runs/notes.txt"), 999);
        write(&data.path().join("runs/20261001-101010-1.jsonl.tmp"), 999);
        write(&data.path().join("runs/archive/20260901-101010-1.jsonl"), 999);
        write(&data.path().join("runs/folder.jsonl/inner.jsonl"), 999);
        write(&data.path().join("settings.json"), 999);
        write(&data.path().join("intray/other.json"), 999);
        write(&data.path().join("20261001-101010-9.jsonl"), 999);
        assert_eq!(measure(data.path()), DataSizes { run_logs: 100, decisions: 0 });
    }

    #[cfg(unix)]
    #[test]
    fn links_do_not_count() {
        let data = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(&outside.path().join("big.jsonl"), 5000);
        write(&outside.path().join("big.json"), 5000);
        write(&data.path().join("runs/20261001-101010-1.jsonl"), 10);
        std::os::unix::fs::symlink(outside.path().join("big.jsonl"), data.path().join("runs/20261001-101010-2.jsonl")).unwrap();
        fs::create_dir_all(data.path().join("intray")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("big.json"), data.path().join("intray/decisions.json")).unwrap();
        assert_eq!(measure(data.path()), DataSizes { run_logs: 10, decisions: 0 });
    }

    #[test]
    fn a_missing_folder_counts_nothing() {
        let data = tempfile::tempdir().unwrap();
        assert_eq!(measure(data.path()), DataSizes::default());
        assert_eq!(measure(&data.path().join("nowhere")), DataSizes::default());
        write(&data.path().join("runs/20261001-101010-1.jsonl"), 5);
        assert_eq!(measure(data.path()), DataSizes { run_logs: 5, decisions: 0 });
    }

    #[test]
    fn a_decisions_folder_counts_nothing() {
        let data = tempfile::tempdir().unwrap();
        write(&data.path().join("intray/decisions.json/inner"), 80);
        assert_eq!(measure(data.path()).decisions, 0);
    }
}
