//! The job boards `/scrape` searches: the portal folders under `.agents/skills` and the search terms they share.

pub mod application;
mod domain;
mod infrastructure;

use std::path::Path;

use crate::discovery::application::JobBoard;
use crate::portals::application::{PortalsView, portals_view};
use crate::portals::infrastructure::PortalFolder;
use crate::shared_kernel::LoadError;

/// The installed portals and the search file. `board` is `None` when the saved jobs could not be read. A folder or
/// file that could not be read is an error, and the view says which part is missing.
pub fn load(root: &Path, board: Option<&JobBoard>) -> (PortalsView, Vec<LoadError>) {
    let folder = PortalFolder::in_workspace(root);
    let (installed, plan) = (folder.installed(), folder.search_plan());
    let errors = [installed.as_ref().err(), plan.as_ref().err()].into_iter().flatten().cloned().collect();
    (portals_view(installed, plan, board, &[]), errors)
}
