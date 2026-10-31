use crate::discovery::application::JobBoard;
use crate::workspace::domain::{ProfileFile, ProfileState, SyncState, TemplateState, WorkspaceStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalRow {
    pub name: String,
    pub enabled: bool,
    pub jobs_seen: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceView {
    pub profile: Vec<ProfileFile>,
    pub portals: Vec<PortalRow>,
    pub templates: TemplateState,
    pub sync: SyncState,
    pub has_salary_data: bool,
}

impl WorkspaceView {
    /// `(filled, total)` skill files.
    pub fn profile_counts(&self) -> (usize, usize) {
        let filled = self.profile.iter().filter(|file| file.state == ProfileState::Filled).count();
        (filled, self.profile.len())
    }
}

pub fn workspace_view(status: &WorkspaceStatus, board: &JobBoard) -> WorkspaceView {
    let portals = status.portals.iter().map(|portal| PortalRow { name: portal.name.clone(), enabled: portal.enabled, jobs_seen: board.seen_on_portal(&portal.name) }).collect();
    WorkspaceView { profile: status.profile.clone(), portals, templates: status.templates.clone(), sync: status.sync.clone(), has_salary_data: status.has_salary_data }
}

/// How runs reach Claude, from the CLI's own `apiKeySource` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    NoRunYet,
    ClaudeLogin,
    ApiKey,
}

pub fn auth_mode(api_key_source: Option<&str>) -> AuthMode {
    match api_key_source {
        None => AuthMode::NoRunYet,
        Some("none" | "") => AuthMode::ClaudeLogin,
        Some(_) => AuthMode::ApiKey,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_filled_profile_files() {
        let file = |name: &str, state| ProfileFile { name: name.into(), state };
        let profile = vec![file("cv.md", ProfileState::Filled), file("skills.md", ProfileState::NeedsSetup { placeholders: 2 }), file("goals.md", ProfileState::Missing)];
        let view = WorkspaceView { profile, ..WorkspaceView::default() };
        assert_eq!(view.profile_counts(), (1, 3));
    }

    #[test]
    fn reads_the_cli_auth_source() {
        assert_eq!(auth_mode(None), AuthMode::NoRunYet);
        assert_eq!(auth_mode(Some("none")), AuthMode::ClaudeLogin);
        assert_eq!(auth_mode(Some("ANTHROPIC_API_KEY")), AuthMode::ApiKey);
    }
}
