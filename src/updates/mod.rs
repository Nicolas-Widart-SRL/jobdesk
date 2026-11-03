//! Updating Jobdesk itself: the feed check, the download, the signature check and the restart that applies it.

pub mod application;
mod domain;
mod infrastructure;

use crate::updates::application::{InstallBlock, ServiceEvent, Updates};
use crate::updates::infrastructure::{IdleUpdates, ScriptedUpdates};

/// An update that never hears back: a check started with it stays running. For previews and snapshots.
/// `current` is the version this copy runs, such as `0.1.0`.
pub fn idle_updates(current: &str) -> Updates {
    Updates::new(Box::new(IdleUpdates), current, None)
}

/// An update that answers from `events` at once, so a test or a snapshot reaches any state without the network.
pub fn scripted_updates(current: &str, events: Vec<ServiceEvent>, block: Option<InstallBlock>) -> Updates {
    Updates::new(Box::new(ScriptedUpdates::new(events)), current, block)
}
