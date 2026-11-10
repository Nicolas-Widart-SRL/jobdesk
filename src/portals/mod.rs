//! The job boards `/scrape` searches: the portal folders under `.agents/skills` and the search terms they share.

pub mod application;
mod domain;
mod infrastructure;

use std::path::Path;

use crate::discovery::application::JobBoard;
use crate::portals::application::{PortalsView, portals_view};
use crate::portals::infrastructure::PortalFolder;
use crate::shared_kernel::LoadError;

/// The installed portals and the search file. `board` is `None` when the saved jobs could not be read. A
/// `.agents/skills` that could not be read is the error. A search file that could not be read only reads as unread in
/// the view, because the profile reports that file already.
pub fn load(root: &Path, board: Option<&JobBoard>) -> (PortalsView, Option<LoadError>) {
    let folder = PortalFolder::in_workspace(root);
    let installed = folder.installed();
    let error = installed.as_ref().err().cloned();
    (portals_view(installed, folder.search_plan(), board, &[]), error)
}
