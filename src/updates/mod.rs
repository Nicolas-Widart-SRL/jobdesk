//! Updating Jobdesk itself: the feed check, the download, the signature check and the restart that applies it.

pub mod application;
mod domain;
mod infrastructure;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::updates::application::{ApplyFailed, ApplyFailure, InstallBlock, Platform, RestartPlan, ServiceEvent, Updates};
#[cfg(target_os = "macos")]
use crate::updates::infrastructure::install_macos::reopen;
use crate::updates::infrastructure::{GithubUpdates, HttpsTransport, IdleUpdates, ScriptedUpdates, UpdateKey, cleanup, install_windows};

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
/// before `last_apply_failure`, which it leaves in place. Errors are ignored, and links are removed, never followed.
pub fn clear_downloads(data: &Path) {
    cleanup::clear(&downloads(data));
}

/// Why the last Restart to update did not apply, once: the next call gives `None`.
pub fn last_apply_failure(data: &Path) -> Option<ApplyFailed> {
    cleanup::take_apply_failure(&downloads(data))
}

/// Applies `plan` once the window has closed, then opens Jobdesk again with `args`, which `relaunch_args` gives.
/// It checks the file against the release's signature once more first. On macOS it swaps the bundle and opens the
/// new copy. On Windows it starts the setup, which closes this copy and opens the new one, and returns at once.
/// When it cannot apply, it notes why in `updates/` for the next launch and opens this copy again.
pub fn finish(plan: &RestartPlan, args: &[OsString]) {
    let Ok(exe) = std::env::current_exe() else { return };
    let key = UpdateKey::embedded();
    let applied = match Platform::current() {
        #[cfg(target_os = "macos")]
        Some(Platform::MacArm) => infrastructure::install_macos::apply(plan, &exe, key.as_ref()),
        Some(Platform::WindowsX64) => install_windows::apply(plan, &exe, args, key.as_ref()),
        _ => Err(ApplyFailure::NotInstalled),
    };
    let note = |failure| {
        if let Some(dir) = plan.file.parent() {
            cleanup::record_apply_failure(dir, ApplyFailed { version: plan.version, failure });
        }
    };
    match applied {
        // The setup opens the new copy itself once it has installed it.
        Ok(()) if Platform::current() == Some(Platform::WindowsX64) => {}
        Ok(()) => {
            if reopen(&exe, args).is_err() {
                note(ApplyFailure::Start);
            }
        }
        // Noted before this copy opens again, so its launch finds the note.
        Err(failure) => {
            note(failure);
            let _ = reopen(&exe, args);
        }
    }
}

/// Starts the program at `exe` again with `args`.
#[cfg(not(target_os = "macos"))]
fn reopen(exe: &Path, args: &[OsString]) -> std::io::Result<()> {
    let mut program = std::process::Command::new(exe);
    crate::shared_kernel::process_tree::no_window(&mut program);
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
        let release = Release { version: Version { major: 0, minor: 2, patch: 0 }, released: None, notes: String::new(), size: 10, signature: "sig".into() };
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
}
