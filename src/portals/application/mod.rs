mod changes;
mod community;
mod view;

pub use changes::PortalChanges;
pub use community::{Community, CommunityPortalView, CommunityState, CommunityView, CountryGroup, STALE_AFTER};
pub use view::{Badge, CHECK_ALL_LINE, PortalSummary, PortalView, PortalsView, SEARCH_TERMS_LINE, portals_view};

/// The manifest each portal folder holds, for the check of an `ai-job-search` release.
pub use crate::portals::domain::MANIFEST;
pub use crate::portals::domain::{
    AddError, AddField, CommunityFailure, CommunityList, CountryCode, FolderName, GitSha, ListedPortal, Origin, Phrase, Priority, Quotes, SearchPlan,
    add_portal_line, list_url, parse_codeowners, site_lines_for, term_as_sent,
};
pub use crate::portals::domain::{ChangeError, PortalStore};
