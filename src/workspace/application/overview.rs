use crate::portals::application::PortalsView;
use crate::profile::application::{ProfileView, TopicCounts, topic_counts};
use crate::shared_kernel::{KeySource, key_source};
use crate::workspace::domain::{SyncState, TemplateState, ToolCheck, WorkspaceStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalRow {
    pub name: String,
    pub enabled: bool,
    pub jobs_seen: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceView {
    /// How many profile topics are filled, for Setup's one-line summary.
    pub topics: TopicCounts,
    pub portals: Vec<PortalRow>,
    pub templates: TemplateState,
    pub sync: SyncState,
    pub has_salary_data: bool,
}

/// Setup names each portal by its folder, as before the Portals screen. A portal's jobs count as none while the saved
/// jobs cannot be read, and Setup hides them then.
pub fn workspace_view(status: &WorkspaceStatus, portals: &PortalsView, profile: &ProfileView) -> WorkspaceView {
    let portals = portals
        .portals
        .iter()
        .map(|portal| PortalRow { name: portal.folder.clone(), enabled: portal.on, jobs_seen: portal.jobs.map_or(0, |(count, _)| count) })
        .collect();
    WorkspaceView {
        topics: topic_counts(profile),
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

/// Read as Settings › Providers reads it. A sign-in Claude Code chose itself, such as an `apiKeyHelper`, reads as an API key:
/// the only other sources the CLI names are keys too.
pub fn auth_mode(api_key_source: Option<&str>) -> AuthMode {
    match api_key_source.map(key_source) {
        None => AuthMode::NoRunYet,
        Some(KeySource::ClaudeSignIn) => AuthMode::ClaudeLogin,
        Some(KeySource::KeyVariable | KeySource::Other) => AuthMode::ApiKey,
    }
}

/// What still stands between the user and a working setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Readiness {
    /// Required tools that did not answer. Only these keep the Setup tab.
    pub missing: usize,
    /// Profile topics with something left to fill, not started ones included, for the rail's word only.
    /// A hidden topic could not be read, so it is not counted either way.
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
    let topics = view.topics;
    let to_fill = topics.total.saturating_sub(topics.filled + topics.hidden);
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

    fn view_with(filled: usize, hidden: usize) -> WorkspaceView {
        WorkspaceView { topics: TopicCounts { filled, total: 11, hidden, any_found: true }, ..WorkspaceView::default() }
    }

    #[test]
    fn a_full_setup_is_ready() {
        let readiness = readiness(&view_with(11, 0), Some(&tools_without(&[])));
        assert_eq!(readiness, Readiness { missing: 0, to_fill: 0 });
        assert!(readiness.ready());
    }

    #[test]
    fn a_missing_required_tool_is_not_ready() {
        let readiness = readiness(&view_with(11, 0), Some(&tools_without(&["bun"])));
        assert_eq!(readiness.missing, 1);
        assert!(!readiness.ready());
    }

    #[test]
    fn a_missing_optional_tool_is_still_ready() {
        assert!(readiness(&view_with(11, 0), Some(&tools_without(&["pdftotext"]))).ready());
    }

    #[test]
    fn tools_not_checked_yet_count_as_nothing() {
        let readiness = readiness(&view_with(11, 0), None);
        assert_eq!(readiness.missing, 0);
        assert!(readiness.ready());
    }

    #[test]
    fn topics_to_fill_never_count_as_missing() {
        let readiness = readiness(&view_with(7, 0), Some(&tools_without(&[])));
        assert_eq!(readiness, Readiness { missing: 0, to_fill: 4 });
        assert!(readiness.ready());
    }

    /// A hidden topic may be filled or not, so it is left out of what is still to fill.
    #[test]
    fn hidden_topics_are_not_to_fill() {
        assert_eq!(readiness(&view_with(6, 1), Some(&tools_without(&[]))).to_fill, 4);
        assert_eq!(readiness(&view_with(0, 11), Some(&tools_without(&[]))).to_fill, 0);
    }

    /// With no profile file found, every topic is still to fill.
    #[test]
    fn no_profile_leaves_every_topic_to_fill() {
        let view = WorkspaceView { topics: TopicCounts { filled: 0, total: 11, hidden: 0, any_found: false }, ..WorkspaceView::default() };
        assert_eq!(readiness(&view, Some(&tools_without(&[]))).to_fill, 11);
    }

    #[test]
    fn the_view_counts_the_profile_topics() {
        let (profile, _) = crate::profile::application::profile_view(&|_| Ok(None));
        let view = workspace_view(&WorkspaceStatus::default(), &PortalsView::default(), &profile);
        assert_eq!(view.topics, TopicCounts { filled: 0, total: 11, hidden: 0, any_found: false });
    }

    #[test]
    fn reads_the_cli_auth_source() {
        assert_eq!(auth_mode(None), AuthMode::NoRunYet);
        assert_eq!(auth_mode(Some("none")), AuthMode::ClaudeLogin);
        assert_eq!(auth_mode(Some("ANTHROPIC_API_KEY")), AuthMode::ApiKey);
    }

    #[test]
    fn the_login_key_is_the_claude_sign_in_as_on_providers() {
        assert_eq!(auth_mode(Some("/login managed key")), AuthMode::ClaudeLogin);
        assert_eq!(auth_mode(Some("")), AuthMode::ClaudeLogin);
        assert_eq!(auth_mode(Some("apiKeyHelper")), AuthMode::ApiKey, "a key Claude Code found itself is still a key");
    }
}
