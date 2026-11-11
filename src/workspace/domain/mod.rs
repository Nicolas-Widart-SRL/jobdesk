mod root;
mod status;

pub use root::{MARKER, WorkspaceRoot};
pub use status::{
    ACTIVE_TEMPLATE, ASSISTANT, CV_TEMPLATES, LETTER_TEMPLATES, SyncState, TOOLS, TemplateState, ToolCheck, ToolProber, ToolSpec, WorkspaceInspector,
    WorkspaceStatus,
};
