mod batch;
mod documents;
mod helpers;
mod onboarding;
mod profile_run;

pub use batch::{Batch, Group, Item, Message, read_message};
pub use documents::{Added, Documents, Refused};
pub use helpers::HelperSetup;
pub use onboarding::{HelperLine, Onboarding, Stage, Work};
pub use profile_run::{DRAFT_SAVE_EVERY, PollInput, ProfilePhase, ProfileRun, SETUP, SaveTrouble, SendRefusal, StartRefusal, StartRules, phase_of};

pub use crate::onboarding::domain::{
    Arch, Artifact, CV_LOOKS_GOOD, Contents, CopyFailure, DocumentCopier, Draft, FOLDER_NAME, Failure, HELPERS, Helper, HelperInstaller, HelperProbe,
    InstallPhase, Layout, LitBy, Outcome, PATH_A_REPLY, PICK_UP_REPLY, PROGRESS_LINES, Phase, Place, Progress, ProgressLine, ProgressStore, Refusal, Saved,
    SourceFetcher, Step, Unpacker, joined_reply, review_reply, tool_line,
};
