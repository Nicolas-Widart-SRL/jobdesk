#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileState {
    Filled,
    NeedsSetup { placeholders: usize },
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileFile {
    pub name: String,
    pub state: ProfileState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalInfo {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TemplateState {
    pub custom_cv: bool,
    pub custom_letter: bool,
    pub registered: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncState {
    pub gmail_last_sync: Option<String>,
    pub gmail_set_up: bool,
    pub notion_set_up: bool,
}

/// What the checkout holds right now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceStatus {
    pub profile: Vec<ProfileFile>,
    pub portals: Vec<PortalInfo>,
    pub templates: TemplateState,
    pub sync: SyncState,
    pub has_salary_data: bool,
}

pub trait WorkspaceInspector {
    fn status(&self) -> WorkspaceStatus;
}

/// `/reset` restores tokens such as `[FIRST_NAME]`, and `/setup` replaces them.
pub fn placeholder_count(text: &str) -> usize {
    text.split('[')
        .skip(1)
        .filter(|rest| {
            rest.split_once(']').is_some_and(|(token, _)| {
                token.len() >= 3 && token.chars().all(|c| c.is_ascii_uppercase() || c == '_') && token.chars().any(|c| c.is_ascii_uppercase())
            })
        })
        .count()
}

/// A program the workflow calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub args: &'static [&'static str],
    pub purpose: &'static str,
    pub required: bool,
}

const fn tool(name: &'static str, purpose: &'static str, required: bool) -> ToolSpec {
    ToolSpec { name, args: &["--version"], purpose, required }
}

pub const TOOLS: [ToolSpec; 6] = [
    tool("claude", "Runs commands", true),
    tool("python3", "State tools", true),
    tool("bun", "Portal CLIs", true),
    tool("lualatex", "CV", true),
    tool("xelatex", "Cover letter", true),
    tool("pdftotext", "Layout check", false),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCheck {
    pub spec: ToolSpec,
    pub version: Option<String>,
}

pub trait ToolProber: Send + Sync {
    /// The first line a tool prints about itself, or `None` when it cannot run.
    fn probe(&self, tool: &ToolSpec) -> Option<String>;
}

#[cfg(test)]
mod tests {
    use super::placeholder_count;

    #[test]
    fn counts_only_setup_tokens() {
        assert_eq!(placeholder_count(r"\name{[FIRST_NAME]}{[LAST_NAME]} [x] [AB] [Note] - [ ] a[0] [YOUR_ADDRESS]"), 3);
    }
}
