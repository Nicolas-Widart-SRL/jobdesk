mod batch;
mod documents;
mod helpers;
mod onboarding;
mod profile_run;
mod work;

pub use batch::{Batch, Group, Item, Message, MessageBlock, PROPOSAL_WORDS, read_message};
pub use documents::{Added, CopyStop, Documents, Refused};
pub use helpers::HelperSetup;
pub use onboarding::{HelperLine, Onboarding, Stage, Work};
pub use profile_run::{
    DRAFT_SAVE_EVERY, PATH_A, PollInput, ProfilePhase, ProfileRun, SETUP, SETUP_COMPLETE, SaveTrouble, SendRefusal, StartRefusal, StartRules,
};
pub use work::{LineState, SoFar, WorkLine, so_far, work_lines};

pub use crate::onboarding::domain::{
    Arch, Artifact, CV_LOOKS_GOOD, Contents, CopyFailure, DocumentCopier, DocumentKind, Draft, FOLDER_NAME, Failure, HELPERS, Helper, HelperInstaller,
    HelperProbe, InstallPhase, Layout, LitBy, Outcome, PATH_A_REPLY, PICK_UP_REPLY, PROGRESS_LINES, Phase, Place, Progress, ProgressLine, ProgressStore,
    Refusal, Saved, SourceFetcher, Step, Unpacker, joined_reply, review_reply, tool_line,
};
/// The CV's pages, which the CV check shows as Profile's CV documents draw them.
pub use crate::profile::application::{CvShelf, PageImage, PageState, PageStore, PdfDocument};
/// The run the Profile chapter shows, and the user's answer to a prompt in it.
pub use crate::runs::application::{Decision, Prompt, PromptKind, Question, Quiet, Run, RunDesk, RunStatus, elapsed_text, quiet};
