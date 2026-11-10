//! The job boards `/scrape` searches: the portal folders under `.agents/skills` and the search terms they share, and
//! the community list of portals on GitHub.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;
use std::sync::Arc;

use crate::discovery::application::JobBoard;
use crate::portals::application::PortalChanges;
use crate::portals::application::{Community, CommunityFailure, CommunityList, PortalsView, portals_view};
use crate::portals::infrastructure::PortalWriter;
use crate::portals::infrastructure::{CommunityCache, GithubCommunity, HttpsApi, PortalFolder, ScriptedCommunity};
use crate::shared_kernel::LoadError;

/// The installed portals and the search file. `board` is `None` when the saved jobs could not be read. A
/// `.agents/skills`, or a `SKILL.md` in it, that could not be read is the error. A search file that could not be read
/// only reads as unread in the view, because the profile reports that file already.
pub fn load(root: &Path, board: Option<&JobBoard>) -> (PortalsView, Option<LoadError>) {
    let folder = PortalFolder::in_workspace(root);
    let installed = folder.installed();
    let error = installed.as_ref().err().cloned();
    (portals_view(installed, folder.search_plan(), board), error)
}

/// The community list from GitHub, without sign-in. `data` is the app's data folder: the list read last is saved in its
/// `portals/community.json`, and read from there now, without the network. With no data folder, nothing is saved and
/// every read fetches the whole list. `version` is this copy's version, sent as its user agent, and `repaint` wakes the
/// window when an answer arrives. Nothing here starts `claude`, uses the API key or writes a run log.
pub fn community(data: Option<&Path>, version: &str, repaint: Arc<dyn Fn() + Send + Sync>) -> Community {
    let cache = data.map(CommunityCache::in_data);
    let saved = cache.as_ref().and_then(CommunityCache::load);
    Community::new(Box::new(GithubCommunity::new(Arc::new(HttpsApi::new(version)), cache, repaint)), saved)
}

/// A community list that starts from `saved` and answers each read from `answers` at once, so a test or a snapshot
/// reaches any state without the network. Once `answers` is used up, a read never ends.
pub fn scripted_community(saved: Option<CommunityList>, answers: Vec<Result<CommunityList, CommunityFailure>>) -> Community {
    Community::new(Box::new(ScriptedCommunity::new(answers)), saved)
}

/// The switch and the removal for the portals of the checkout at `root`.
pub fn open_changes(root: &Path) -> PortalChanges {
    PortalChanges::new(Box::new(PortalWriter::in_workspace(root)))
}
