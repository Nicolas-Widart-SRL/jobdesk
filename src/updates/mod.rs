//! Updating Jobdesk itself: the feed check, the download, the signature check and the restart that applies it.

pub mod application;
mod domain;
mod infrastructure;

use crate::updates::application::{InstallBlock, Platform, ServiceEvent, Updates};
use crate::updates::infrastructure::{IdleUpdates, ScriptedUpdates};

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
