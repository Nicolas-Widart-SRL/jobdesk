//! The record of a folder update: each file it writes or deletes, with the SHA-256 of what was there before and of
//! what it wrote, and where the original is in the backup. While the update runs it is `.jobdesk/update.json`, and a
//! record left there means the update stopped midway. Once the last file is in place it moves into the backup as
//! `update.json`, the record an undo reads. Both put back a file only while it still holds what the update wrote.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::folder_update::domain::{Reverted, UpdateError, side_name};
use crate::folder_update::infrastructure::disk::{
    Found, holds, in_jobdesk, is_plain_relative, is_real_dir, joined, look, read_plain, remove, sync_dir, write_new,
};
use crate::shared_kernel::installed_source::{self, sha256_hex};

/// The record of an update that runs, in `.jobdesk`.
pub const JOURNAL: &str = "update.json";
/// The backup of the last update, in `.jobdesk`.
pub const BACKUP: &str = "backup";
/// The record of the last update, in the backup.
pub const RECORD: &str = "update.json";
/// The folder of the backup that holds the originals, each under the plain number of its operation: no folders, no
/// links, and no copy that a tool walking the folder would take for a command, a setting or a profile.
pub const FILES: &str = "files";

/// The largest record that reads: 20,000 files of a release and their side copies need far less.
const MAX_JOURNAL: u64 = 32 * 1024 * 1024;

/// Every file an update writes or deletes, in the order it does so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    /// The version the update brings.
    pub version: String,
    /// The SHA-256 of the release zip the update brings, the pin's. `None` in a record written before it was kept.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Undo ran to its end on this record, which then names only the files it left as they were. A record without it
    /// beside a note of its release is an Undo that stopped partway.
    #[serde(default)]
    pub undone: bool,
    /// The folders the update makes, outermost first.
    pub folders: Vec<String>,
    pub ops: Vec<Op>,
}

/// One file an update writes or deletes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    /// Relative to the folder, with `/` separators.
    pub path: String,
    /// The SHA-256 of what the folder held there before. `None` when it held nothing.
    pub before: Option<String>,
    /// The name of the original in the backup's `files`: a plain number. Set exactly when `before` is.
    pub backup: Option<String>,
    /// The SHA-256 of what the update wrote. `None` for a delete.
    pub wrote: Option<String>,
}

/// What putting back one file did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// The file holds what it held before the update again.
    Reverted,
    /// The file already held what it held before: the update had not reached it, or it was put back already.
    Already,
    /// The file holds neither, or its original is missing from the backup: it is left as it is.
    Kept,
}

/// Writes `journal` as `name` in `dir` so that a stop at any moment leaves either no file or the whole one: written in
/// full beside it first, then renamed onto the name.
pub fn write(dir: &Path, name: &str, journal: &Journal) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(journal).map_err(io::Error::other)?;
    let part = dir.join(format!("{name}.part"));
    remove(&part)?;
    write_new(&part, &bytes, 0o600)?;
    fs::rename(&part, dir.join(name))?;
    sync_dir(dir)
}

/// The journal at `path`. `None` when nothing is there. A record that does not read, is a link, is larger than it may
/// be or names a path that is not plain is `BadJournal`.
pub fn read(path: &Path) -> Result<Option<Journal>, UpdateError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(UpdateError::Disk),
        Ok(meta) if !meta.is_file() || meta.len() > MAX_JOURNAL => return Err(UpdateError::BadJournal),
        Ok(_) => {}
    }
    let (bytes, _) = read_plain(path).map_err(|_| UpdateError::BadJournal)?;
    let journal: Journal = serde_json::from_slice(&bytes).map_err(|_| UpdateError::BadJournal)?;
    if journal.is_plain() { Ok(Some(journal)) } else { Err(UpdateError::BadJournal) }
}

impl Journal {
    /// Whether every path it names stays where an update writes: a plain relative path in the folder, outside
    /// `.jobdesk` but for the two files of the base, and a plain number in the backup's `files`. The record lives in
    /// the folder, where a run's tools can write, so it is checked before anything is put back from it.
    fn is_plain(&self) -> bool {
        let base = [installed_source::ZIP, installed_source::RECORD].map(|name| format!("{}/{name}", installed_source::DIR));
        let hash =
            |hash: &Option<String>| hash.as_deref().is_none_or(|hash| hash.len() == 64 && hash.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')));
        let number = |name: &str| (1..=10).contains(&name.len()) && name.bytes().all(|byte| byte.is_ascii_digit());
        hash(&self.sha256)
            && self.folders.iter().all(|folder| is_plain_relative(folder) && !in_jobdesk(folder))
            && self.ops.iter().all(|op| {
                is_plain_relative(&op.path)
                    && (!in_jobdesk(&op.path) || base.contains(&op.path))
                    && op.backup.as_deref().is_none_or(number)
                    && op.backup.is_some() == op.before.is_some()
                    && hash(&op.before)
                    && hash(&op.wrote)
            })
    }
}

/// Puts back every file of `journal` in `folder` that still holds what the update wrote, last written first, from the
/// backup in `jobdesk`, and removes the folders the update made once they are empty. A file that holds something else
/// is left as it is and listed. `.jobdesk`, its `backup` and the backup's `files` must be real folders, not links, or
/// nothing is put back. Every folder whose names it changed is on the disk before it returns.
pub fn revert_all(folder: &Path, jobdesk: &Path, journal: &Journal) -> io::Result<Reverted> {
    revert_from(folder, files_of(jobdesk)?, journal)
}

/// `revert_all`, with the originals in `files`, the backup's checked `files` folder. `None` puts back only what needs
/// no original, and keeps every file that does.
///
/// A safety file the update replaced and the copy of the user's file it saved beside it, `<path>.yours-<version>`, are
/// kept or put back together. While the file stays, for any reason, the copy holds the user's own settings from before
/// the update, so it stays too and is listed. A copy changed since keeps the file as well.
pub fn revert_from(folder: &Path, files: Option<PathBuf>, journal: &Journal) -> io::Result<Reverted> {
    let mut reverted = Reverted::default();
    let mut touched = BTreeSet::new();
    let pairs = Pairs::of(journal);
    // The safety files kept so far. The update wrote each copy before its file, so the file comes first here.
    let mut kept_files = BTreeSet::new();
    for op in journal.ops.iter().rev() {
        let outcome = match (pairs.file_of(&op.path), pairs.copy_of(&op.path)) {
            (Some(file), _) if kept_files.contains(file) => left(folder, op)?,
            (_, Some(copy)) if changed(folder, copy)? => left(folder, op)?,
            _ => revert(folder, files.as_deref(), op)?,
        };
        match outcome {
            Outcome::Reverted => {
                touched.extend(joined(folder, &op.path).parent().map(Path::to_path_buf));
                if !in_jobdesk(&op.path) {
                    reverted.reverted.push(op.path.clone());
                }
            }
            Outcome::Already => {}
            Outcome::Kept => {
                if pairs.copy_of(&op.path).is_some() {
                    kept_files.insert(op.path.as_str());
                }
                reverted.kept.push(op.path.clone());
            }
        }
    }
    for made in journal.folders.iter().rev() {
        // Looked at from the folder down, so a link on the way leaves it alone.
        if look(folder, made)?.found == Found::Other && is_real_dir(&joined(folder, made)) {
            // Only while empty: a folder that holds anything stays.
            if fs::remove_dir(joined(folder, made)).is_ok() {
                touched.extend(joined(folder, made).parent().map(Path::to_path_buf));
            }
        }
    }
    // An original renamed back left `files` too. A folder removed above is gone, and is left out.
    touched.extend(files);
    for dir in touched.iter().filter(|dir| is_real_dir(dir)) {
        sync_dir(dir)?;
    }
    reverted.reverted.sort();
    reverted.kept.sort();
    Ok(reverted)
}

/// Each safety file the update replaced and the copy of the user's file it saved beside it, by name: the copy's
/// operation is named `<path>.yours-<version>` for the file's `<path>` and the record's version. A release never holds
/// such a name, so only an upstream-wins step writes one.
struct Pairs<'a> {
    /// The copy's operation of each such file, by the file's path.
    copies: BTreeMap<&'a str, &'a Op>,
    /// The file's path of each copy, by the copy's path.
    files: BTreeMap<&'a str, &'a str>,
}

impl<'a> Pairs<'a> {
    fn of(journal: &'a Journal) -> Self {
        let by_path: BTreeMap<&str, &Op> = journal.ops.iter().map(|op| (op.path.as_str(), op)).collect();
        let (mut copies, mut files) = (BTreeMap::new(), BTreeMap::new());
        for op in &journal.ops {
            let Some(name) = side_name(&op.path, &journal.version, true) else { continue };
            if let Some(copy) = by_path.get(name.as_str()) {
                copies.insert(op.path.as_str(), *copy);
                files.insert(copy.path.as_str(), op.path.as_str());
            }
        }
        Self { copies, files }
    }

    /// The operation of the copy saved beside the safety file at `path`, when the update saved one.
    fn copy_of(&self, path: &str) -> Option<&'a Op> {
        self.copies.get(path).copied()
    }

    /// The safety file whose copy is at `path`, when it is one.
    fn file_of(&self, path: &str) -> Option<&'a str> {
        self.files.get(path).copied()
    }
}

/// Whether the file of `op` holds neither what it held before the update nor what the update wrote: it changed since.
/// A link on the way, or anything but a plain file, counts as changed.
fn changed(folder: &Path, op: &Op) -> io::Result<bool> {
    let current = match look(folder, &op.path)?.found {
        Found::File(bytes, _) => Some(sha256_hex(&bytes)),
        Found::Missing => None,
        Found::Link | Found::Other => return Ok(true),
    };
    Ok(current != op.before && current != op.wrote)
}

/// Leaves the file of `op` as it is: `Already` when it holds what it held before the update, else `Kept`.
fn left(folder: &Path, op: &Op) -> io::Result<Outcome> {
    Ok(if holds(folder, &op.path, op.before.as_deref())? { Outcome::Already } else { Outcome::Kept })
}

/// The backup's `files` of `jobdesk`, `None` when the backup holds none. `.jobdesk`, the backup or `files` as a link
/// or anything but a folder is an error.
pub fn files_of(jobdesk: &Path) -> io::Result<Option<PathBuf>> {
    let refused = || io::Error::other("a link or a file where the backup's folders go");
    let (backup, files) = (jobdesk.join(BACKUP), jobdesk.join(BACKUP).join(FILES));
    if !is_real_dir(jobdesk) {
        return Err(refused());
    }
    for dir in [&backup, &files] {
        match fs::symlink_metadata(dir) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
            Ok(meta) if !meta.is_dir() => return Err(refused()),
            Ok(_) => {}
        }
    }
    Ok(Some(files))
}

/// Puts back the file of `op`, when it still holds what the update wrote: its original renamed back from `files`, or
/// the file removed when there was none. The path is looked at from the folder down before it is read and again just
/// before it is written or removed, and a link found on the way leaves it as it is. Each look comes before its act: a
/// link or a save that lands between the two is not seen.
fn revert(folder: &Path, files: Option<&Path>, op: &Op) -> io::Result<Outcome> {
    let seen = look(folder, &op.path)?;
    let current = match &seen.found {
        Found::File(bytes, _) => Some(sha256_hex(bytes)),
        Found::Missing => None,
        Found::Link | Found::Other => return Ok(Outcome::Kept),
    };
    if current == op.before {
        return Ok(Outcome::Already);
    }
    if current != op.wrote {
        return Ok(Outcome::Kept);
    }
    let target = joined(folder, &op.path);
    let (Some(before), Some(saved)) = (&op.before, &op.backup) else {
        if !holds(folder, &op.path, op.wrote.as_deref())? {
            return Ok(Outcome::Kept);
        }
        fs::remove_file(&target)?;
        return Ok(Outcome::Reverted);
    };
    let Some(files) = files else { return Ok(Outcome::Kept) };
    let saved = files.join(saved);
    let original = fs::symlink_metadata(&saved).is_ok_and(|meta| meta.is_file()) && read_plain(&saved).is_ok_and(|(bytes, _)| sha256_hex(&bytes) == *before);
    if !original {
        return Ok(Outcome::Kept);
    }
    for missing in &seen.missing {
        match look(folder, missing)?.found {
            Found::Missing => {
                let made = joined(folder, missing);
                fs::create_dir(&made)?;
                // The new folder's name is in its parent: on the disk before the file goes in it.
                sync_dir(made.parent().unwrap_or(folder))?;
            }
            Found::Other if is_real_dir(&joined(folder, missing)) => {}
            _ => return Ok(Outcome::Kept),
        }
    }
    if !holds(folder, &op.path, op.wrote.as_deref())? {
        return Ok(Outcome::Kept);
    }
    fs::rename(&saved, &target)?;
    Ok(Outcome::Reverted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn journal(path: &str, backup: Option<&str>) -> Journal {
        let hash = sha256_hex(b"x");
        let op = Op { path: path.to_owned(), before: backup.map(|_| hash.clone()), backup: backup.map(str::to_owned), wrote: Some(hash.clone()) };
        Journal { version: "1.8.0".to_owned(), sha256: None, undone: false, folders: Vec::new(), ops: vec![op] }
    }

    #[test]
    fn a_journal_reads_back_as_written() {
        let dir = tempfile::tempdir().unwrap();
        let written = journal("CLAUDE.md", Some("0"));
        write(dir.path(), JOURNAL, &written).unwrap();
        assert_eq!(read(&dir.path().join(JOURNAL)), Ok(Some(written)));
        assert!(!dir.path().join("update.json.part").exists());
        assert_eq!(read(&dir.path().join("missing.json")), Ok(None));
    }

    #[test]
    fn a_journal_that_leads_out_of_the_folder_does_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let bad = [
            journal("../outside.md", None),
            journal("/etc/hosts", None),
            journal("CLAUDE.md", Some("../../outside")),
            journal("CLAUDE.md", Some("files/CLAUDE.md")),
            journal("CLAUDE.md", Some("l/0")),
            journal("CLAUDE.md", Some("")),
            journal("CLAUDE.md", Some("-1")),
            journal(".jobdesk/backup/update.json", None),
            journal(".JOBDESK/source.zip", None),
            Journal { folders: vec!["..".to_owned()], ..journal("CLAUDE.md", None) },
            Journal { sha256: Some("NOT-A-HASH".to_owned()), ..journal("CLAUDE.md", None) },
            Journal { folders: vec![".jobdesk".to_owned()], ..journal("CLAUDE.md", None) },
            Journal { ops: vec![Op { wrote: Some("not a hash".to_owned()), ..journal("CLAUDE.md", None).ops[0].clone() }], ..journal("CLAUDE.md", None) },
            Journal { ops: vec![Op { backup: None, ..journal("CLAUDE.md", Some("0")).ops[0].clone() }], ..journal("CLAUDE.md", None) },
        ];
        for journal in bad {
            write(dir.path(), JOURNAL, &journal).unwrap();
            assert_eq!(read(&dir.path().join(JOURNAL)), Err(UpdateError::BadJournal), "{journal:?}");
        }
        assert!(journal(".jobdesk/source.zip", Some("12")).is_plain(), "the base's own files may be named");
        fs::write(dir.path().join(JOURNAL), b"not json").unwrap();
        assert_eq!(read(&dir.path().join(JOURNAL)), Err(UpdateError::BadJournal));
    }
}
