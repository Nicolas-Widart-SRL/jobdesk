//! The plan of a folder update: both release zips checked and read, then each path of either looked at in the folder
//! and decided, in path order. Only those paths are read: the user's own files, and `.jobdesk`, never are.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::folder_update::domain::{Action, Plan, Side, SkipReason, Skipped, Step, Update, UpdateError, decide, side_name};
use crate::folder_update::infrastructure::disk::{Found, in_jobdesk, look};
use crate::shared_kernel::installed_source::plain_version;
use crate::shared_kernel::release_zip::{Entry, ZipFailure, compared, read_verified};

/// What the update does with each file of the base release and the new one in `folder`. Nothing is written.
pub fn plan(folder: &Path, update: &Update) -> Result<Plan, UpdateError> {
    if !plain_version(update.version) {
        return Err(UpdateError::BadVersion);
    }
    let base = files(read_verified(update.base.zip, update.base.sha256).map_err(UpdateError::BadBase)?);
    let new = files(read_verified(update.new.zip, update.new.sha256).map_err(UpdateError::BadRelease)?);
    let base_folders: BTreeSet<String> = base.keys().flat_map(|path| folders_of(path)).collect();
    let (paths, renamed) = paths_of(&base, &new)?;
    let old_names: BTreeSet<&String> = renamed.values().copied().collect();
    let mut plan = Plan { version: update.version.to_owned(), steps: Vec::new(), skipped: Vec::new() };
    for path in paths.iter().copied() {
        let skip = |reason| Skipped { path: path.clone(), reason };
        if old_names.contains(path) {
            // Planned under its new name.
            continue;
        }
        if let Some(old) = renamed.get(path) {
            let (Some((base, _)), Some(theirs)) = (base.get(old.as_str()), new.get(path.as_str())) else {
                return Err(UpdateError::BadRelease(ZipFailure::BadArchive));
            };
            match renamed_in_case(folder, path, old, base, theirs, update.version, &paths)? {
                Ok(Some(step)) => plan.steps.push(step),
                Ok(None) => {}
                Err(reason) => plan.skipped.push(skip(reason)),
            }
            continue;
        }
        let seen = look(folder, path).map_err(|_| UpdateError::Disk)?;
        let (ours, ours_mode) = match seen.found {
            Found::File(bytes, mode) => (Some(bytes), mode),
            Found::Missing => (None, 0o644),
            Found::Link => {
                plan.skipped.push(skip(SkipReason::Link));
                continue;
            }
            Found::Other => {
                plan.skipped.push(skip(SkipReason::InTheWay));
                continue;
            }
        };
        let parent_kept = !seen.missing.iter().any(|folder| base_folders.contains(folder));
        let theirs = new.get(path.as_str());
        let action =
            decide(path, base.get(path.as_str()).map(|(bytes, _)| bytes.as_slice()), ours.as_deref(), theirs.map(|(bytes, _)| bytes.as_slice()), parent_kept);
        let base_mode = base.get(path.as_str()).map(|(_, mode)| *mode);
        // Bytes that need no change still take a mode the release changed, on a file whose mode the user left: the
        // user's own bytes go back with it. Else the base moves on, and a later update takes the old mode for the user's.
        let mode_only = cfg!(unix) && base_mode == Some(ours_mode) && theirs.is_some_and(|(_, mode)| *mode != ours_mode);
        let action = match (action, &ours) {
            (Action::Keep, Some(bytes)) if mode_only => Action::Write(bytes.clone()),
            (action, _) => action,
        };
        let side = match &action {
            Action::Keep => continue,
            Action::Skip => {
                plan.skipped.push(skip(SkipReason::RemovedFolder));
                continue;
            }
            Action::Conflict { theirs: copy } | Action::UpstreamWins { ours: copy, .. } => {
                let upstream_wins = matches!(action, Action::UpstreamWins { .. });
                // Upstream's copy takes the release's mode, and the user's keeps its own.
                let mode = if upstream_wins { ours_mode } else { theirs.map_or(0o644, |(_, mode)| *mode) };
                match side_of(folder, path, update.version, upstream_wins, (copy, mode), &paths)? {
                    Ok((side, _)) => Some(side),
                    Err(reason) => {
                        plan.skipped.push(skip(reason));
                        continue;
                    }
                }
            }
            Action::Write(_) | Action::Delete | Action::Restore(_) => None,
        };
        let mode = kept_mode(ours.is_some().then_some(ours_mode), base_mode, theirs.map(|(_, mode)| *mode));
        plan.steps.push(Step { path: path.clone(), action, ours, mode, side, folders: seen.missing });
    }
    Ok(plan)
}

/// The mode a file the update writes gets, merged as its text is: on Unix, the folder's file's own, `ours`, when the
/// user changed it from the base release's, `base`, so a merge never widens who may read a private file, and else the
/// new release's, `theirs`, so a script the release makes runnable runs. Windows keeps no such bits, so there it is
/// always the release's.
fn kept_mode(ours: Option<u32>, base: Option<u32>, theirs: Option<u32>) -> u32 {
    match (ours, theirs) {
        (Some(ours), _) if cfg!(unix) && base != Some(ours) => ours,
        (_, Some(theirs)) => theirs,
        (Some(ours), None) => ours,
        (None, None) => 0o644,
    }
}

/// The files of a release by their path with `/` separators, each with its bytes and mode.
fn files(entries: Vec<Entry>) -> Files {
    entries
        .into_iter()
        .map(|entry| {
            let path = entry.path.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
            (path, (entry.bytes, entry.mode))
        })
        .collect()
}

/// The files of a release by their path, each with its bytes and mode.
type Files = BTreeMap<String, (Vec<u8>, u32)>;

/// The paths of either release the update plans, none in `.jobdesk`, and the files upstream renamed in letter case or
/// accent composition alone, such as `docs/Guide.md` to `docs/guide.md`, each new name with its old one. macOS and
/// Windows take both names for one file, so the update never deletes, renames or writes the user's file under either:
/// see `renamed_in_case`. Each zip refuses two such names, so a clash is always one name only in the base and one only
/// in the new release. Any other refuses the whole update, before anything changes.
fn paths_of<'a>(base: &'a Files, new: &'a Files) -> Result<(BTreeSet<&'a String>, BTreeMap<&'a String, &'a String>), UpdateError> {
    let paths: BTreeSet<&String> = base.keys().chain(new.keys()).filter(|path| !in_jobdesk(path)).collect();
    let mut by_name: BTreeMap<String, Vec<&String>> = BTreeMap::new();
    for path in &paths {
        by_name.entry(compared(Path::new(path.as_str()))).or_default().push(path);
    }
    let mut renamed = BTreeMap::new();
    for same in by_name.into_values().filter(|same| same.len() > 1) {
        let only_in =
            |files: &Files, other: &Files| same.iter().copied().filter(|path| files.contains_key(*path) && !other.contains_key(*path)).collect::<Vec<_>>();
        match (only_in(base, new).as_slice(), only_in(new, base).as_slice()) {
            ([old], [new_name]) if same.len() == 2 => renamed.insert(*new_name, *old),
            _ => return Err(UpdateError::BadRelease(ZipFailure::BadArchive)),
        };
    }
    Ok((paths, renamed))
}

/// What the update does with `path`, a file upstream renamed from `old` in letter case or accent composition alone. It
/// never deletes, renames or writes the user's file under either name, whichever the disk tells apart. The user's file
/// is the one under the old name, or else under the new one:
///
/// - none: the user deleted it, and it stays deleted, as `decide` says;
/// - what `decide` keeps, such as the same text as upstream's, line endings aside, or a text upstream left as it was:
///   nothing to do;
/// - anything else, which would write the user's file: a `Conflict` under the new name, with upstream's copy saved
///   beside it. The base then moves to the
///   new release, as after any conflict, and the user's file stays a user version from then on.
///
/// One of the `SECURITY_FILES` renamed so would take `UpstreamWins`, which writes upstream's copy in the user's file's
/// place: under a name that differs only in case, that needs a rename on disk, which the update does not do. So it is a
/// `Conflict` too, and upstream's copy of a security file renamed in case alone lands beside the user's, not in its
/// place. A deleted one comes back under the new name, as `decide` says, since no file of the user's is there.
fn renamed_in_case(
    folder: &Path,
    path: &str,
    old: &str,
    base: &[u8],
    theirs: &(Vec<u8>, u32),
    version: &str,
    paths: &BTreeSet<&String>,
) -> Result<Result<Option<Step>, SkipReason>, UpdateError> {
    let disk = |_| UpdateError::Disk;
    let mut ours = None;
    for name in [old, path] {
        match look(folder, name).map_err(disk)?.found {
            Found::File(bytes, _) => {
                ours = Some(bytes);
                break;
            }
            Found::Missing => {}
            Found::Link => return Ok(Err(SkipReason::Link)),
            Found::Other => return Ok(Err(SkipReason::InTheWay)),
        }
    }
    let (theirs, mode) = theirs;
    let Some(ours) = ours else {
        return Ok(match decide(path, Some(base), None, Some(theirs), true) {
            action @ Action::Restore(_) => {
                let folders = look(folder, path).map_err(disk)?.missing;
                Ok(Some(Step { path: path.to_owned(), action, ours: None, mode: *mode, side: None, folders }))
            }
            _ => Ok(None),
        });
    };
    // The rules of any other file, line endings included: only an action that would write the user's file becomes a
    // side copy, and the user's file is never written.
    if decide(path, Some(base), Some(&ours), Some(theirs), true) == Action::Keep {
        return Ok(Ok(None));
    }
    let (side, folders) = match side_of(folder, path, version, false, (theirs, *mode), paths)? {
        Ok(found) => found,
        Err(reason) => return Ok(Err(reason)),
    };
    let action = Action::Conflict { theirs: theirs.clone() };
    Ok(Ok(Some(Step { path: path.to_owned(), action, ours: Some(ours), mode: *mode, side: Some(side), folders })))
}

/// The copy saved beside `path` in a conflict, of `copy`'s bytes with its mode, and the folders above it that are
/// missing, outermost first. No name for the version, a name a release also uses, a link, a folder in its place, or a
/// file there with other bytes is a reason to skip the file: a file at that name is never overwritten, since it may
/// hold the user's own notes, and the backup of the update after next would replace the only other copy of them.
fn side_of(
    folder: &Path,
    path: &str,
    version: &str,
    upstream_wins: bool,
    (copy, mode): (&[u8], u32),
    paths: &BTreeSet<&String>,
) -> Result<Result<(Side, Vec<String>), SkipReason>, UpdateError> {
    let Some(name) = side_name(path, version, upstream_wins) else { return Ok(Err(SkipReason::BadVersion)) };
    if paths.contains(&name) {
        return Ok(Err(SkipReason::InTheWay));
    }
    let seen = look(folder, &name).map_err(|_| UpdateError::Disk)?;
    let existing = match seen.found {
        Found::File(bytes, _) if bytes == copy => Some(bytes),
        Found::File(..) | Found::Other => return Ok(Err(SkipReason::InTheWay)),
        Found::Missing => None,
        Found::Link => return Ok(Err(SkipReason::Link)),
    };
    Ok(Ok((Side { path: name, existing, mode }, seen.missing)))
}

/// The folders above `path`, outermost first.
fn folders_of(path: &str) -> Vec<String> {
    path.match_indices('/').map(|(at, _)| path[..at].to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(paths: &[&str]) -> Files {
        paths.iter().map(|path| ((*path).to_owned(), (b"text".to_vec(), 0o644))).collect()
    }

    #[test]
    fn jobdesk_is_never_planned_whatever_a_release_holds() {
        // The release zip already refuses such entries: this is the planner's own guard.
        let (base, new) = (release(&[".jobdesk/source.zip", "a.md"]), release(&[".JobDesk/source.json", "b.md", "docs/.jobdesk"]));
        let paths: Vec<&String> = paths_of(&base, &new).unwrap().0.into_iter().collect();
        assert_eq!(paths, ["a.md", "b.md", "docs/.jobdesk"]);
    }

    #[test]
    fn a_name_renamed_in_case_or_composition_alone_is_found_with_its_old_name() {
        let (base, new) = (release(&["docs/Guide.md", "caf\u{e9}.md", "a.md"]), release(&["docs/guide.md", "cafe\u{301}.md", "a.md", "Docs.md"]));
        let (_, renamed) = paths_of(&base, &new).unwrap();
        let pairs: Vec<(&str, &str)> = renamed.into_iter().map(|(new, old)| (new.as_str(), old.as_str())).collect();
        assert_eq!(pairs, [("cafe\u{301}.md", "caf\u{e9}.md"), ("docs/guide.md", "docs/Guide.md")]);
        let (base, new) = (release(&["Docs/a.md", "Docs/b.md"]), release(&["docs/a.md", "docs/b.md"]));
        assert_eq!(paths_of(&base, &new).unwrap().1.len(), 2, "each file of a folder renamed in case alone");
    }

    #[test]
    fn any_other_clash_refuses_the_update() {
        // Not from a checked zip, which refuses two such names: the planner refuses them again.
        let refused = Err(UpdateError::BadRelease(ZipFailure::BadArchive));
        assert_eq!(paths_of(&release(&["a.md"]), &release(&["a.md", "A.md"])).map(|_| ()), refused, "both names in the new release");
        assert_eq!(paths_of(&release(&["a.md", "A.md"]), &release(&["B.md"])).map(|_| ()), refused, "both in the base");
    }

    #[test]
    fn the_folders_above_a_path_come_outermost_first() {
        assert_eq!(folders_of("a/b/c.md"), ["a", "a/b"]);
        assert!(folders_of("c.md").is_empty());
    }
}
