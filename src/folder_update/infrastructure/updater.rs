//! Runs a folder update on the disk, and undoes it.
//!
//! An update holds the folder's lock, `.jobdesk/.open`, alone from start to end, so it runs only while no other
//! Jobdesk has the folder open, and two Jobdesks never update one folder at once: see `hold`. It writes every new file into a hidden staging folder beside the job search folder, on the same disk, and
//! copies the original of every file it replaces or deletes, and the old base, into a backup there too. Only then does
//! it touch the folder: the backup takes the place of the last update's in `.jobdesk/backup`, the journal is written,
//! and each staged file is renamed into place. Anything that fails before the first rename leaves the folder as it was;
//! anything after it rolls the folder back from the journal.
//!
//! `update`, `undo` and `recover` must run only while no task and no Claude session is open, and their caller keeps it
//! so. Every check of a file or of a link on its path comes just before the act it guards, never with it: a save, or a
//! folder turned into a link, that lands in the gap between the two is not seen. A run's tools could do that at any
//! moment, so these checks guard against what the user leaves in the folder, not against a run that works meanwhile.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

use crate::folder_update::domain::{Action, Plan, Report, Reverted, Update, UpdateError};
use crate::folder_update::infrastructure::disk::{Found, exists, is_real_dir, joined, look, remove, sync_dir, write_new};
use crate::folder_update::infrastructure::hold::hold_alone;
use crate::folder_update::infrastructure::journal::{self, BACKUP, FILES, JOURNAL, Journal, Op, RECORD};
use crate::folder_update::infrastructure::planner::plan;
use crate::shared_kernel::installed_source::{self, InstalledSource, sha256_hex};

/// Where `recover` sets aside, in `.jobdesk`, a journal it could not roll back in full or could not read.
const STOPPED: &str = "update-stopped.json";
/// Where Undo notes, in the backup, the release it undid, so no later pass applies that same release again.
const SKIPPED: &str = "skipped.json";

/// A moment of an update a test can stop or fail it at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    /// Every file is staged and backed up, and the folder is untouched.
    Staged,
    /// The backup has taken the last update's place, and the journal is not written yet.
    Swapped,
    /// The journal is written, and no file is renamed in yet.
    Journaled,
    /// The file of the journal's operation with this index is in place.
    Applied(usize),
}

/// One file the update writes or deletes, in the order it does.
struct Change {
    /// Relative to the folder, with `/` separators.
    path: String,
    /// The bytes and mode it writes. `None` deletes the file.
    write: Option<(Vec<u8>, u32)>,
    /// What the folder holds there, as the plan saw it.
    before: Option<Vec<u8>>,
}

/// Updates `folder` from the release in `update.base` to the one in `update.new`. On success it reports what changed.
pub fn update(folder: &Path, update: &Update) -> Result<Report, UpdateError> {
    update_with(folder, update, &mut |_| Ok(()))
}

/// `update`, with `hook` heard at each `Moment`. An error from it fails the update there.
pub fn update_with(folder: &Path, update: &Update, hook: &mut dyn FnMut(Moment) -> io::Result<()>) -> Result<Report, UpdateError> {
    let jobdesk = jobdesk_of(folder)?;
    let _alone = hold_alone(&jobdesk)?;
    if exists(&jobdesk.join(JOURNAL)) {
        return Err(UpdateError::Interrupted);
    }
    // The gate read the base and the note before the download, and another Jobdesk may have changed either since.
    gate_holds(folder, &jobdesk, update)?;
    let plan = plan(folder, update)?;
    let changes = changes(folder, &plan, update)?;
    let staging = Staging::start(folder)?;
    let applied = apply(folder, &jobdesk, &staging.path, (&plan, update.new.sha256), &changes, hook);
    staging.end();
    applied.map(|()| plan.report())
}

/// Rolls `folder` back from the journal an update that stopped midway left, and removes the staging folders such an
/// update left beside it. `None` when no update stopped midway.
///
/// An update that stopped after its backup took the last update's place left that earlier backup in its staging
/// folder, which this removes: the update before can then no longer be undone. That is the user's "Last update only":
/// the stopped update's backup has already taken the earlier one's place, and the folder itself is rolled back whole.
///
/// It never leaves the folder stuck. When the backup is missing, a link or not a real folder, it puts back only what
/// needs no original, sets the journal aside as `.jobdesk/update-stopped.json`, and is `Kept` with every file whose
/// original it could not put back. A journal that does not read is set aside the same way, as `BadJournal`. Either way
/// the next update runs.
pub fn recover(folder: &Path) -> Result<Option<Reverted>, UpdateError> {
    let jobdesk = folder.join(installed_source::DIR);
    if !is_real_dir(&jobdesk) {
        return Ok(None);
    }
    let _alone = hold_alone(&jobdesk)?;
    clear_stagings(folder);
    let journal = match journal::read(&jobdesk.join(JOURNAL)) {
        Ok(Some(journal)) => journal,
        Ok(None) => return Ok(None),
        Err(UpdateError::BadJournal) => {
            set_aside(&jobdesk)?;
            return Err(UpdateError::BadJournal);
        }
        Err(error) => return Err(error),
    };
    if !is_real_dir(&jobdesk.join(BACKUP)) || journal::files_of(&jobdesk).is_err() {
        let reverted = journal::revert_from(folder, None, &journal).map_err(|_| UpdateError::Disk)?;
        set_aside(&jobdesk)?;
        return if reverted.kept.is_empty() { Ok(Some(reverted)) } else { Err(UpdateError::Kept(reverted.kept)) };
    }
    let reverted = journal::revert_all(folder, &jobdesk, &journal).map_err(|_| UpdateError::Disk)?;
    if reverted.kept.is_empty() {
        fs::remove_file(jobdesk.join(JOURNAL)).map_err(|_| UpdateError::Disk)?;
    } else {
        // The originals of the files kept live only in the backup: the journal stays beside them as its record.
        fs::rename(jobdesk.join(JOURNAL), jobdesk.join(BACKUP).join(RECORD)).map_err(|_| UpdateError::Disk)?;
        sync_dir(&jobdesk.join(BACKUP)).map_err(|_| UpdateError::Disk)?;
    }
    sync_dir(&jobdesk).map_err(|_| UpdateError::Disk)?;
    Ok(Some(reverted))
}

/// Whether the last update of `folder` can be undone: its backup and its record are there.
pub fn can_undo(folder: &Path) -> bool {
    let jobdesk = folder.join(installed_source::DIR);
    is_real_dir(&jobdesk) && !exists(&jobdesk.join(JOURNAL)) && matches!(journal::read(&jobdesk.join(BACKUP).join(RECORD)), Ok(Some(_)))
}

/// Undoes the last update of `folder`: puts back each file that still holds what the update wrote, removes the side
/// copies it made that are unchanged, and puts back the old base. A file changed since is left as it is and listed,
/// and so is the copy of the user's file beside a safety file it leaves, which `journal::revert_from` pairs with it,
/// and the record then names only those files and their originals, marked undone. Jobdesk does not offer Undo for it
/// again: the originals stay in the backup until the next update's backup replaces them, as "Last update
/// only" allows.
pub fn undo(folder: &Path) -> Result<Reverted, UpdateError> {
    let jobdesk = jobdesk_of(folder)?;
    let _alone = hold_alone(&jobdesk)?;
    if exists(&jobdesk.join(JOURNAL)) {
        return Err(UpdateError::Interrupted);
    }
    let backup = jobdesk.join(BACKUP);
    if !is_real_dir(&backup) {
        return Err(UpdateError::NothingToUndo);
    }
    let Some(record) = journal::read(&backup.join(RECORD))? else { return Err(UpdateError::NothingToUndo) };
    // The release is noted before anything is put back, so a folder is never undone without its note: a note that
    // cannot be written refuses the Undo with the folder as it was.
    let undone = undone_release(folder, &record).ok_or(UpdateError::Unnoted)?;
    note_skipped(&backup, &undone).map_err(|_| UpdateError::Unnoted)?;
    let reverted = journal::revert_all(folder, &jobdesk, &record).map_err(|_| UpdateError::Disk)?;
    if reverted.kept.is_empty() {
        fs::remove_file(backup.join(RECORD)).map_err(|_| UpdateError::Disk)?;
        sync_dir(&backup).map_err(|_| UpdateError::Disk)?;
    } else {
        // The record is all that says which numbered original is whose: it stays, naming only the files kept and
        // marked undone, so the originals can still be found in the backup. Jobdesk does not offer Undo for it again.
        let ops = record.ops.iter().filter(|op| reverted.kept.contains(&op.path)).cloned().collect();
        journal::write(&backup, RECORD, &Journal { ops, undone: true, ..record }).map_err(|_| UpdateError::Disk)?;
    }
    Ok(reverted)
}

/// Whether an Undo of `folder` noted its release and stopped before it put every file back: the update's record is
/// still there, Undo has not run it to its end, and the note names the release it records. It only reads.
pub fn undo_unfinished(folder: &Path) -> bool {
    if !can_undo(folder) {
        return false;
    }
    let Ok(Some(record)) = journal::read(&folder.join(installed_source::DIR).join(BACKUP).join(RECORD)) else { return false };
    let Some(note) = skipped(folder) else { return false };
    !record.undone && note.version == record.version && record.sha256.as_ref().is_none_or(|sha256| *sha256 == note.sha256)
}

/// The release the last Undo of `folder` undid, as it noted it in the backup: `None` when none is noted, when `.jobdesk`
/// or the backup is a link or not a folder, or when the note does not read. The next update's backup takes the place of
/// the one that holds it, so it lasts until a newer release updates the folder.
pub fn skipped(folder: &Path) -> Option<InstalledSource> {
    let (jobdesk, backup) = (folder.join(installed_source::DIR), folder.join(installed_source::DIR).join(BACKUP));
    if !is_real_dir(&jobdesk) || !is_real_dir(&backup) {
        return None;
    }
    installed_source::read_record(&backup.join(SKIPPED))
}

/// The release the update of `record` applied: its version and hash as the record keeps them, or, for a record written
/// before it kept the hash, the folder's base when it reads and is of the record's version. `None` when neither says.
fn undone_release(folder: &Path, record: &Journal) -> Option<InstalledSource> {
    if !installed_source::plain_version(&record.version) {
        return None;
    }
    match &record.sha256 {
        Some(sha256) => Some(InstalledSource { version: record.version.clone(), sha256: sha256.clone() }),
        None => installed_source::read(folder).map(|(source, _)| source).filter(|source| source.version == record.version),
    }
}

/// Notes `undone` in `backup` as the release the user undid, over any older note, so that a stop at any moment leaves
/// the old note or the whole new one: written in full beside it first, then renamed onto its name.
fn note_skipped(backup: &Path, undone: &InstalledSource) -> io::Result<()> {
    let part = backup.join(format!("{SKIPPED}.part"));
    remove(&part)?;
    let written = write_new(&part, &installed_source::record(undone)?, 0o600).and_then(|()| fs::rename(&part, backup.join(SKIPPED)));
    if written.is_err() {
        // A refused Undo leaves the folder as it found it.
        let _ = remove(&part);
    }
    written?;
    sync_dir(backup)
}

/// Moves the journal of `jobdesk` to `STOPPED`, over any older one, so no later update or undo waits on it.
fn set_aside(jobdesk: &Path) -> Result<(), UpdateError> {
    remove(&jobdesk.join(STOPPED)).map_err(|_| UpdateError::Disk)?;
    fs::rename(jobdesk.join(JOURNAL), jobdesk.join(STOPPED)).map_err(|_| UpdateError::Disk)?;
    sync_dir(jobdesk).map_err(|_| UpdateError::Disk)
}

/// Whether what the gate read of `folder` still holds, once the update holds it alone: the base's record in `jobdesk`
/// names the release the gate read, and no Undo has noted this very release since. `Superseded` when the base is now
/// this release or a newer one, `Busy` when it moved anywhere else or does not read, and `Undone` when the note now
/// names this release, version and hash alike.
fn gate_holds(folder: &Path, jobdesk: &Path, update: &Update) -> Result<(), UpdateError> {
    let seen = InstalledSource { version: update.base_version.to_owned(), sha256: update.base.sha256.to_owned() };
    let new = numbers(update.version);
    match installed_source::read_record(&jobdesk.join(installed_source::RECORD)) {
        Some(base) if base == seen => {}
        Some(base) if new.is_some() && numbers(&base.version) >= new => return Err(UpdateError::Superseded),
        _ => return Err(UpdateError::Busy),
    }
    let undone = |note: InstalledSource| new.is_some() && numbers(&note.version) == new && note.sha256 == update.new.sha256;
    if skipped(folder).is_some_and(undone) { Err(UpdateError::Undone) } else { Ok(()) }
}

/// The three numbers of a plain version, such as `1.8.0`, which compare as versions do. `None` for any other.
fn numbers(version: &str) -> Option<[u32; 3]> {
    if !installed_source::plain_version(version) {
        return None;
    }
    let mut parts = version.split('.').map(|part| part.parse().ok());
    Some([parts.next()??, parts.next()??, parts.next()??])
}

/// The folder's `.jobdesk`, when it is a real folder and not a link.
fn jobdesk_of(folder: &Path) -> Result<PathBuf, UpdateError> {
    let jobdesk = folder.join(installed_source::DIR);
    if is_real_dir(&jobdesk) { Ok(jobdesk) } else { Err(UpdateError::NoBase) }
}

/// Every file the plan writes or deletes, then the new base, in the order the update does them.
fn changes(folder: &Path, plan: &Plan, update: &Update) -> Result<Vec<Change>, UpdateError> {
    let mut changes = Vec::new();
    let mut push = |path: &str, write: Option<(&[u8], u32)>, before: &Option<Vec<u8>>| {
        let write = write.map(|(bytes, mode)| (bytes.to_vec(), mode));
        changes.push(Change { path: path.to_owned(), write, before: before.clone() });
    };
    for step in &plan.steps {
        match (&step.action, &step.side) {
            (Action::Write(bytes) | Action::Restore(bytes), _) => push(&step.path, Some((bytes, step.mode)), &step.ours),
            (Action::Delete, _) => push(&step.path, None, &step.ours),
            (Action::Conflict { theirs }, Some(side)) => {
                // A copy already there with the same bytes stays as it is.
                if side.existing.as_ref() != Some(theirs) {
                    push(&side.path, Some((theirs, side.mode)), &side.existing);
                }
            }
            (Action::UpstreamWins { ours, theirs }, Some(side)) => {
                if side.existing.as_ref() != Some(ours) {
                    push(&side.path, Some((ours, side.mode)), &side.existing);
                }
                push(&step.path, Some((theirs, step.mode)), &step.ours);
            }
            _ => {}
        }
    }
    let source = InstalledSource { version: update.version.to_owned(), sha256: update.new.sha256.to_owned() };
    let record = installed_source::record(&source).map_err(|_| UpdateError::Disk)?;
    for (name, bytes) in [(installed_source::ZIP, update.new.zip.to_vec()), (installed_source::RECORD, record)] {
        let path = format!("{}/{name}", installed_source::DIR);
        let before = match look(folder, &path).map_err(|_| UpdateError::Disk)?.found {
            Found::File(bytes, _) => Some(bytes),
            Found::Missing => None,
            Found::Link | Found::Other => return Err(UpdateError::NoBase),
        };
        changes.push(Change { path, write: Some((bytes, 0o600)), before });
    }
    Ok(changes)
}

/// Stages and backs up every change in `staging`, swaps the backup in, writes the journal, and renames each staged file
/// into place. A failure after the journal is written rolls the folder back. The plan comes with the SHA-256 of the
/// release zip it brings, which the journal keeps beside its version for a later Undo to note.
fn apply(
    folder: &Path,
    jobdesk: &Path,
    staging: &Path,
    (plan, sha256): (&Plan, &str),
    changes: &[Change],
    hook: &mut dyn FnMut(Moment) -> io::Result<()>,
) -> Result<(), UpdateError> {
    let disk = |_| UpdateError::Disk;
    let staged = staging.join("new");
    let backup = staging.join(BACKUP);
    fs::create_dir(&staged).map_err(disk)?;
    fs::create_dir(&backup).map_err(disk)?;
    fs::create_dir(backup.join(FILES)).map_err(disk)?;
    let ops = stage(folder, &staged, &backup.join(FILES), changes)?;
    let folders: BTreeSet<&String> = plan.steps.iter().flat_map(|step| &step.folders).collect();
    let journal =
        Journal { version: plan.version.clone(), sha256: Some(sha256.to_owned()), undone: false, folders: folders.into_iter().cloned().collect(), ops };
    hook(Moment::Staged).map_err(disk)?;

    // The last update's backup steps aside, so it comes back if this one fails before the first rename.
    let kept = jobdesk.join(BACKUP);
    let previous = staging.join("previous");
    let had_previous = exists(&kept);
    if had_previous {
        fs::rename(&kept, &previous).map_err(disk)?;
    }
    let put_back = || {
        let _ = remove(&kept);
        if had_previous {
            let _ = fs::rename(&previous, &kept);
        }
        // With the journal's removal before it, if any: nothing is left to roll back, so a failed sync is not one.
        let _ = sync_dir(jobdesk);
    };
    if fs::rename(&backup, &kept).and_then(|()| hook(Moment::Swapped)).and_then(|()| journal::write(jobdesk, JOURNAL, &journal)).is_err() {
        let _ = remove(&jobdesk.join(JOURNAL));
        let _ = remove(&jobdesk.join(format!("{JOURNAL}.part")));
        put_back();
        return Err(UpdateError::Disk);
    }

    let mut done = 0;
    let placed = hook(Moment::Journaled).map_err(disk).and_then(|()| place(folder, &staged, &journal, changes, hook, &mut done)).and_then(|()| {
        // The journal becomes the record of the last update, which an undo reads.
        fs::rename(jobdesk.join(JOURNAL), kept.join(RECORD)).map_err(disk)
    });
    let Err(error) = placed else {
        // The update is whole once the rename is done, so a failed sync after it is not a reason to roll back.
        let _ = sync_dir(&kept);
        let _ = sync_dir(jobdesk);
        return Ok(());
    };
    // Only the changes that were done are put back: one not reached yet may hold a change the user made meanwhile.
    let reached = Journal { ops: journal.ops[..done].to_vec(), ..journal.clone() };
    match journal::revert_all(folder, jobdesk, &reached) {
        Ok(reverted) if reverted.kept.is_empty() => match fs::remove_file(jobdesk.join(JOURNAL)) {
            Ok(()) => {
                put_back();
                Err(error)
            }
            Err(_) => Err(UpdateError::RollbackFailed),
        },
        // A file changed since the update wrote it stays, and its original lives only in the backup: the backup stays,
        // with the journal as its record of where each original is.
        Ok(reverted) => match fs::rename(jobdesk.join(JOURNAL), kept.join(RECORD)) {
            Ok(()) => {
                let _ = sync_dir(&kept);
                let _ = sync_dir(jobdesk);
                Err(UpdateError::Kept(reverted.kept))
            }
            Err(_) => Err(UpdateError::RollbackFailed),
        },
        // The journal stays, so the next rollback tries again.
        Err(_) => Err(UpdateError::RollbackFailed),
    }
}

/// Writes each change's new bytes into `staged`, and the original of each file it replaces or deletes into `files`,
/// both under the change's index. Each file the plan read must still hold what it read, or nothing goes further.
fn stage(folder: &Path, staged: &Path, files: &Path, changes: &[Change]) -> Result<Vec<Op>, UpdateError> {
    let disk = |_| UpdateError::Disk;
    let mut ops = Vec::with_capacity(changes.len());
    for (index, change) in changes.iter().enumerate() {
        let current = match look(folder, &change.path).map_err(disk)?.found {
            Found::File(bytes, mode) => Some((bytes, mode)),
            Found::Missing => None,
            Found::Link | Found::Other => return Err(UpdateError::Changed),
        };
        if current.as_ref().map(|(bytes, _)| bytes) != change.before.as_ref() {
            return Err(UpdateError::Changed);
        }
        let saved = match current {
            Some((bytes, mode)) => {
                write_new(&files.join(index.to_string()), &bytes, mode).map_err(disk)?;
                Some(index.to_string())
            }
            None => None,
        };
        if let Some((bytes, mode)) = &change.write {
            write_new(&staged.join(index.to_string()), bytes, *mode).map_err(disk)?;
        }
        let wrote = change.write.as_ref().map(|(bytes, _)| sha256_hex(bytes));
        ops.push(Op { path: change.path.clone(), before: change.before.as_deref().map(sha256_hex), backup: saved, wrote });
    }
    for dir in [files, files.parent().unwrap_or(files), staged] {
        sync_dir(dir).map_err(disk)?;
    }
    Ok(ops)
}

/// Makes the folders the journal lists, then renames each staged file into place, or deletes the file, in order, and
/// counts in `done` the changes done. Just before each, the file is read again from the folder down: a link on the way,
/// or anything but what the journal says it held before, is `Changed`. So a file saved after it was staged and before
/// that read is never replaced. A save in the short gap between that read and the rename is not seen, and is lost.
fn place(
    folder: &Path,
    staged: &Path,
    journal: &Journal,
    changes: &[Change],
    hook: &mut dyn FnMut(Moment) -> io::Result<()>,
    done: &mut usize,
) -> Result<(), UpdateError> {
    let disk = |_| UpdateError::Disk;
    for made in &journal.folders {
        match look(folder, made).map_err(disk)?.found {
            Found::Missing => fs::create_dir(joined(folder, made)).map_err(disk)?,
            Found::Other if is_real_dir(&joined(folder, made)) => {}
            _ => return Err(UpdateError::Changed),
        }
    }
    for ((index, change), op) in changes.iter().enumerate().zip(&journal.ops) {
        let seen = look(folder, &change.path).map_err(disk)?;
        let current = match &seen.found {
            Found::File(bytes, _) => Some(sha256_hex(bytes)),
            Found::Missing if seen.missing.is_empty() => None,
            Found::Missing | Found::Link | Found::Other => return Err(UpdateError::Changed),
        };
        if current != op.before {
            return Err(UpdateError::Changed);
        }
        let target = joined(folder, &change.path);
        match change.write {
            Some(_) => fs::rename(staged.join(index.to_string()), &target).map_err(disk)?,
            None => fs::remove_file(&target).map_err(disk)?,
        }
        *done = index + 1;
        hook(Moment::Applied(index)).map_err(disk)?;
    }
    // Every name is on the disk before the journal becomes the record, so a power loss never leaves an old file
    // behind a finished update.
    let touched: BTreeSet<PathBuf> = changes.iter().filter_map(|change| joined(folder, &change.path).parent().map(Path::to_path_buf)).collect();
    let made = journal.folders.iter().filter_map(|made| joined(folder, made).parent().map(Path::to_path_buf));
    for dir in touched.into_iter().chain(made).collect::<BTreeSet<_>>() {
        sync_dir(&dir).map_err(disk)?;
    }
    Ok(())
}

/// The start of the name of every staging folder of a folder update of `folder`: `.<name>.jobdesk-update-`. Setup's
/// own staging folders, `.<name>.jobdesk-<pid>`, never match it, nor it theirs.
fn staging_prefix(folder: &Path) -> Option<String> {
    Some(format!(".{}.jobdesk-update-", folder.file_name()?.to_str()?))
}

/// Removes the staging folders of updates of `folder` left beside it. Only while the folder's lock is held alone: then no
/// other Jobdesk is updating it, so none of them is in use. Links are removed, never followed. A stopped update's
/// staging folder may hold the backup of the update before it, which goes with it: see `recover`.
fn clear_stagings(folder: &Path) {
    let (Some(parent), Some(prefix)) = (folder.parent(), staging_prefix(folder)) else { return };
    let Ok(entries) = fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid = name.to_str().and_then(|name| name.strip_prefix(prefix.as_str()));
        if pid.is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit())) {
            let _ = remove(&entry.path());
        }
    }
}

/// This Jobdesk's staging folder for an update of a folder, hidden beside it, so each rename into the folder stays on
/// one disk. It is removed at the end, whatever the end.
struct Staging {
    path: PathBuf,
}

impl Staging {
    /// Clears what earlier updates left, then makes this one's staging folder, readable only by the user.
    fn start(folder: &Path) -> Result<Self, UpdateError> {
        let (Some(parent), Some(prefix)) = (folder.parent(), staging_prefix(folder)) else { return Err(UpdateError::Disk) };
        clear_stagings(folder);
        let mut name = OsString::from(prefix);
        name.push(process::id().to_string());
        let path = parent.join(name);
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&path).map_err(|_| UpdateError::Disk)?;
        Ok(Self { path })
    }

    fn end(self) {
        let _ = remove(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::panic::{self, AssertUnwindSafe};

    use super::*;
    use crate::folder_update::domain::{Release, SideCopy, SkipReason, Skipped};
    use crate::folder_update::infrastructure::hold::{Open, hold_open};
    use crate::shared_kernel::release_zip::fixtures::{Entry, zip_of};

    const OLD: &str = "1.7.5";
    const NEW: &str = "1.8.0";

    const CLAUDE_BASE: &str = "# Job search\n\nName: [YOUR_NAME]\n\n## How to apply\n\nWrite one letter per role.\n";
    const CLAUDE_NEW: &str = "# Job search\n\nName: [YOUR_NAME]\n\n## How to apply\n\nWrite one short letter per role.\n";
    const CLAUDE_OURS: &str = "# Job search\n\nName: Ada Quill\n\n## How to apply\n\nWrite one letter per role.\n";
    const CLAUDE_MERGED: &str = "# Job search\n\nName: Ada Quill\n\n## How to apply\n\nWrite one short letter per role.\n";
    const SETTINGS_OURS: &str = "{\n  \"allow\": [\"Read\", \"Bash\"]\n}\n";
    const SETTINGS_NEW: &str = "{\n  \"allow\": [\"Read\", \"Grep\"]\n}\n";
    const GUIDE_OURS: &str = "one\ntwo\nthree\nfour\nfive, mine\n";
    const GUIDE_NEW: &str = "one\ntwo\nthree\nfour\nFIVE\n";

    const BASE: &[(&str, &str)] = &[
        ("CLAUDE.md", CLAUDE_BASE),
        (".claude/commands/apply.md", "Apply to the role.\n"),
        (".claude/commands/old.md", "An old command.\n"),
        (".claude/settings.json", "{\n  \"allow\": [\"Read\"]\n}\n"),
        ("tools/security_guards.py", "GUARD = 1\n"),
        ("docs/guide.md", "one\ntwo\nthree\nfour\nfive\n"),
        ("docs/notes.md", "Notes\n"),
        ("templates/letter.md", "Dear team,\n"),
    ];

    const NEW_FILES: &[(&str, &str)] = &[
        ("CLAUDE.md", CLAUDE_NEW),
        (".claude/commands/apply.md", "Apply to the role, then log it.\n"),
        (".claude/commands/review.md", "Review the week.\n"),
        (".claude/settings.json", SETTINGS_NEW),
        ("tools/security_guards.py", "GUARD = 2\n"),
        ("docs/guide.md", GUIDE_NEW),
        ("docs/notes.md", "Notes, upstream\n"),
        ("templates/letter.md", "Dear hiring team,\n"),
        ("templates/new/intro.md", "Hello.\n"),
        ("scripts/run.sh", "#!/bin/sh\necho run\n"),
    ];

    /// A fictional release zip of `files` in a top folder named for `version`. A `.sh` file may run.
    fn release(version: &str, files: &[(&str, &str)]) -> Vec<u8> {
        let names: Vec<String> = files.iter().map(|(path, _)| format!("ai-job-search-{version}/{path}")).collect();
        let entries: Vec<Entry> =
            names.iter().zip(files).map(|(name, (path, text))| Entry::Mode(name, text.as_bytes(), if path.ends_with(".sh") { 0o755 } else { 0o644 })).collect();
        zip_of(&entries)
    }

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &to.join(entry.file_name()));
            } else {
                fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
            }
        }
    }

    /// Every file under `dir` by its path with `/` separators, a link as the place it points to. The lock is left out:
    /// it is the one file an update adds and never removes.
    fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                let name = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                let kind = entry.file_type().unwrap();
                if kind.is_symlink() {
                    found.insert(name, format!("link to {}", fs::read_link(&path).unwrap().display()).into_bytes());
                } else if kind.is_dir() {
                    found.insert(format!("{name}/"), Vec::new());
                    walk(root, &path, found);
                } else if name != ".jobdesk/.open" {
                    found.insert(name, fs::read(&path).unwrap());
                }
            }
        }
        let mut found = BTreeMap::new();
        walk(dir, dir, &mut found);
        found
    }

    /// `tree` without the backup.
    fn without_backup(mut tree: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
        tree.retain(|path, _| !path.starts_with(".jobdesk/backup"));
        tree
    }

    /// A copy of the sample workspace set up from a fictional base release, beside a newer one.
    struct Fixture {
        _temp: tempfile::TempDir,
        folder: PathBuf,
        base: Vec<u8>,
        base_sha256: String,
        base_version: &'static str,
        new: Vec<u8>,
        new_sha256: String,
        version: &'static str,
    }

    impl Fixture {
        fn new(base: &[(&str, &str)], new: &[(&str, &str)]) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let folder = temp.path().join("Job search");
            copy_tree(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample_workspace"), &folder);
            for (path, text) in base {
                let at = joined(&folder, path);
                fs::create_dir_all(at.parent().unwrap()).unwrap();
                fs::write(at, text).unwrap();
            }
            let base = release(OLD, base);
            let base_sha256 = sha256_hex(&base);
            installed_source::write_into(&folder, &InstalledSource { version: OLD.to_owned(), sha256: base_sha256.clone() }, &base).unwrap();
            let new = release(NEW, new);
            let new_sha256 = sha256_hex(&new);
            Self { _temp: temp, folder, base, base_sha256, base_version: OLD, new, new_sha256, version: NEW }
        }

        /// The fixture with the user's own changes: a filled-in profile, a widened setting, a conflicting edit, two
        /// deleted files and private notes of their own.
        fn edited() -> Self {
            let fixture = Self::new(BASE, NEW_FILES);
            fixture.write("CLAUDE.md", CLAUDE_OURS);
            fixture.write(".claude/settings.json", SETTINGS_OURS);
            fixture.write("docs/guide.md", GUIDE_OURS);
            fixture.write("notes/private.md", "Questions for the studio call.\n");
            fixture.write(".env", "FICTIONAL=1\n");
            fs::remove_file(fixture.folder.join("tools/security_guards.py")).unwrap();
            fs::remove_file(fixture.folder.join("docs/notes.md")).unwrap();
            fixture
        }

        /// The update from this fixture's base to its new release.
        fn update(&self) -> Update<'_> {
            Update {
                base: Release { zip: &self.base, sha256: &self.base_sha256 },
                base_version: self.base_version,
                new: Release { zip: &self.new, sha256: &self.new_sha256 },
                version: self.version,
            }
        }

        fn run(&self) -> Result<Report, UpdateError> {
            update(&self.folder, &self.update())
        }

        /// Makes the folder's new release its base, and `files` of `version` the next release.
        fn next(&mut self, version: &'static str, files: &[(&str, &str)]) {
            self.base = std::mem::take(&mut self.new);
            self.base_sha256 = std::mem::take(&mut self.new_sha256);
            self.base_version = self.version;
            self.instead(version, files);
        }

        /// Keeps the folder's base, and makes `files` of `version` the next release.
        fn instead(&mut self, version: &'static str, files: &[(&str, &str)]) {
            self.new = release(version, files);
            self.new_sha256 = sha256_hex(&self.new);
            self.version = version;
        }

        fn write(&self, path: &str, text: &str) {
            let at = joined(&self.folder, path);
            fs::create_dir_all(at.parent().unwrap()).unwrap();
            fs::write(at, text).unwrap();
        }

        fn read(&self, path: &str) -> Option<String> {
            fs::read_to_string(joined(&self.folder, path)).ok()
        }

        fn jobdesk(&self) -> PathBuf {
            self.folder.join(installed_source::DIR)
        }

        /// Everything beside the folder, so a test sees no staging folder is left.
        fn around(&self) -> Vec<String> {
            let mut names: Vec<String> =
                fs::read_dir(self.folder.parent().unwrap()).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        }
    }

    /// The names in `dir`, sorted.
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    fn strings(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| (*path).to_owned()).collect()
    }

    /// The original of `path` in the backup of the last update, found through its record.
    fn backed_up(fixture: &Fixture, path: &str) -> String {
        let record = journal::read(&fixture.jobdesk().join(BACKUP).join(RECORD)).unwrap().unwrap();
        let op = record.ops.iter().find(|op| op.path == path).unwrap();
        fs::read_to_string(fixture.jobdesk().join(BACKUP).join(FILES).join(op.backup.as_deref().unwrap())).unwrap()
    }

    #[test]
    fn a_clean_update_takes_every_upstream_change() {
        let fixture = Fixture::new(BASE, NEW_FILES);
        let report = fixture.run().unwrap();
        assert_eq!(
            report,
            Report {
                version: NEW.to_owned(),
                updated: strings(&[
                    ".claude/commands/apply.md",
                    ".claude/settings.json",
                    "CLAUDE.md",
                    "docs/guide.md",
                    "docs/notes.md",
                    "templates/letter.md",
                    "tools/security_guards.py",
                ]),
                added: strings(&[".claude/commands/review.md", "scripts/run.sh", "templates/new/intro.md"]),
                deleted: strings(&[".claude/commands/old.md"]),
                ..Report::default()
            }
        );
        for (path, text) in NEW_FILES {
            assert_eq!(fixture.read(path).as_deref(), Some(*text), "{path}");
        }
        assert_eq!(fixture.read(".claude/commands/old.md"), None);
        let source = InstalledSource { version: NEW.to_owned(), sha256: fixture.new_sha256.clone() };
        assert_eq!(installed_source::read(&fixture.folder), Some((source, fixture.new.clone())), "the new release is the base");
        assert!(!fixture.jobdesk().join(JOURNAL).exists(), "the journal goes after the last rename");
        assert!(can_undo(&fixture.folder));
        assert_eq!(fixture.around(), ["Job search"], "no staging folder is left");
    }

    #[cfg(unix)]
    #[test]
    fn an_added_file_takes_the_mode_of_its_entry() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new(BASE, NEW_FILES);
        // The base release's mode, whatever the umask made of it, so the file reads as one the user left.
        fs::set_permissions(fixture.folder.join("CLAUDE.md"), fs::Permissions::from_mode(0o644)).unwrap();
        fixture.run().unwrap();
        let mode = |path: &str| fs::metadata(joined(&fixture.folder, path)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode("scripts/run.sh"), 0o755);
        assert_eq!(mode("CLAUDE.md"), 0o644);
    }

    /// A file the update writes over keeps the permissions the user gave it, so a private one stays private. A file it adds takes the
    /// release's, and each side copy the permissions of the file it copies.
    #[cfg(unix)]
    #[test]
    fn a_file_written_over_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::edited();
        let mode = |path: &str| fs::metadata(joined(&fixture.folder, path)).unwrap().permissions().mode() & 0o777;
        for path in ["CLAUDE.md", ".claude/settings.json", "templates/letter.md"] {
            fs::set_permissions(joined(&fixture.folder, path), fs::Permissions::from_mode(0o600)).unwrap();
        }
        fixture.run().unwrap();
        assert_eq!(fixture.read("CLAUDE.md").as_deref(), Some(CLAUDE_MERGED));
        assert_eq!(mode("CLAUDE.md"), 0o600, "a merge");
        assert_eq!(mode("templates/letter.md"), 0o600, "a file the user never changed");
        assert_eq!(mode(".claude/settings.json"), 0o600, "a safety file upstream wins");
        assert_eq!(mode(".claude/settings.json.yours-1.8.0"), 0o600, "the copy of the user's file");
        assert_eq!(mode("docs/guide.md.update-1.8.0"), 0o644, "the copy of upstream's");
        assert_eq!(mode("scripts/run.sh"), 0o755, "a file the update adds");
    }

    /// A mode the release changes, such as a script made runnable, applies to a file whose mode the user left as the
    /// base had it, along with the release's text.
    #[cfg(unix)]
    #[test]
    fn a_mode_upstream_changes_applies_to_a_file_whose_mode_the_user_left() {
        use std::os::unix::fs::PermissionsExt;
        let mut fixture = Fixture::new(&[("tools/run.py", "print(1)\n")], &[("tools/run.py", "print(2)\n")]);
        fixture.new = zip_of(&[Entry::Mode("ai-job-search-1.8.0/tools/run.py", b"print(2)\n", 0o755)]);
        fixture.new_sha256 = sha256_hex(&fixture.new);
        let path = joined(&fixture.folder, "tools/run.py");
        // The base release's mode, whatever the umask made of it.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        fixture.run().unwrap();
        assert_eq!(fixture.read("tools/run.py").as_deref(), Some("print(2)\n"));
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o755);
    }

    #[test]
    fn an_untouched_file_is_updated_and_a_user_edit_merges() {
        let fixture = Fixture::edited();
        let report = fixture.run().unwrap();
        assert_eq!(fixture.read(".claude/commands/apply.md").as_deref(), Some("Apply to the role, then log it.\n"));
        assert_eq!(fixture.read("templates/letter.md").as_deref(), Some("Dear hiring team,\n"));
        assert_eq!(fixture.read("CLAUDE.md").as_deref(), Some(CLAUDE_MERGED), "the user's name and upstream's line both land");
        assert_eq!(report.updated, strings(&[".claude/commands/apply.md", "CLAUDE.md", "templates/letter.md"]));
        assert_eq!(report.added, strings(&[".claude/commands/review.md", "scripts/run.sh", "templates/new/intro.md"]));
        assert_eq!(report.deleted, strings(&[".claude/commands/old.md"]));
    }

    #[test]
    fn a_conflict_keeps_the_users_file_and_saves_upstreams_beside_it() {
        let fixture = Fixture::edited();
        let report = fixture.run().unwrap();
        assert_eq!(fixture.read("docs/guide.md").as_deref(), Some(GUIDE_OURS));
        assert_eq!(fixture.read("docs/guide.md.update-1.8.0").as_deref(), Some(GUIDE_NEW));
        assert_eq!(report.conflicts, [SideCopy { path: "docs/guide.md".to_owned(), side: "docs/guide.md.update-1.8.0".to_owned() }]);
    }

    #[test]
    fn a_security_file_takes_upstreams_copy_and_a_deleted_one_comes_back() {
        let fixture = Fixture::edited();
        let report = fixture.run().unwrap();
        assert_eq!(fixture.read(".claude/settings.json").as_deref(), Some(SETTINGS_NEW));
        assert_eq!(fixture.read(".claude/settings.json.yours-1.8.0").as_deref(), Some(SETTINGS_OURS));
        let wins = SideCopy { path: ".claude/settings.json".to_owned(), side: ".claude/settings.json.yours-1.8.0".to_owned() };
        assert_eq!(report.upstream_wins, [wins]);
        assert_eq!(fixture.read("tools/security_guards.py").as_deref(), Some("GUARD = 2\n"));
        assert_eq!(report.restored, strings(&["tools/security_guards.py"]));
    }

    #[test]
    fn a_security_file_comes_back_into_a_folder_the_user_removed() {
        let fixture = Fixture::new(BASE, NEW_FILES);
        fs::remove_dir_all(fixture.folder.join("tools")).unwrap();
        let report = fixture.run().unwrap();
        assert_eq!(fixture.read("tools/security_guards.py").as_deref(), Some("GUARD = 2\n"));
        assert_eq!(report.restored, strings(&["tools/security_guards.py"]));
    }

    #[test]
    fn a_file_the_user_deleted_stays_deleted() {
        let fixture = Fixture::edited();
        let report = fixture.run().unwrap();
        assert_eq!(fixture.read("docs/notes.md"), None);
        assert!(!report.updated.contains(&"docs/notes.md".to_owned()));
    }

    #[test]
    fn a_new_file_in_a_folder_the_user_removed_is_skipped() {
        let fixture = Fixture::new(BASE, &[NEW_FILES, &[("docs/more/extra.md", "More.\n")]].concat());
        fs::remove_dir_all(fixture.folder.join("docs")).unwrap();
        let report = fixture.run().unwrap();
        assert!(!fixture.folder.join("docs").exists());
        assert_eq!(report.skipped, [Skipped { path: "docs/more/extra.md".to_owned(), reason: SkipReason::RemovedFolder }]);
    }

    #[test]
    fn the_users_own_files_stay_byte_identical() {
        let fixture = Fixture::edited();
        let in_a_release = |path: &str| BASE.iter().chain(NEW_FILES).any(|(file, _)| path.starts_with(file));
        let own = |tree: BTreeMap<String, Vec<u8>>| -> BTreeMap<String, Vec<u8>> {
            tree.into_iter().filter(|(path, _)| !path.ends_with('/') && !path.starts_with(".jobdesk/") && !in_a_release(path)).collect()
        };
        let before = own(tree(&fixture.folder));
        assert!(before.contains_key("notes/private.md") && before.contains_key(".env") && before.contains_key("job_search_tracker.csv"));
        fixture.run().unwrap();
        assert_eq!(own(tree(&fixture.folder)), before);
    }

    #[test]
    fn jobdesk_is_never_touched_but_for_the_base() {
        // The planner's own guard is in `planner::tests`: the release zip refuses a `.jobdesk` entry before it.
        let fixture = Fixture::edited();
        fs::write(fixture.jobdesk().join("notes.md"), "Not the user's.\n").unwrap();
        fixture.run().unwrap();
        assert_eq!(fs::read_to_string(fixture.jobdesk().join("notes.md")).unwrap(), "Not the user's.\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_skipped_and_never_followed() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new(BASE, NEW_FILES);
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("letter.md"), "Dear team,\n").unwrap();
        fs::write(outside.path().join("apply.md"), "Apply to the role.\n").unwrap();
        fs::remove_dir_all(fixture.folder.join("templates")).unwrap();
        symlink(outside.path(), fixture.folder.join("templates")).unwrap();
        fs::remove_file(fixture.folder.join(".claude/commands/apply.md")).unwrap();
        symlink(outside.path().join("apply.md"), fixture.folder.join(".claude/commands/apply.md")).unwrap();
        let outside_before = tree(outside.path());
        let report = fixture.run().unwrap();
        let link = |path: &str| Skipped { path: path.to_owned(), reason: SkipReason::Link };
        assert_eq!(report.skipped, [link(".claude/commands/apply.md"), link("templates/letter.md"), link("templates/new/intro.md")]);
        assert_eq!(tree(outside.path()), outside_before, "nothing outside the folder changed");
        assert!(fs::symlink_metadata(fixture.folder.join("templates")).unwrap().file_type().is_symlink());
        assert_eq!(fixture.read("CLAUDE.md").as_deref(), Some(CLAUDE_NEW), "the rest of the update ran");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_where_a_side_copy_goes_skips_the_file() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::edited();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("target.md"), "Elsewhere.\n").unwrap();
        symlink(outside.path().join("target.md"), fixture.folder.join("docs/guide.md.update-1.8.0")).unwrap();
        let report = fixture.run().unwrap();
        assert!(report.skipped.contains(&Skipped { path: "docs/guide.md".to_owned(), reason: SkipReason::Link }));
        assert!(report.conflicts.is_empty());
        assert_eq!(fs::read_to_string(outside.path().join("target.md")).unwrap(), "Elsewhere.\n");
    }

    #[test]
    fn the_same_inputs_give_the_same_plan() {
        let fixture = Fixture::edited();
        let first = plan(&fixture.folder, &fixture.update()).unwrap();
        assert!(!first.steps.is_empty());
        assert_eq!(plan(&fixture.folder, &fixture.update()).unwrap(), first);
        let paths: Vec<&String> = first.steps.iter().map(|step| &step.path).collect();
        assert!(paths.is_sorted(), "{paths:?}");
    }

    #[test]
    fn a_failure_before_or_after_the_first_rename_keeps_the_folder_and_the_last_undo() {
        for at in [Moment::Staged, Moment::Swapped, Moment::Journaled, Moment::Applied(0), Moment::Applied(1)] {
            let mut fixture = Fixture::edited();
            fixture.run().unwrap();
            fixture.next("1.9.0", &[("CLAUDE.md", "# Job search, 1.9\n"), ("docs/new.md", "New.\n")]);
            let before = tree(&fixture.folder);
            let failed = update_with(&fixture.folder, &fixture.update(), &mut |moment| {
                if moment == at { Err(io::Error::other("a simulated failure")) } else { Ok(()) }
            });
            assert_eq!(failed, Err(UpdateError::Disk), "{at:?}");
            assert_eq!(tree(&fixture.folder), before, "{at:?}: the folder, its base and the last update's backup are as they were");
            assert_eq!(fixture.around(), ["Job search"], "{at:?}: the staging folder goes");
            assert!(can_undo(&fixture.folder), "{at:?}: the last update can still be undone");
            assert_eq!(undo(&fixture.folder).map(|reverted| reverted.kept), Ok(Vec::new()), "{at:?}");
            assert_eq!(fixture.read("CLAUDE.md").as_deref(), Some(CLAUDE_OURS), "{at:?}: the 1.8.0 update is undone");
        }
    }

    #[test]
    fn a_failure_after_a_rename_rolls_the_folder_back() {
        let fixture = Fixture::edited();
        let before = tree(&fixture.folder);
        let failed = update_with(&fixture.folder, &fixture.update(), &mut |moment| match moment {
            Moment::Applied(3) => Err(io::Error::other("a simulated failure")),
            _ => Ok(()),
        });
        assert_eq!(failed, Err(UpdateError::Disk));
        assert_eq!(tree(&fixture.folder), before);
        assert_eq!(fixture.around(), ["Job search"]);
    }

    #[test]
    fn a_stop_midway_rolls_back_from_the_journal_at_the_next_open() {
        let fixture = Fixture::edited();
        let before = tree(&fixture.folder);
        let stopped = panic::catch_unwind(AssertUnwindSafe(|| {
            update_with(&fixture.folder, &fixture.update(), &mut |moment| {
                assert_ne!(moment, Moment::Applied(4), "a simulated stop after some renames");
                Ok(())
            })
        }));
        assert!(stopped.is_err());
        assert!(fixture.jobdesk().join(JOURNAL).exists(), "the journal is left behind");
        assert_ne!(without_backup(tree(&fixture.folder)), without_backup(before.clone()), "some files were renamed in");
        assert_eq!(fixture.run(), Err(UpdateError::Interrupted), "no update runs over a stopped one");

        let reverted = recover(&fixture.folder).unwrap().unwrap();
        assert!(reverted.kept.is_empty(), "{reverted:?}");
        assert!(!reverted.reverted.is_empty());
        assert_eq!(without_backup(tree(&fixture.folder)), without_backup(before));
        assert!(!fixture.jobdesk().join(JOURNAL).exists());
        assert_eq!(fixture.around(), ["Job search"], "the stopped update's staging folder goes");
        assert_eq!(recover(&fixture.folder), Ok(None), "nothing more to roll back");
        assert!(fixture.run().is_ok(), "the update runs again");
    }

    #[test]
    fn undo_puts_everything_back() {
        let fixture = Fixture::edited();
        let before = without_backup(tree(&fixture.folder));
        let report = fixture.run().unwrap();
        let reverted = undo(&fixture.folder).unwrap();
        assert!(reverted.kept.is_empty(), "{reverted:?}");
        let mut expected: Vec<String> = [report.updated, report.added, report.deleted, report.restored].concat();
        expected.extend(report.conflicts.into_iter().map(|copy| copy.side));
        for wins in report.upstream_wins {
            expected.extend([wins.path, wins.side]);
        }
        expected.sort();
        assert_eq!(reverted.reverted, expected);
        assert_eq!(without_backup(tree(&fixture.folder)), before, "files, side copies, made folders and the base are as they were");
        assert!(!fixture.folder.join("templates/new").exists());
        assert!(!can_undo(&fixture.folder));
        assert_eq!(undo(&fixture.folder), Err(UpdateError::NothingToUndo));
    }

    /// Undo notes the release it undid, so it is not applied again, and the next update that lands takes the note away.
    #[test]
    fn undo_notes_the_release_it_undid_until_the_next_update() {
        let mut fixture = Fixture::edited();
        assert_eq!(skipped(&fixture.folder), None);
        fixture.run().unwrap();
        assert_eq!(skipped(&fixture.folder), None, "an update notes nothing");
        undo(&fixture.folder).unwrap();
        let undone = InstalledSource { version: NEW.to_owned(), sha256: fixture.new_sha256.clone() };
        assert_eq!(skipped(&fixture.folder), Some(undone));

        let note = fixture.jobdesk().join(BACKUP).join(SKIPPED);
        fs::write(&note, "{\"version\": \"1.8\", \"sha256\": \"x\"}").unwrap();
        assert_eq!(skipped(&fixture.folder), None, "a note that does not read skips nothing");
        fs::remove_file(&note).unwrap();
        #[cfg(unix)]
        {
            let elsewhere = fixture.folder.join("notes/elsewhere.json");
            fs::write(&elsewhere, installed_source::record(&InstalledSource { version: NEW.to_owned(), sha256: fixture.new_sha256.clone() }).unwrap()).unwrap();
            std::os::unix::fs::symlink(&elsewhere, &note).unwrap();
            assert_eq!(skipped(&fixture.folder), None, "a link is never read");
            fs::remove_file(&note).unwrap();
        }

        undo(&fixture.folder).unwrap_err();
        fixture.run().unwrap();
        undo(&fixture.folder).unwrap();
        assert!(skipped(&fixture.folder).is_some());
        assert_eq!(fixture.run(), Err(UpdateError::Undone), "the release undone is not applied again");
        fixture.instead("1.8.1", NEW_FILES);
        fixture.run().unwrap();
        assert_eq!(skipped(&fixture.folder), None, "the next update's backup takes the note's place");
        fixture.next("1.9.0", NEW_FILES);
        fixture.run().unwrap();
        assert_eq!(skipped(&fixture.folder), None);
    }

    /// Undo notes the release before it puts anything back: a note it cannot write refuses the Undo with the folder
    /// untouched, so a folder is never undone without its note.
    #[test]
    fn an_undo_that_cannot_note_the_release_changes_nothing() {
        let fixture = Fixture::edited();
        fixture.run().unwrap();
        // A folder that is not empty where the note goes, which the note cannot replace.
        let note = fixture.jobdesk().join(BACKUP).join(SKIPPED);
        fs::create_dir_all(note.join("in the way")).unwrap();
        let before = tree(&fixture.folder);
        assert_eq!(undo(&fixture.folder), Err(UpdateError::Unnoted));
        assert_eq!(tree(&fixture.folder), before, "nothing was put back");
        assert!(can_undo(&fixture.folder), "the Undo can be asked for again");
        fs::remove_dir_all(&note).unwrap();
        undo(&fixture.folder).unwrap();
        assert!(skipped(&fixture.folder).is_some());
    }

    /// An Undo that stopped partway, its note written and only some files put back, is found by a look that changes
    /// nothing, and Undo again finishes it. An Undo that finished, even one that kept files, is not taken for one.
    #[test]
    fn an_undo_that_stopped_partway_is_found_and_finished() {
        let fixture = Fixture::edited();
        let before = without_backup(tree(&fixture.folder));
        fixture.run().unwrap();
        assert!(!undo_unfinished(&fixture.folder), "an update alone is no unfinished undo");
        let backup = fixture.jobdesk().join(BACKUP);
        let record = journal::read(&backup.join(RECORD)).unwrap().unwrap();
        // What a stop midway leaves: the note, and the last half of the record's files put back, as Undo goes last first.
        note_skipped(&backup, &undone_release(&fixture.folder, &record).unwrap()).unwrap();
        let half = Journal { ops: record.ops[record.ops.len() / 2..].to_vec(), ..record.clone() };
        journal::revert_all(&fixture.folder, &fixture.jobdesk(), &half).unwrap();
        assert_ne!(without_backup(tree(&fixture.folder)), before, "the folder is mixed");

        let looked = tree(&fixture.folder);
        assert!(undo_unfinished(&fixture.folder));
        assert_eq!(tree(&fixture.folder), looked, "the look changes nothing");
        let reverted = undo(&fixture.folder).unwrap();
        assert!(reverted.kept.is_empty(), "{reverted:?}");
        assert_eq!(without_backup(tree(&fixture.folder)), before, "the retry finishes the revert");
        assert!(!undo_unfinished(&fixture.folder));

        let kept = Fixture::edited();
        kept.run().unwrap();
        kept.write("CLAUDE.md", "Mine, after the update.\n");
        assert!(!undo(&kept.folder).unwrap().kept.is_empty());
        assert!(can_undo(&kept.folder), "the record of the kept file stays, though Jobdesk does not offer Undo for it again");
        assert!(!undo_unfinished(&kept.folder), "an Undo that finished and kept a file is not unfinished");
    }

    /// The release to skip comes from the update's own record, so a base record edited since still gets it noted.
    #[test]
    fn undo_notes_the_release_from_its_record_when_the_base_does_not_read() {
        let fixture = Fixture::edited();
        fixture.run().unwrap();
        fixture.write(".jobdesk/source.json", "{ edited }");
        undo(&fixture.folder).unwrap();
        assert_eq!(skipped(&fixture.folder), Some(InstalledSource { version: NEW.to_owned(), sha256: fixture.new_sha256.clone() }));
    }

    /// From end to end over the disk: an update, its Undo, then the same pin from a later answer of the feed is skipped,
    /// and a newer pin updates.
    #[test]
    fn a_pass_after_undo_skips_the_same_release() {
        use std::sync::{Arc, Mutex};

        use crate::folder_update::application::{FolderUpdate, Outcome, Stage, UndoOutcome};
        use crate::folder_update::infrastructure::DiskFolder;
        use crate::shared_kernel::download::DownloadFailure;
        use crate::updates::application::{LatestFeed, SourcePin, Version};

        /// Hands over the zip it holds, and counts the downloads.
        struct Serve(Mutex<(Vec<u8>, usize)>);
        impl crate::folder_update::domain::ZipFetcher for Serve {
            fn fetch(&self, _url: &str, _name: &str, _progress: &mut dyn FnMut(u64)) -> Result<Vec<u8>, DownloadFailure> {
                let mut held = self.0.lock().unwrap();
                held.1 += 1;
                Ok(held.0.clone())
            }
        }

        let mut fixture = Fixture::edited();
        let serve = Arc::new(Serve(Mutex::new((fixture.new.clone(), 0))));
        let disk = Arc::new(DiskFolder::default());
        let mut desk = FolderUpdate::new(disk.clone(), disk.clone(), serve.clone(), disk, "0.2.0", Arc::new(|| {}));
        // This Jobdesk holds the folder open, which keeps out only other Jobdesks.
        desk.opened(fixture.folder.clone());
        let jobdesk = Version::parse("0.2.0").unwrap();
        let feed = |version: &str, sha256: &str| LatestFeed::for_tests(jobdesk, Some(SourcePin::parse(version, sha256).unwrap()));
        let settle = |desk: &mut FolderUpdate| {
            while desk.is_working() {
                desk.poll();
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        };
        desk.start(fixture.folder.clone(), &feed(NEW, &fixture.new_sha256), true);
        settle(&mut desk);
        assert!(matches!(desk.stage(), Stage::Ended(Outcome::Updated(_))), "{:?}", desk.stage());
        desk.undo(fixture.folder.clone(), true);
        settle(&mut desk);
        assert!(matches!(desk.stage(), Stage::Undid(UndoOutcome::Undone(_))), "{:?}", desk.stage());
        desk.start(fixture.folder.clone(), &feed(NEW, &fixture.new_sha256), true);
        settle(&mut desk);
        assert_eq!(desk.stage(), &Stage::Ended(Outcome::Skipped));
        assert_eq!(serve.0.lock().unwrap().1, 1, "the same release is not downloaded again");

        fixture.next("1.9.0", NEW_FILES);
        serve.0.lock().unwrap().0 = fixture.new.clone();
        desk.start(fixture.folder.clone(), &feed("1.9.0", &fixture.new_sha256), true);
        settle(&mut desk);
        assert!(matches!(desk.stage(), Stage::Ended(Outcome::Updated(_))), "a newer release updates: {:?}", desk.stage());
    }

    /// A file changed since the update is kept, and the record stays, naming only it and its original. The updater can
    /// still undo it when called, which the end of this test checks, but Jobdesk does not offer that Undo again.
    #[test]
    fn undo_keeps_a_file_changed_since_the_update_and_its_record() {
        let fixture = Fixture::edited();
        fixture.run().unwrap();
        fixture.write(".claude/commands/apply.md", "Apply, my way.\n");
        fixture.write("docs/guide.md.update-1.8.0", "Read and edited.\n");
        let reverted = undo(&fixture.folder).unwrap();
        assert_eq!(reverted.kept, strings(&[".claude/commands/apply.md", "docs/guide.md.update-1.8.0"]));
        assert_eq!(fixture.read(".claude/commands/apply.md").as_deref(), Some("Apply, my way.\n"));
        assert_eq!(fixture.read("docs/guide.md.update-1.8.0").as_deref(), Some("Read and edited.\n"));
        assert_eq!(fixture.read("CLAUDE.md").as_deref(), Some(CLAUDE_OURS), "the rest is undone");

        // The record, read after the undo, still names the original of each file kept.
        let record = journal::read(&fixture.jobdesk().join(BACKUP).join(RECORD)).unwrap().expect("the record stays");
        let paths: Vec<&str> = record.ops.iter().map(|op| op.path.as_str()).collect();
        assert_eq!(paths, [".claude/commands/apply.md", "docs/guide.md.update-1.8.0"], "only the files kept");
        assert_eq!(backed_up(&fixture, ".claude/commands/apply.md"), "Apply to the role.\n", "the original stays in the backup");
        assert!(can_undo(&fixture.folder));

        // Called again once the update's copy is back, the updater puts the original back. No screen asks for it.
        fixture.write(".claude/commands/apply.md", "Apply to the role, then log it.\n");
        let again = undo(&fixture.folder).unwrap();
        assert_eq!(again.reverted, strings(&[".claude/commands/apply.md"]));
        assert_eq!(again.kept, strings(&["docs/guide.md.update-1.8.0"]));
        assert_eq!(fixture.read(".claude/commands/apply.md").as_deref(), Some("Apply to the role.\n"));
    }

    /// A safety file the user changed after the update stays, and so does the copy of theirs saved beside it, which then
    /// holds their settings from before the update.
    #[test]
    fn undo_keeps_the_copy_beside_a_safety_file_it_keeps() {
        let fixture = Fixture::edited();
        fixture.run().unwrap();
        fixture.write(".claude/settings.json", "{\n  \"allow\": [\"Grep\", \"Edit\"]\n}\n");
        let reverted = undo(&fixture.folder).unwrap();
        assert_eq!(reverted.kept, strings(&[".claude/settings.json", ".claude/settings.json.yours-1.8.0"]));
        assert_eq!(fixture.read(".claude/settings.json").as_deref(), Some("{\n  \"allow\": [\"Grep\", \"Edit\"]\n}\n"));
        assert_eq!(fixture.read(".claude/settings.json.yours-1.8.0").as_deref(), Some(SETTINGS_OURS), "the user's settings stay beside it");
        assert_eq!(fixture.read("CLAUDE.md").as_deref(), Some(CLAUDE_OURS), "the rest is undone");
    }

    /// The copy of the user's safety file stays beside it while Undo keeps the file for any reason, and goes with it
    /// when it is put back: a copy the user changed keeps the file too.
    #[test]
    fn undo_keeps_or_reverts_a_safety_file_and_its_copy_together() {
        let fixture = Fixture::edited();
        fixture.run().unwrap();
        fixture.write(".claude/settings.json.yours-1.8.0", "Merged by hand.\n");
        let reverted = undo(&fixture.folder).unwrap();
        assert_eq!(reverted.kept, strings(&[".claude/settings.json", ".claude/settings.json.yours-1.8.0"]));
        assert_eq!(fixture.read(".claude/settings.json").as_deref(), Some(SETTINGS_NEW));
        assert_eq!(fixture.read(".claude/settings.json.yours-1.8.0").as_deref(), Some("Merged by hand.\n"));

        let fixture = Fixture::edited();
        fixture.run().unwrap();
        fs::remove_file(fixture.folder.join(".claude/settings.json.yours-1.8.0")).unwrap();
        let reverted = undo(&fixture.folder).unwrap();
        assert!(reverted.kept.is_empty(), "{reverted:?}");
        assert_eq!(fixture.read(".claude/settings.json").as_deref(), Some(SETTINGS_OURS), "a copy the user deleted holds nothing back");
    }

    /// The rollback of a stopped update keeps the copy beside a safety file it keeps, as Undo does.
    #[test]
    fn a_rollback_keeps_the_copy_beside_a_safety_file_it_keeps() {
        let fixture = stopped();
        fixture.write(".claude/settings.json", "Edited after the stop.\n");
        let reverted = recover(&fixture.folder).unwrap().unwrap();
        assert_eq!(reverted.kept, strings(&[".claude/settings.json", ".claude/settings.json.yours-1.8.0"]));
        assert_eq!(fixture.read(".claude/settings.json.yours-1.8.0").as_deref(), Some(SETTINGS_OURS));
        assert_eq!(fixture.read(".claude/commands/review.md"), None, "the rest is rolled back");
    }

    #[test]
    fn an_existing_side_copy_that_differs_skips_the_file() {
        // The user's notes in an older copy: the next update's backup would replace the only other copy of them.
        let fixture = Fixture::edited();
        fixture.write("docs/guide.md.update-1.8.0", "An older copy, with my notes.\n");
        let report = fixture.run().unwrap();
        assert_eq!(fixture.read("docs/guide.md.update-1.8.0").as_deref(), Some("An older copy, with my notes.\n"), "never overwritten");
        assert_eq!(fixture.read("docs/guide.md").as_deref(), Some(GUIDE_OURS));
        assert!(report.conflicts.is_empty(), "{report:?}");
        assert!(report.skipped.contains(&Skipped { path: "docs/guide.md".to_owned(), reason: SkipReason::InTheWay }), "{report:?}");

        // A copy that already holds the new version stays as it is, and the conflict is reported as usual.
        let fixture = Fixture::edited();
        fixture.write("docs/guide.md.update-1.8.0", GUIDE_NEW);
        let report = fixture.run().unwrap();
        assert_eq!(report.conflicts, [SideCopy { path: "docs/guide.md".to_owned(), side: "docs/guide.md.update-1.8.0".to_owned() }]);
    }

    /// A safety file whose `.yours-` name is taken by a file with other bytes keeps the user's file in place, so its
    /// safety update is listed as left alone and never shown as applied.
    #[test]
    fn a_safety_file_whose_copy_name_is_taken_is_skipped_whole() {
        let fixture = Fixture::edited();
        fixture.write(".claude/settings.json.yours-1.8.0", "Notes on my settings.\n");
        let report = fixture.run().unwrap();
        assert_eq!(fixture.read(".claude/settings.json").as_deref(), Some(SETTINGS_OURS), "the user's file stays");
        assert_eq!(fixture.read(".claude/settings.json.yours-1.8.0").as_deref(), Some("Notes on my settings.\n"));
        assert!(report.upstream_wins.is_empty(), "{report:?}");
        assert!(report.skipped.contains(&Skipped { path: ".claude/settings.json".to_owned(), reason: SkipReason::InTheWay }), "{report:?}");
    }

    #[test]
    fn the_backup_holds_only_the_last_update() {
        let mut fixture = Fixture::edited();
        fixture.run().unwrap();
        let next: Vec<(&str, &str)> =
            NEW_FILES.iter().map(|&(path, text)| if path == "templates/letter.md" { (path, "Dear hiring manager,\n") } else { (path, text) }).collect();
        fixture.next("1.9.0", &next);
        let report = fixture.run().unwrap();
        assert_eq!(report.updated, strings(&["templates/letter.md"]));
        let backup = tree(&fixture.jobdesk().join(BACKUP));
        let files: Vec<&String> = backup.keys().filter(|path| !path.ends_with('/')).collect();
        assert_eq!(files, ["files/0", "files/1", "files/2", "update.json"], "flat, one plain number for each original");
        assert_eq!(backed_up(&fixture, "templates/letter.md"), "Dear hiring team,\n");
        let old: InstalledSource = serde_json::from_str(&backed_up(&fixture, ".jobdesk/source.json")).unwrap();
        assert_eq!(old.version, NEW);
        assert_eq!(backup["files/1"], fixture.base, "the old base zip");
        assert_eq!(journal::read(&fixture.jobdesk().join(BACKUP).join(RECORD)).unwrap().unwrap().version, "1.9.0");
    }

    #[test]
    fn a_second_jobdesk_is_busy() {
        let fixture = Fixture::edited();
        let before = tree(&fixture.folder);
        let _held = hold_alone(&fixture.jobdesk()).unwrap();
        assert_eq!(fixture.run(), Err(UpdateError::Busy));
        assert_eq!(undo(&fixture.folder), Err(UpdateError::Busy));
        assert_eq!(recover(&fixture.folder), Err(UpdateError::Busy));
        assert_eq!(tree(&fixture.folder), before);
    }

    /// Another Jobdesk that has the folder open holds its lock shared, as a second handle of the lock file does here:
    /// its tasks, Profile's Save and portal changes may write the folder at any moment, so nothing updates, undoes or
    /// rolls it back meanwhile.
    #[test]
    fn a_folder_another_jobdesk_has_open_is_busy() {
        let fixture = Fixture::edited();
        fixture.run().unwrap();
        let other = File::options().read(true).write(true).create(true).truncate(false).open(fixture.jobdesk().join(".open")).unwrap();
        other.lock_shared().unwrap();
        assert!(matches!(hold_open(&fixture.folder), Open::Held(_)), "a Jobdesk opens it beside the other");
        let before = tree(&fixture.folder);
        assert_eq!(fixture.run(), Err(UpdateError::Busy));
        assert_eq!(undo(&fixture.folder), Err(UpdateError::Busy));
        assert_eq!(recover(&fixture.folder), Err(UpdateError::Busy));
        assert_eq!(tree(&fixture.folder), before);
        drop(other);
        assert!(undo(&fixture.folder).is_ok(), "once it has closed the folder");
    }

    /// The gate reads the base and the skip note before the download, and another Jobdesk may change either before
    /// this one holds the folder alone: the update reads both again then, and changes nothing when they moved.
    #[test]
    fn an_update_whose_base_moved_since_the_gate_changes_nothing() {
        // Another Jobdesk brought the folder to this very release meanwhile.
        let fixture = Fixture::edited();
        fixture.run().unwrap();
        let before = tree(&fixture.folder);
        assert_eq!(fixture.run(), Err(UpdateError::Superseded));
        assert_eq!(tree(&fixture.folder), before, "the last update's backup stays");

        // Another Jobdesk set the folder to some other release meanwhile.
        let fixture = Fixture::edited();
        let other = release("1.7.4", BASE);
        let record = installed_source::record(&InstalledSource { version: "1.7.4".to_owned(), sha256: sha256_hex(&other) }).unwrap();
        fs::write(fixture.jobdesk().join(installed_source::ZIP), &other).unwrap();
        fs::write(fixture.jobdesk().join(installed_source::RECORD), record).unwrap();
        let before = tree(&fixture.folder);
        assert_eq!(fixture.run(), Err(UpdateError::Busy));
        assert_eq!(tree(&fixture.folder), before);
    }

    #[test]
    fn an_update_the_user_undid_since_the_gate_changes_nothing() {
        // Another Jobdesk applied this release and the user undid it there, while this one downloaded it.
        let fixture = Fixture::edited();
        fixture.run().unwrap();
        undo(&fixture.folder).unwrap();
        let before = tree(&fixture.folder);
        assert_eq!(fixture.run(), Err(UpdateError::Undone));
        assert_eq!(tree(&fixture.folder), before, "the skipped release is not applied again");
    }

    #[test]
    fn a_version_whose_record_would_not_read_back_changes_nothing() {
        let mut fixture = Fixture::edited();
        let before = tree(&fixture.folder);
        fixture.version = "1.8";
        assert_eq!(fixture.run(), Err(UpdateError::BadVersion));
        assert_eq!(tree(&fixture.folder), before);
    }

    #[test]
    fn a_release_that_does_not_match_changes_nothing() {
        let mut fixture = Fixture::edited();
        let before = tree(&fixture.folder);
        fixture.new_sha256 = sha256_hex(b"another release");
        assert!(matches!(fixture.run(), Err(UpdateError::BadRelease(_))));
        assert_eq!(tree(&fixture.folder), before);
        assert_eq!(fixture.around(), ["Job search"]);
    }

    #[test]
    fn a_rename_that_changes_only_letter_case_keeps_the_file() {
        // Each assertion holds on a disk that tells case apart and on one that does not: the update never writes either
        // name of the pair, so the names in the folder are the same on both.
        let fixture = Fixture::new(&[("docs/Guide.md", "Guide\n")], &[("docs/guide.md", "Guide, upstream\n")]);
        let report = fixture.run().unwrap();
        assert_eq!(names_in(&fixture.folder.join("docs")), ["Guide.md", "guide.md.update-1.8.0"]);
        assert_eq!(fixture.read("docs/Guide.md").as_deref(), Some("Guide\n"), "the user's file is byte-identical");
        assert_eq!(fixture.read("docs/guide.md.update-1.8.0").as_deref(), Some("Guide, upstream\n"));
        let conflict = SideCopy { path: "docs/guide.md".to_owned(), side: "docs/guide.md.update-1.8.0".to_owned() };
        assert_eq!(report, Report { version: NEW.to_owned(), conflicts: vec![conflict], ..Report::default() });
        assert_eq!(installed_source::read(&fixture.folder).unwrap().0.version, NEW, "the base moves, as after any conflict");
        // Undo removes the side copy and leaves the file.
        undo(&fixture.folder).unwrap();
        assert_eq!(names_in(&fixture.folder.join("docs")), ["Guide.md"]);
    }

    #[test]
    fn a_rename_of_only_the_case_of_a_folder_keeps_its_files() {
        let base = [("Docs/a.md", "A\n"), ("Docs/b.md", "B\n")];
        let fixture = Fixture::new(&base, &[("docs/a.md", "A, upstream\n"), ("docs/b.md", "B, upstream\n")]);
        let report = fixture.run().unwrap();
        for (path, text) in base {
            assert_eq!(fixture.read(path).as_deref(), Some(text), "{path}");
        }
        assert_eq!(fixture.read("docs/a.md.update-1.8.0").as_deref(), Some("A, upstream\n"));
        assert_eq!(fixture.read("docs/b.md.update-1.8.0").as_deref(), Some("B, upstream\n"));
        let side = |path: &str| SideCopy { path: path.to_owned(), side: format!("{path}.update-1.8.0") };
        assert_eq!(report.conflicts, [side("docs/a.md"), side("docs/b.md")]);
        assert!(report.deleted.is_empty() && report.added.is_empty() && report.skipped.is_empty(), "{report:?}");
    }

    #[test]
    fn a_renamed_in_case_file_the_user_has_as_upstream_or_deleted_needs_nothing() {
        // Upstream renamed it and kept its text: the user's file already holds it.
        let fixture = Fixture::new(&[("docs/Guide.md", "Guide\n")], &[("docs/guide.md", "Guide\n")]);
        let report = fixture.run().unwrap();
        assert_eq!(report, Report { version: NEW.to_owned(), ..Report::default() });
        assert_eq!(names_in(&fixture.folder.join("docs")), ["Guide.md"]);

        // The user deleted it: it stays deleted.
        let fixture = Fixture::new(&[("docs/Guide.md", "Guide\n")], &[("docs/guide.md", "Guide, upstream\n")]);
        fs::remove_file(fixture.folder.join("docs/Guide.md")).unwrap();
        assert_eq!(fixture.run().unwrap(), Report { version: NEW.to_owned(), ..Report::default() });
        assert!(names_in(&fixture.folder.join("docs")).is_empty());
    }

    #[test]
    fn a_renamed_in_case_file_whose_text_upstream_kept_keeps_the_users_edit() {
        let fixture = Fixture::new(&[("docs/Guide.md", "Guide\n")], &[("docs/guide.md", "Guide\n")]);
        fixture.write("docs/Guide.md", "Guide, mine\n");
        assert_eq!(fixture.run().unwrap(), Report { version: NEW.to_owned(), ..Report::default() }, "upstream left the text alone");
        assert_eq!(names_in(&fixture.folder.join("docs")), ["Guide.md"]);
        assert_eq!(fixture.read("docs/Guide.md").as_deref(), Some("Guide, mine\n"));
    }

    #[test]
    fn a_renamed_in_case_file_that_differs_only_in_line_endings_needs_nothing() {
        // The user's copy holds upstream's change already, in CRLF line endings.
        let fixture = Fixture::new(&[("docs/Guide.md", "one\ntwo\n")], &[("docs/guide.md", "one\nTWO\n")]);
        fixture.write("docs/Guide.md", "one\r\nTWO\r\n");
        assert_eq!(fixture.run().unwrap(), Report { version: NEW.to_owned(), ..Report::default() }, "line endings alone");
        assert_eq!(names_in(&fixture.folder.join("docs")), ["Guide.md"]);
    }

    #[test]
    fn a_security_file_renamed_in_case_alone_is_a_conflict() {
        let fixture = Fixture::new(&[(".claude/Settings.json", "{}\n")], &[(".claude/settings.json", SETTINGS_NEW)]);
        let report = fixture.run().unwrap();
        assert_eq!(fixture.read(".claude/Settings.json").as_deref(), Some("{}\n"), "the user's file stays in place");
        let side = ".claude/settings.json.update-1.8.0";
        assert_eq!(fixture.read(side).as_deref(), Some(SETTINGS_NEW));
        assert_eq!(report.conflicts, [SideCopy { path: ".claude/settings.json".to_owned(), side: side.to_owned() }]);
        assert!(report.upstream_wins.is_empty());
    }

    #[test]
    fn a_renamed_in_case_file_whose_side_name_is_taken_is_skipped() {
        // A folder of the user's where the side copy goes.
        let fixture = Fixture::new(&[("docs/Guide.md", "Guide\n")], &[("docs/guide.md", "Guide, upstream\n")]);
        fs::create_dir_all(fixture.folder.join("docs/guide.md.update-1.8.0")).unwrap();
        let report = fixture.run().unwrap();
        assert_eq!(report.skipped, [Skipped { path: "docs/guide.md".to_owned(), reason: SkipReason::InTheWay }]);
        assert!(report.conflicts.is_empty());
        assert_eq!(fixture.read("docs/Guide.md").as_deref(), Some("Guide\n"));

        // A file of the new release where the side copy goes.
        let new = [("docs/guide.md", "Guide, upstream\n"), ("docs/guide.md.update-1.8.0", "Shipped.\n")];
        let fixture = Fixture::new(&[("docs/Guide.md", "Guide\n")], &new);
        let report = fixture.run().unwrap();
        assert_eq!(report.skipped, [Skipped { path: "docs/guide.md".to_owned(), reason: SkipReason::InTheWay }]);
        assert_eq!(fixture.read("docs/Guide.md").as_deref(), Some("Guide\n"));
        assert_eq!(fixture.read("docs/guide.md.update-1.8.0").as_deref(), Some("Shipped.\n"), "the release's own file is added");
    }

    #[test]
    fn a_file_saved_just_before_its_rename_stops_the_update_and_stays() {
        let fixture = Fixture::edited();
        let mut before = tree(&fixture.folder);
        // CLAUDE.md is renamed in after the first few files: an editor saves it while they go in.
        let failed = update_with(&fixture.folder, &fixture.update(), &mut |moment| {
            if moment == Moment::Applied(2) {
                fixture.write("CLAUDE.md", "Saved in an editor.\n");
            }
            Ok(())
        });
        assert_eq!(failed, Err(UpdateError::Changed));
        assert_eq!(fixture.read("CLAUDE.md").as_deref(), Some("Saved in an editor.\n"));
        before.insert("CLAUDE.md".to_owned(), b"Saved in an editor.\n".to_vec());
        assert_eq!(tree(&fixture.folder), before, "the rest is rolled back");
        assert_eq!(fixture.around(), ["Job search"]);
    }

    #[test]
    fn a_rollback_that_keeps_a_changed_file_keeps_its_original_in_the_backup() {
        let fixture = Fixture::edited();
        // The first file goes in, is edited meanwhile, and a later file fails.
        let failed = update_with(&fixture.folder, &fixture.update(), &mut |moment| match moment {
            Moment::Applied(5) => {
                fixture.write(".claude/commands/apply.md", "Edited meanwhile.\n");
                Err(io::Error::other("a simulated failure"))
            }
            _ => Ok(()),
        });
        assert_eq!(failed, Err(UpdateError::Kept(strings(&[".claude/commands/apply.md"]))));
        assert!(!fixture.jobdesk().join(JOURNAL).exists(), "the rollback is done");
        assert_eq!(backed_up(&fixture, ".claude/commands/apply.md"), "Apply to the role.\n", "the record says where it is");
        assert_eq!(fixture.read(".claude/commands/apply.md").as_deref(), Some("Edited meanwhile.\n"));
        assert_eq!(fixture.read("CLAUDE.md").as_deref(), Some(CLAUDE_OURS), "the rest is rolled back");
        let saved = fixture.jobdesk().join(BACKUP).join(FILES).join("0");
        assert_eq!(fs::read_to_string(saved).ok().as_deref(), Some("Apply to the role.\n"), "the original is not lost");
    }

    /// The fixture after an update that stopped once its first files were in, and the journal it left.
    fn stopped() -> Fixture {
        let fixture = Fixture::edited();
        let stopped = panic::catch_unwind(AssertUnwindSafe(|| {
            update_with(&fixture.folder, &fixture.update(), &mut |moment| {
                assert_ne!(moment, Moment::Applied(4), "a simulated stop after some renames");
                Ok(())
            })
        }));
        assert!(stopped.is_err());
        fixture
    }

    #[cfg(unix)]
    #[test]
    fn a_stopped_update_whose_backup_is_gone_never_blocks_the_folder() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(outside.path().join(FILES)).unwrap();
        fs::write(outside.path().join(FILES).join("0"), SECRET).unwrap();
        for linked in [false, true] {
            let fixture = stopped();
            fs::remove_dir_all(fixture.jobdesk().join(BACKUP)).unwrap();
            if linked {
                symlink(outside.path(), fixture.jobdesk().join(BACKUP)).unwrap();
            }
            // The files whose originals were in the backup and that the update had replaced or deleted, and the copy of
            // the user's safety file beside one, which then holds the only copy of their settings.
            let lost = strings(&[".claude/commands/apply.md", ".claude/commands/old.md", ".claude/settings.json", ".claude/settings.json.yours-1.8.0"]);
            assert_eq!(recover(&fixture.folder), Err(UpdateError::Kept(lost)), "linked: {linked}");
            assert!(!fixture.jobdesk().join(JOURNAL).exists(), "linked: {linked}");
            assert!(fixture.jobdesk().join("update-stopped.json").is_file(), "linked: {linked}: the journal is kept aside");
            assert_eq!(fixture.read(".claude/commands/review.md"), None, "linked: {linked}: what needs no original is rolled back");
            assert_eq!(fs::read_to_string(outside.path().join(FILES).join("0")).unwrap(), SECRET, "linked: {linked}");
            assert_eq!(recover(&fixture.folder), Ok(None), "linked: {linked}: nothing more to roll back");
            assert!(fixture.run().is_ok(), "linked: {linked}: the folder updates again");
            assert_eq!(fs::read_to_string(outside.path().join(FILES).join("0")).unwrap(), SECRET, "linked: {linked}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_stopped_update_whose_backup_files_are_not_a_folder_never_blocks_the_folder() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("0"), SECRET).unwrap();
        for linked in [true, false] {
            let fixture = stopped();
            let files = fixture.jobdesk().join(BACKUP).join(FILES);
            fs::remove_dir_all(&files).unwrap();
            if linked {
                symlink(outside.path(), &files).unwrap();
            } else {
                fs::write(&files, "not a folder").unwrap();
            }
            assert!(is_real_dir(&fixture.jobdesk().join(BACKUP)), "the backup itself is a real folder");
            let lost = strings(&[".claude/commands/apply.md", ".claude/commands/old.md", ".claude/settings.json", ".claude/settings.json.yours-1.8.0"]);
            assert_eq!(recover(&fixture.folder), Err(UpdateError::Kept(lost)), "linked: {linked}");
            assert!(!fixture.jobdesk().join(JOURNAL).exists(), "linked: {linked}: the journal is set aside");
            assert_eq!(fs::read_to_string(outside.path().join("0")).unwrap(), SECRET, "linked: {linked}");
            assert!(fixture.run().is_ok(), "linked: {linked}: the folder updates again");
        }
    }

    #[test]
    fn a_rollback_that_keeps_a_file_leaves_the_record_of_its_original() {
        let fixture = stopped();
        fixture.write(".claude/commands/apply.md", "Edited after the stop.\n");
        let reverted = recover(&fixture.folder).unwrap().unwrap();
        assert_eq!(reverted.kept, strings(&[".claude/commands/apply.md"]));
        assert!(!fixture.jobdesk().join(JOURNAL).exists());
        assert_eq!(backed_up(&fixture, ".claude/commands/apply.md"), "Apply to the role.\n", "the record names the original");
        assert_eq!(fixture.read(".claude/commands/apply.md").as_deref(), Some("Edited after the stop.\n"));
    }

    #[test]
    fn a_journal_that_does_not_read_is_set_aside() {
        let fixture = Fixture::edited();
        fs::write(fixture.jobdesk().join(JOURNAL), "not a journal").unwrap();
        assert_eq!(fixture.run(), Err(UpdateError::Interrupted));
        assert_eq!(recover(&fixture.folder), Err(UpdateError::BadJournal));
        assert_eq!(fs::read_to_string(fixture.jobdesk().join("update-stopped.json")).unwrap(), "not a journal");
        assert_eq!(recover(&fixture.folder), Ok(None));
        assert!(fixture.run().is_ok(), "the folder updates again");
    }

    const SECRET: &str = "A fictional private key.\n";
    const PLANTED: &str = "Planted by a run.\n";

    /// A folder updated once, with `x.md` planted in it, and a directory outside it that holds `SECRET` as `name`.
    fn planted(name: &str) -> (Fixture, tempfile::TempDir) {
        let fixture = Fixture::new(BASE, NEW_FILES);
        fixture.run().unwrap();
        fixture.write("x.md", PLANTED);
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join(name), SECRET).unwrap();
        (fixture, outside)
    }

    /// Writes a record of the last update whose one operation puts `SECRET`, found at `backup`, back as `x.md`, as a
    /// run's tools could.
    fn forge(fixture: &Fixture, backup: &str) {
        let op = Op {
            path: "x.md".to_owned(),
            before: Some(sha256_hex(SECRET.as_bytes())),
            backup: Some(backup.to_owned()),
            wrote: Some(sha256_hex(PLANTED.as_bytes())),
        };
        let record = Journal { version: NEW.to_owned(), sha256: None, undone: false, folders: Vec::new(), ops: vec![op] };
        journal::write(&fixture.jobdesk().join(BACKUP), RECORD, &record).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn undo_never_follows_a_link_inside_the_backup() {
        use std::os::unix::fs::symlink;
        // A link below `files`, reached through a path in the record.
        let (fixture, outside) = planted("secret");
        symlink(outside.path(), fixture.jobdesk().join(BACKUP).join(FILES).join("l")).unwrap();
        forge(&fixture, "files/l/secret");
        let _ = undo(&fixture.folder);
        assert_eq!(fs::read_to_string(outside.path().join("secret")).unwrap(), SECRET, "a link below files");
        assert_eq!(fixture.read("x.md").as_deref(), Some(PLANTED));

        // `files` itself a link.
        let (fixture, outside) = planted("0");
        fs::remove_dir_all(fixture.jobdesk().join(BACKUP).join(FILES)).unwrap();
        symlink(outside.path(), fixture.jobdesk().join(BACKUP).join(FILES)).unwrap();
        forge(&fixture, "0");
        let _ = undo(&fixture.folder);
        assert_eq!(fs::read_to_string(outside.path().join("0")).unwrap(), SECRET, "files as a link");
        assert_eq!(fixture.read("x.md").as_deref(), Some(PLANTED));

        // `backup` itself a link.
        let (fixture, outside) = planted("0");
        let elsewhere = tempfile::tempdir().unwrap();
        fs::create_dir(elsewhere.path().join(FILES)).unwrap();
        fs::rename(outside.path().join("0"), elsewhere.path().join(FILES).join("0")).unwrap();
        fs::remove_dir_all(fixture.jobdesk().join(BACKUP)).unwrap();
        symlink(elsewhere.path(), fixture.jobdesk().join(BACKUP)).unwrap();
        forge(&fixture, "0");
        let _ = undo(&fixture.folder);
        assert_eq!(fs::read_to_string(elsewhere.path().join(FILES).join("0")).unwrap(), SECRET, "backup as a link");
        assert_eq!(fixture.read("x.md").as_deref(), Some(PLANTED));
    }

    #[cfg(unix)]
    #[test]
    fn undo_never_follows_a_linked_folder_above_a_path() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new(BASE, NEW_FILES);
        fixture.run().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("x.md"), PLANTED).unwrap();
        symlink(outside.path(), fixture.folder.join("linked")).unwrap();
        // The update made the file, so an undo would remove it.
        let op = Op { path: "linked/x.md".to_owned(), before: None, backup: None, wrote: Some(sha256_hex(PLANTED.as_bytes())) };
        journal::write(
            &fixture.jobdesk().join(BACKUP),
            RECORD,
            &Journal { version: NEW.to_owned(), sha256: None, undone: false, folders: Vec::new(), ops: vec![op] },
        )
        .unwrap();
        let reverted = undo(&fixture.folder).unwrap();
        assert_eq!(reverted.kept, ["linked/x.md"]);
        assert_eq!(fs::read_to_string(outside.path().join("x.md")).unwrap(), PLANTED);
    }

    #[cfg(unix)]
    #[test]
    fn undo_never_removes_a_folder_through_a_link() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new(BASE, NEW_FILES);
        fixture.run().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(outside.path().join("empty")).unwrap();
        symlink(outside.path(), fixture.folder.join("linked")).unwrap();
        let record =
            Journal { version: NEW.to_owned(), sha256: None, undone: false, folders: vec!["linked".to_owned(), "linked/empty".to_owned()], ops: Vec::new() };
        journal::write(&fixture.jobdesk().join(BACKUP), RECORD, &record).unwrap();
        let _ = undo(&fixture.folder);
        assert!(outside.path().join("empty").is_dir(), "the empty folder outside stays");
        assert!(fs::symlink_metadata(fixture.folder.join("linked")).unwrap().file_type().is_symlink(), "the link stays");
    }
}
