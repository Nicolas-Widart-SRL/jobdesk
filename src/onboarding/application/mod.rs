mod onboarding;

pub use onboarding::{Onboarding, Stage};

pub use crate::onboarding::domain::{Contents, FOLDER_NAME, Failure, Phase, Place, Progress, ProgressStore, SourceFetcher, Step, Unpacker};
