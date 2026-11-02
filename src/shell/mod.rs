//! Composition root: wires the features together and owns the window.

mod app;
mod live_snapshot;
mod snapshot;

pub use crate::shared_kernel::Source;
pub use app::{Screen, Shell, install_look, run};
pub use snapshot::{Issue, Snapshot};
