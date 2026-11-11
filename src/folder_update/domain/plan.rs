//! A folder update before and after it runs: what it starts from, the plan of what it does with each file it changes,
//! the report the notice shows, and what an undo or a rollback left.

use crate::folder_update::domain::Action;
use crate::shared_kernel::release_zip::ZipFailure;

/// A release zip and the SHA-256 its bytes must match.
#[derive(Debug, Clone, Copy)]
pub struct Release<'a> {
    pub zip: &'a [u8],
    pub sha256: &'a str,
}

/// What an update starts from: the release the folder was set up from or last updated to, as the gate read it, the new
/// release, whose SHA-256 is the signed one, and the new release's version, such as `1.8.0`. The update reads the
/// folder's base again once it holds the folder alone, and changes nothing unless it is still the one the gate read.
#[derive(Debug, Clone, Copy)]
pub struct Update<'a> {
    pub base: Release<'a>,
    /// The base's version, as the gate read it with `base`.
    pub base_version: &'a str,
    pub new: Release<'a>,
    pub version: &'a str,
}

/// Why the update leaves a file of a release alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The file, a folder above it in the job search folder, or the copy that would go beside it is a link. Nothing is
    /// written, renamed or deleted through one.
    Link,
    /// Upstream added the file into a folder the user has removed.
    RemovedFolder,
    /// The file needs a copy beside it, and the version gives it no name.
    BadVersion,
    /// A folder stands where the file goes, a file where a folder above it goes, or a folder, another file of a release
    /// or a file with other bytes where its side copy goes.
    InTheWay,
}

/// A file of a release the update leaves alone, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub path: String,
    pub reason: SkipReason,
}

/// The copy an update saves beside a file in a conflict: upstream's, or the user's when upstream wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Side {
    /// Relative to the folder, with `/` separators.
    pub path: String,
    /// What is at that name now: the copy's own bytes, as an earlier update or the user left them. `None` when nothing
    /// is. A file there with other bytes is never overwritten: the file is skipped instead.
    pub existing: Option<Vec<u8>>,
    /// The mode the copy gets: the new release's for upstream's copy, the user's file's for theirs.
    pub mode: u32,
}

/// What the update does with one file it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Relative to the folder, with `/` separators.
    pub path: String,
    /// Never `Keep` or `Skip`: those files are not steps.
    pub action: Action,
    /// The folder's file when the plan was made. `None` when it is absent.
    pub ours: Option<Vec<u8>>,
    /// The mode the file gets: on Unix the folder's file's own when it is there, else the one the new release gives it.
    pub mode: u32,
    /// The copy saved beside the file, for a `Conflict` or an `UpstreamWins`.
    pub side: Option<Side>,
    /// The folders above the file that are missing, outermost first. The update makes them when it writes the file.
    pub folders: Vec<String>,
}

/// Everything a folder update does, in path order. The same inputs always give the same plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The new release's version.
    pub version: String,
    pub steps: Vec<Step>,
    pub skipped: Vec<Skipped>,
}

/// A file in a conflict and the name of the copy saved beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideCopy {
    pub path: String,
    pub side: String,
}

/// What an update did, for the notice: the paths of each outcome, in path order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub version: String,
    /// Files that were there and now hold upstream's change, merged with the user's where both changed.
    pub updated: Vec<String>,
    /// Files new upstream.
    pub added: Vec<String>,
    /// Files upstream removed that the user had left as they were.
    pub deleted: Vec<String>,
    /// The user's file stays, and upstream's copy is beside it.
    pub conflicts: Vec<SideCopy>,
    /// Upstream's copy of a file that guards what Claude may do is in place, and the user's is beside it.
    pub upstream_wins: Vec<SideCopy>,
    /// Files that guard what Claude may do, which the user had deleted, back with upstream's fix.
    pub restored: Vec<String>,
    pub skipped: Vec<Skipped>,
}

/// What an undo, or the rollback of an update that stopped midway, did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reverted {
    /// The files of the folder put back as they were before the update, or removed when the update made them.
    pub reverted: Vec<String>,
    /// The files left as they are, because they changed since the update wrote them.
    pub kept: Vec<String>,
}

/// Why an update, an undo or a rollback did not run, or stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// The folder's `.jobdesk` is missing, a link or not a folder.
    NoBase,
    /// The base zip was refused.
    BadBase(ZipFailure),
    /// The new release zip was refused.
    BadRelease(ZipFailure),
    /// The version is not three whole numbers, so the folder's new base would never read back. Nothing was changed.
    BadVersion,
    /// Another Jobdesk has this folder open, or is updating, undoing or rolling it back now, or it changed the folder's
    /// base since the gate read it. Nothing was changed.
    Busy,
    /// Since the gate read it, the folder's base became this release or a newer one. Nothing was changed.
    Superseded,
    /// Since the gate read the note, the user undid this very release. Nothing was changed.
    Undone,
    /// An update stopped midway, and its rollback has not run yet.
    Interrupted,
    /// The record of an update that stopped midway does not read, so it cannot be rolled back.
    BadJournal,
    /// A file the update changes changed while the update ran, and stays as the user left it. Nothing was changed, or
    /// what was is rolled back.
    Changed,
    /// No update is recorded, or its backup has gone.
    NothingToUndo,
    /// Undo could not note the release it would undo, so a later pass could apply it again. Nothing was changed.
    Unnoted,
    /// A file could not be read or written. When that happened after the first file was replaced, the folder was rolled
    /// back.
    Disk,
    /// The update failed after the first file was replaced and was rolled back, but these files changed meanwhile and
    /// stay as they are. Their originals stay in the backup, which keeps the update's record of where each one is.
    Kept(Vec<String>),
    /// The update failed after the first file was replaced, and its rollback failed too. The record of the update
    /// stays, and the next rollback tries again.
    RollbackFailed,
}

impl Plan {
    /// The report of the update, once it ran to the end.
    pub fn report(&self) -> Report {
        let mut report = Report { version: self.version.clone(), skipped: self.skipped.clone(), ..Report::default() };
        for step in &self.steps {
            let path = step.path.clone();
            let side = || SideCopy { path: path.clone(), side: step.side.as_ref().map(|side| side.path.clone()).unwrap_or_default() };
            match &step.action {
                Action::Write(_) if step.ours.is_some() => report.updated.push(path.clone()),
                Action::Write(_) => report.added.push(path.clone()),
                Action::Delete => report.deleted.push(path.clone()),
                Action::Conflict { .. } => report.conflicts.push(side()),
                Action::UpstreamWins { .. } => report.upstream_wins.push(side()),
                Action::Restore(_) => report.restored.push(path.clone()),
                Action::Keep | Action::Skip => {}
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(path: &str, action: Action, ours: Option<&[u8]>, side: Option<&str>) -> Step {
        let side = side.map(|side| Side { path: side.to_owned(), existing: None, mode: 0o644 });
        Step { path: path.to_owned(), action, ours: ours.map(<[u8]>::to_vec), mode: 0o644, side, folders: Vec::new() }
    }

    #[test]
    fn the_report_lists_each_outcome() {
        let plan = Plan {
            version: "1.8.0".to_owned(),
            steps: vec![
                step("added.md", Action::Write(b"new".to_vec()), None, None),
                step("changed.md", Action::Write(b"new".to_vec()), Some(b"old"), None),
                step("gone.md", Action::Delete, Some(b"old"), None),
                step("mine.md", Action::Conflict { theirs: b"new".to_vec() }, Some(b"mine"), Some("mine.md.update-1.8.0")),
                step(
                    ".claude/settings.json",
                    Action::UpstreamWins { ours: b"mine".to_vec(), theirs: b"new".to_vec() },
                    Some(b"mine"),
                    Some(".claude/settings.json.yours-1.8.0"),
                ),
                step("tools/security_guards.py", Action::Restore(b"new".to_vec()), None, None),
            ],
            skipped: vec![Skipped { path: "linked/a.md".to_owned(), reason: SkipReason::Link }],
        };
        let side = |path: &str, side: &str| SideCopy { path: path.to_owned(), side: side.to_owned() };
        assert_eq!(
            plan.report(),
            Report {
                version: "1.8.0".to_owned(),
                updated: vec!["changed.md".to_owned()],
                added: vec!["added.md".to_owned()],
                deleted: vec!["gone.md".to_owned()],
                conflicts: vec![side("mine.md", "mine.md.update-1.8.0")],
                upstream_wins: vec![side(".claude/settings.json", ".claude/settings.json.yours-1.8.0")],
                restored: vec!["tools/security_guards.py".to_owned()],
                skipped: vec![Skipped { path: "linked/a.md".to_owned(), reason: SkipReason::Link }],
            }
        );
    }
}
