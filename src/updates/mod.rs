//! Updating Jobdesk itself: the feed check, the download, the signature check and the restart that applies it.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::shared_kernel::https::HttpsTransport;
use crate::updates::application::{ApplyFailed, ApplyFailure, InstallBlock, Platform, RestartPlan, ServiceEvent, SourcePinReader, Updates};
#[cfg(target_os = "macos")]
use crate::updates::infrastructure::install_macos::reopen;
use crate::updates::infrastructure::{GithubSourcePins, GithubUpdates, IdleUpdates, ScriptedUpdates, UpdateKey, cleanup, install_windows};

/// The folder in the app's data folder that holds the downloads, and the note of an apply that failed.
fn downloads(data: &Path) -> PathBuf {
    data.join("updates")
}

/// Updates from the releases on GitHub. `data` is the app's data folder: downloads go to its `updates/` only.
/// `current` is the version this copy runs, `block` what `install_block` says, and `auto` the saved switch.
/// `repaint` wakes the window when an answer arrives. A build with no update key is blocked whatever `block` says.
/// Nothing here starts `claude`, uses the API key or writes a run log.
pub fn github(data: &Path, current: &str, block: Option<InstallBlock>, auto: bool, repaint: Arc<dyn Fn() + Send + Sync>) -> Updates {
    let key = UpdateKey::embedded();
    let block = if key.is_none() { Some(InstallBlock::NoKey) } else { block };
    let service = GithubUpdates::new(Arc::new(HttpsTransport::new(current)), downloads(data), Platform::current(), key, repaint);
    Updates::new(Box::new(service), current, block, auto)
}

/// The `ai-job-search` release this copy vouches for, read from the feed on GitHub and checked against the update key
/// built into it. `current` is the version this copy runs. `None` in a build with no update key, which can vouch for
/// no release. Nothing here starts `claude`, uses the API key or writes a run log.
pub fn source_pins(current: &str) -> Option<Arc<dyn SourcePinReader>> {
    let key = UpdateKey::embedded()?;
    Some(Arc::new(GithubSourcePins::new(Arc::new(HttpsTransport::new(current)), Some(key))))
}

/// Why this copy cannot replace itself, if it cannot: a build with no update key, a copy no installer put in place,
/// or on macOS a bundle in a folder Jobdesk cannot write. It tries to make a folder beside the bundle, and removes it.
pub fn install_block() -> Option<InstallBlock> {
    if UpdateKey::embedded().is_none() {
        return Some(InstallBlock::NoKey);
    }
    let Ok(exe) = std::env::current_exe() else { return Some(InstallBlock::NotInstalled) };
    match Platform::current() {
        #[cfg(target_os = "macos")]
        Some(Platform::MacArm) => infrastructure::install_macos::block(&exe),
        Some(Platform::WindowsX64) => install_windows::block(&exe),
        _ => Some(InstallBlock::NoBuild),
    }
}

/// Empties `updates/` in `data`: what a past session downloaded is never applied by a later one. Call it at launch,
/// before `last_apply_failure`, which it leaves in place. The folder of another Jobdesk that still runs stays.
/// Errors are ignored, and links are removed, never followed. On macOS it also removes what a Restart to update that
/// died partway left beside the bundle.
pub fn clear_downloads(data: &Path) {
    cleanup::clear(&downloads(data));
    #[cfg(target_os = "macos")]
    if let Ok(exe) = std::env::current_exe() {
        infrastructure::install_macos::clear_leftovers(&exe);
    }
}

/// Why the last Restart to update did not apply, once: the next call gives `None`.
pub fn last_apply_failure(data: &Path) -> Option<ApplyFailed> {
    cleanup::take_apply_failure(&downloads(data))
}

/// Applies `plan` once the window has closed, then opens Jobdesk again with `args`, which `relaunch_args` gives.
/// It checks the file against the release's signature once more first. On macOS it swaps the bundle and opens the
/// new copy. On Windows it starts the setup, which closes this copy and opens the new one, and returns at once.
/// When it cannot apply, it notes why in `updates/` of `data` for the next launch and opens this copy again.
/// A program that is no longer where it ran from is never started: the note is all that is left.
/// `moved` is `data` when `JOBDESK_DATA_DIR` chose it: the copy that opens next gets the same folder, whichever way it opens.
pub fn finish(data: &Path, moved: Option<&Path>, plan: &RestartPlan, args: &[OsString]) {
    let note = |failure| cleanup::record_apply_failure(&downloads(data), ApplyFailed { version: plan.version, failure });
    let Ok(exe) = std::env::current_exe() else { return note(ApplyFailure::Start) };
    let key = UpdateKey::embedded();
    let applied = match Platform::current() {
        #[cfg(target_os = "macos")]
        Some(Platform::MacArm) => infrastructure::install_macos::apply(plan, &exe, key.as_ref()),
        Some(Platform::WindowsX64) => install_windows::apply(plan, &exe, args, moved, key.as_ref()),
        _ => Err(ApplyFailure::NotInstalled),
    };
    let setup_reopens = Platform::current() == Some(Platform::WindowsX64);
    after_apply(applied, setup_reopens, &note, &|| exe.is_file(), &mut || reopen(&exe, args, moved));
}

/// What follows an apply that ended as `applied`: the note for the next launch, and the copy to open again.
/// `setup_reopens` is a Windows setup that started, which opens the new copy itself. `present` tells whether the
/// program is still a file, as late as it can, and `reopen` starts it.
fn after_apply(
    applied: Result<(), ApplyFailure>,
    setup_reopens: bool,
    note: &dyn Fn(ApplyFailure),
    present: &dyn Fn() -> bool,
    reopen: &mut dyn FnMut() -> std::io::Result<()>,
) {
    match applied {
        // The setup opens the new copy itself once it has installed it.
        Ok(()) if setup_reopens => {}
        Ok(()) => {
            if !present() || reopen().is_err() {
                note(ApplyFailure::Start);
            }
        }
        // Noted before this copy opens again, so its launch finds the note.
        Err(failure) => {
            note(failure);
            if present() {
                let _ = reopen();
            }
        }
    }
}

/// Starts the program at `exe` again with `args`, on the `moved` data folder if there is one.
#[cfg(not(target_os = "macos"))]
fn reopen(exe: &Path, args: &[OsString], moved: Option<&Path>) -> std::io::Result<()> {
    let mut program = std::process::Command::new(exe);
    crate::shared_kernel::process_tree::no_window(&mut program);
    crate::shared_kernel::data_folder::pass_on_data_folder(&mut program, moved);
    program.args(args).spawn().map(drop)
}

/// An update that never hears back: a check started with it stays running. For previews and snapshots.
/// `current` is the version this copy runs, such as `0.1.0`, and `auto` is the saved switch for the automatic check.
pub fn idle_updates(current: &str, auto: bool) -> Updates {
    Updates::new(Box::new(IdleUpdates), current, None, auto)
}

/// An update that answers from `events` at once, so a test or a snapshot reaches any state without the network.
/// The numbers of the download events in `events` do not matter: each download stamps its own.
/// `platform` is the build this copy counts as, so a state reads the same on every computer: `None` is a computer
/// with no published build.
pub fn scripted_updates(current: &str, events: Vec<ServiceEvent>, block: Option<InstallBlock>, auto: bool, platform: Option<Platform>) -> Updates {
    Updates::with_platform(Box::new(ScriptedUpdates::new(events)), current, block, auto, platform)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::NaiveDate;

    use super::*;
    use crate::updates::application::{DownloadEnd, DownloadId, Release, Version};

    fn ready_slip(platform: Platform) -> String {
        let release = Release {
            version: Version { major: 0, minor: 2, patch: 0 },
            released: None,
            notes: String::new(),
            size: 10,
            signature: "sig".into(),
            pins: Vec::new(),
        };
        let script =
            vec![ServiceEvent::Checked(Ok(release)), ServiceEvent::Ended { id: DownloadId(0), end: DownloadEnd::Done(PathBuf::from("/updates/Jobdesk")) }];
        let mut updates = scripted_updates("0.1.0", script, None, true, Some(platform));
        let now = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap().and_hms_opt(9, 0, 0).unwrap();
        updates.poll(now);
        updates.poll(now);
        updates.download();
        updates.poll(now);
        updates.view().notes.unwrap().slip.unwrap()
    }

    #[test]
    fn a_scripted_state_reads_the_same_on_every_computer() {
        assert!(ready_slip(Platform::MacArm).contains("installs 0.2.0"));
        assert!(ready_slip(Platform::WindowsX64).contains("the installer shows its progress"));
    }

    /// A computer with no published build starts no check, whose feed would answer only that it has no build. About
    /// says so, and the rail keeps the version, through the same updates a launch makes.
    #[test]
    fn a_computer_with_no_published_build_never_asks_the_feed() {
        let script = vec![ServiceEvent::Checked(Err(crate::updates::application::Failure::NoBuild))];
        let mut updates = scripted_updates("0.1.0", script, None, true, None);
        let now = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap().and_hms_opt(9, 0, 0).unwrap();
        updates.poll(now);
        updates.check_now(now);
        updates.poll(now);
        let view = updates.view();
        assert_eq!(view.rail_word, ("0.1.0".to_owned(), false), "no check failed");
        assert_eq!(view.about.tag, None);
        assert_eq!(view.about.check_now, None);
        let help: Vec<_> = view.about.help.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(help, ["Jobdesk publishes no build for this computer, so it cannot update itself."]);
        assert_eq!(updates.next_due(now), None, "and the window never wakes for one");
    }

    /// What `after_apply` did, in order: `note <failure>`, `present` and `reopen`.
    fn steps(applied: Result<(), ApplyFailure>, setup_reopens: bool, present: bool, reopens: bool) -> Vec<String> {
        let log = std::cell::RefCell::new(Vec::new());
        after_apply(
            applied,
            setup_reopens,
            &|failure| log.borrow_mut().push(format!("note {failure:?}")),
            &|| {
                log.borrow_mut().push("present".to_owned());
                present
            },
            &mut || {
                log.borrow_mut().push("reopen".to_owned());
                if reopens { Ok(()) } else { Err(std::io::Error::other("fictional")) }
            },
        );
        log.into_inner()
    }

    #[test]
    fn a_failed_apply_is_noted_before_this_copy_opens_again() {
        assert_eq!(steps(Err(ApplyFailure::Replace), false, true, true), ["note Replace", "present", "reopen"]);
        assert_eq!(steps(Err(ApplyFailure::Replace), true, true, false), ["note Replace", "present", "reopen"], "on Windows too");
    }

    #[test]
    fn a_program_that_is_no_longer_a_file_is_never_started() {
        assert_eq!(steps(Err(ApplyFailure::BadSignature), false, false, true), ["note BadSignature", "present"]);
        assert_eq!(steps(Ok(()), false, false, true), ["present", "note Start"], "the note is all that is left");
    }

    #[test]
    fn a_started_windows_setup_opens_the_new_copy_itself() {
        assert!(steps(Ok(()), true, true, true).is_empty());
    }

    #[test]
    fn a_good_swap_that_does_not_open_notes_start() {
        assert_eq!(steps(Ok(()), false, true, false), ["present", "reopen", "note Start"]);
        assert_eq!(steps(Ok(()), false, true, true), ["present", "reopen"], "nothing to note once it opens");
    }
}
