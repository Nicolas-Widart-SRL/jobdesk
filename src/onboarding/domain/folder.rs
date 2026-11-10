use std::path::{Path, PathBuf};

/// The name of the folder setup suggests, in the user's Documents folder or inside a folder that already holds files.
pub const FOLDER_NAME: &str = "Job search";

/// The folder setup suggests first: a new `Job search` folder in the user's Documents folder.
pub fn suggested_folder(documents: &Path) -> PathBuf {
    documents.join(FOLDER_NAME)
}

/// What a chosen path holds now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contents {
    Missing,
    /// A folder with nothing in it.
    Empty,
    /// Anything else: a folder with files, a file, or a link.
    Files,
    /// A folder that is already an ai-job-search folder.
    AiJobSearch,
}

/// Whether setup may use a chosen folder. It sets up only in a new or empty folder, so nothing of the user's is ever
/// overwritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// Nothing is there yet: setup makes the folder.
    New,
    /// An empty folder, used as it is.
    Empty,
    /// Refused: it holds files. `suggest` is a new `Job search` folder inside it.
    NotEmpty { suggest: PathBuf },
    /// Refused: it is already an ai-job-search folder, which the welcome's other choice opens.
    AlreadyAiJobSearch,
}

impl Place {
    pub fn of(chosen: &Path, contents: Contents) -> Place {
        match contents {
            Contents::Missing => Place::New,
            Contents::Empty => Place::Empty,
            Contents::Files => Place::NotEmpty { suggest: chosen.join(FOLDER_NAME) },
            Contents::AiJobSearch => Place::AlreadyAiJobSearch,
        }
    }

    /// Whether Save here may use the folder.
    pub fn usable(&self) -> bool {
        matches!(self, Place::New | Place::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_new_or_empty_folder_is_usable() {
        let chosen = Path::new("/Users/sam/Documents/Old stuff");
        assert_eq!(Place::of(chosen, Contents::Missing), Place::New);
        assert_eq!(Place::of(chosen, Contents::Empty), Place::Empty);
        assert_eq!(Place::of(chosen, Contents::Files), Place::NotEmpty { suggest: PathBuf::from("/Users/sam/Documents/Old stuff/Job search") });
        assert_eq!(Place::of(chosen, Contents::AiJobSearch), Place::AlreadyAiJobSearch);
        let usable: Vec<bool> = [Contents::Missing, Contents::Empty, Contents::Files, Contents::AiJobSearch].map(|c| Place::of(chosen, c).usable()).into();
        assert_eq!(usable, [true, true, false, false]);
    }

    #[test]
    fn the_first_suggestion_is_in_documents() {
        assert_eq!(suggested_folder(Path::new("/Users/sam/Documents")), PathBuf::from("/Users/sam/Documents/Job search"));
    }
}
