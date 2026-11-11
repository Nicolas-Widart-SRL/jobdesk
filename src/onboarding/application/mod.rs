mod batch;
mod helpers;
mod onboarding;

pub use batch::{Batch, Group, Item, Message, read_message};
pub use helpers::HelperSetup;
pub use onboarding::{HelperLine, Onboarding, Stage, Work};

pub use crate::onboarding::domain::{
    Arch, Artifact, CV_LOOKS_GOOD, Contents, FOLDER_NAME, Failure, HELPERS, Helper, HelperInstaller, HelperProbe, InstallPhase, Layout, LitBy, Outcome,
    PICK_UP_REPLY, PROGRESS_LINES, Phase, Place, Progress, ProgressLine, ProgressStore, SourceFetcher, Step, Unpacker, joined_reply, review_reply, tool_line,
};
