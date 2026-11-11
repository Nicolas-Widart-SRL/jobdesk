/// The folder of the checkout that holds the template guides.
pub const ASSISTANT: &str = ".claude/skills/job-application-assistant";
/// The template guides in `ASSISTANT`, for the CV and the cover letter. Each says a custom template is active when it
/// holds `ACTIVE_TEMPLATE`, the start of the block `/add-template` writes into it.
pub const CV_TEMPLATES: &str = "05-cv-templates.md";
pub const LETTER_TEMPLATES: &str = "06-cover-letter-templates.md";
pub const ACTIVE_TEMPLATE: &str = "BEGIN ACTIVE-TEMPLATE";

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
    pub templates: TemplateState,
    pub sync: SyncState,
    pub has_salary_data: bool,
}

pub trait WorkspaceInspector {
    fn status(&self) -> WorkspaceStatus;
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
    tool("bun", "Job site search", true),
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
