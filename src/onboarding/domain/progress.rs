use std::collections::BTreeSet;
use std::fmt;
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
    /// The id of the `/setup` run Read my documents started, which the next open finds again among the runs.
    pub run: Option<String>,
    /// Whether Jobdesk sent its one reply of its own, the one that picks the documents path, so a restart never sends
    /// it twice.
    pub path_a_sent: bool,
    /// What the user typed for Claude's last turn and has not sent yet.
    pub draft: Option<Draft>,
}

impl Progress {
    pub fn is_done(&self, step: Step) -> bool {
        self.done.contains(&step)
    }
}

/// The user's answers to one of Claude's turns, not sent yet. It belongs to the run and the turn it was typed for, so
/// it never shows under a later turn. It holds the user's own words: its `Debug` prints only their sizes, so a log
/// line or a panic never carries them.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub run: String,
    /// The run's turn number when the user typed it: how many messages had been sent, the command line included.
    pub turn: usize,
    /// The field under each numbered question, by Claude's number.
    pub answers: Vec<(u32, String)>,
    /// The one field of a sheet with no numbered questions, or the words of a change the user asks for.
    pub text: String,
}

impl Draft {
    /// Whether every field is empty, so there is nothing to keep.
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.answers.iter().all(|(_, answer)| answer.trim().is_empty())
    }
}

impl fmt::Debug for Draft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sizes: Vec<(u32, usize)> = self.answers.iter().map(|(number, answer)| (*number, answer.chars().count())).collect();
        f.debug_struct("Draft")
            .field("run", &self.run)
            .field("turn", &self.turn)
            .field("answer_sizes", &sizes)
            .field("text_size", &self.text.chars().count())
            .finish()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drafts_debug_names_no_word_the_user_typed() {
        let draft = Draft { run: "r1".into(), turn: 3, answers: vec![(1, "Lowmere harbour".into())], text: "Only remote work".into() };
        let printed = format!("{draft:?}");
        assert!(!printed.contains("Lowmere") && !printed.contains("remote"), "{printed}");
        assert!(printed.contains("r1") && printed.contains('3'));
    }

    #[test]
    fn a_draft_of_blank_fields_is_empty() {
        assert!(Draft { answers: vec![(1, " \n".into())], text: "\t".into(), ..Draft::default() }.is_empty());
        assert!(!Draft { answers: vec![(1, "a".into())], ..Draft::default() }.is_empty());
        assert!(!Draft { text: "a".into(), ..Draft::default() }.is_empty());
    }
}
