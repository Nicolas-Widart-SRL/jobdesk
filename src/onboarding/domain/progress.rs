use std::collections::BTreeSet;
use std::path::PathBuf;

use chrono::NaiveDate;

use crate::shared_kernel::LoadError;

/// A chapter of first-run setup that is either done or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// The job search folder is made and holds ai-job-search.
    Folder,
    /// The helpers are installed.
    Computer,
    /// `/setup` built the profile.
    Profile,
}

/// What first-run setup has done so far. It stays in the data folder, so quitting at any step is safe and the next
/// open picks up where the user stopped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// The job search folder chosen on Save location, once Save here was pressed.
    pub folder: Option<PathBuf>,
    pub done: BTreeSet<Step>,
    /// The ai-job-search version unpacked into the folder, such as `1.7.5`.
    pub version: Option<String>,
    /// The day of the last change, which the next open names as the day the user stopped.
    pub stopped: Option<NaiveDate>,
}

impl Progress {
    pub fn is_done(&self, step: Step) -> bool {
        self.done.contains(&step)
    }
}

/// Where the progress record is kept.
pub trait ProgressStore: Send + Sync {
    /// `None` when no setup has started, or the last one got past the Computer step.
    fn load(&self) -> Result<Option<Progress>, LoadError>;
    fn save(&self, progress: &Progress) -> Result<(), LoadError>;
    /// Forgets the record, once setup is finished.
    fn clear(&self) -> Result<(), LoadError>;
}
