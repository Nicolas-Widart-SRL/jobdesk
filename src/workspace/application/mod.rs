mod environment;
mod open;
mod overview;

pub use environment::EnvironmentChecks;
pub use open::{WorkspaceRoot, open_workspace};
pub use overview::{AuthMode, Readiness, WorkspaceView, auth_mode, readiness, workspace_view};

/// The paths and the marker the checkout is read by, for the check that an `ai-job-search` release ships them.
pub use crate::workspace::domain::{ACTIVE_TEMPLATE, ASSISTANT, CV_TEMPLATES, LETTER_TEMPLATES, MARKER};
pub use crate::workspace::domain::{SyncState, TOOLS, TemplateState, ToolCheck, ToolProber, ToolSpec, WorkspaceInspector, WorkspaceStatus};
