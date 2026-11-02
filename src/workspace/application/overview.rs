use crate::discovery::application::JobBoard;
use crate::workspace::domain::{ProfileFile, ProfileState, SyncState, TemplateState, ToolCheck, WorkspaceStatus};

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
    let portals = status
        .portals
        .iter()
        .map(|portal| PortalRow { name: portal.name.clone(), enabled: portal.enabled, jobs_seen: board.seen_on_portal(&portal.name) })
        .collect();
    WorkspaceView {
        profile: status.profile.clone(),
        portals,
        templates: status.templates.clone(),
        sync: status.sync.clone(),
        has_salary_data: status.has_salary_data,
    }
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

/// What still stands between the user and a working setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Readiness {
    /// Required tools that did not answer. Only these keep the Setup tab.
    pub missing: usize,
    /// Profile files still to fill in, for the rail's word only.
    pub to_fill: usize,
}

impl Readiness {
    pub fn ready(self) -> bool {
        self.missing == 0
    }
}

/// Optional tools never count, and tools not checked yet count as nothing, so a ready setup never flashes the Setup tab.
pub fn readiness(view: &WorkspaceView, tools: Option<&[ToolCheck]>) -> Readiness {
    let missing = tools.map_or(0, |tools| tools.iter().filter(|check| check.spec.required && check.version.is_none()).count());
    let to_fill = view.profile.iter().filter(|file| !matches!(file.state, ProfileState::Filled)).count();
    Readiness { missing, to_fill }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::domain::TOOLS;

    /// Every tool answers except those named.
    fn tools_without(names: &[&str]) -> Vec<ToolCheck> {
        TOOLS.iter().map(|spec| ToolCheck { spec: *spec, version: (!names.contains(&spec.name)).then(|| "1.0".to_owned()) }).collect()
    }

    fn view_with(state: ProfileState) -> WorkspaceView {
        let profile = vec![
            ProfileFile { name: "01-candidate-profile.md".into(), state: ProfileState::Filled },
            ProfileFile { name: "03-writing-style.md".into(), state },
        ];
        WorkspaceView { profile, ..WorkspaceView::default() }
    }

    #[test]
    fn a_full_setup_is_ready() {
        let readiness = readiness(&view_with(ProfileState::Filled), Some(&tools_without(&[])));
        assert_eq!(readiness, Readiness { missing: 0, to_fill: 0 });
        assert!(readiness.ready());
    }

    #[test]
    fn a_missing_required_tool_is_not_ready() {
        let readiness = readiness(&view_with(ProfileState::Filled), Some(&tools_without(&["bun"])));
        assert_eq!(readiness.missing, 1);
        assert!(!readiness.ready());
    }

    #[test]
    fn a_missing_optional_tool_is_still_ready() {
        assert!(readiness(&view_with(ProfileState::Filled), Some(&tools_without(&["pdftotext"]))).ready());
    }

    #[test]
    fn tools_not_checked_yet_count_as_nothing() {
        let readiness = readiness(&view_with(ProfileState::Filled), None);
        assert_eq!(readiness.missing, 0);
        assert!(readiness.ready());
    }

    #[test]
    fn profile_files_to_fill_never_count_as_missing() {
        for state in [ProfileState::Missing, ProfileState::NeedsSetup { placeholders: 3 }] {
            let readiness = readiness(&view_with(state), Some(&tools_without(&[])));
            assert_eq!(readiness, Readiness { missing: 0, to_fill: 1 }, "{state:?}");
            assert!(readiness.ready());
        }
    }

    #[test]
    fn counts_filled_profile_files() {
        let file = |name: &str, state| ProfileFile { name: name.into(), state };
        let profile =
            vec![file("cv.md", ProfileState::Filled), file("skills.md", ProfileState::NeedsSetup { placeholders: 2 }), file("goals.md", ProfileState::Missing)];
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
