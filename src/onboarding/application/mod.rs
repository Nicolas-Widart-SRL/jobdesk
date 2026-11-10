mod helpers;
mod onboarding;

pub use helpers::HelperSetup;
pub use onboarding::{Onboarding, Stage};

pub use crate::onboarding::domain::{
    Arch, Artifact, Contents, FOLDER_NAME, Failure, HELPERS, Helper, HelperInstaller, HelperProbe, InstallPhase, Layout, Outcome, Phase, Place, Progress,
    ProgressStore, SourceFetcher, Step, Unpacker,
};
