//! Ports that read and write nothing, for tests and previews.

use std::path::Path;

use crate::folder_update::domain::{Base, BaseReader, FolderProbe, FolderUpdater, Report, Reverted, Update, UpdateError, ZipFetcher};
use crate::shared_kernel::download::DownloadFailure;
use crate::shared_kernel::installed_source::InstalledSource;

/// A folder that is never there, with nothing to roll back or undo, and a network that never answers.
pub struct IdleFolder;

impl FolderProbe for IdleFolder {
    fn exists(&self, _folder: &Path) -> bool {
        false
    }

    fn has_git(&self, _folder: &Path) -> bool {
        false
    }
}

impl BaseReader for IdleFolder {
    fn read(&self, _folder: &Path) -> Base {
        Base::Missing
    }

    fn skipped(&self, _folder: &Path) -> Option<InstalledSource> {
        None
    }
}

impl ZipFetcher for IdleFolder {
    fn fetch(&self, _url: &str, _name: &str, _progress: &mut dyn FnMut(u64)) -> Result<Vec<u8>, DownloadFailure> {
        Err(DownloadFailure::Offline)
    }
}

impl FolderUpdater for IdleFolder {
    fn open(&self, _folder: &Path) -> bool {
        false
    }

    fn hold_again(&self, _folder: &Path) -> bool {
        false
    }

    fn alone_possible(&self, _folder: &Path) -> bool {
        false
    }

    fn recover(&self, _folder: &Path) -> Result<Option<Reverted>, UpdateError> {
        Ok(None)
    }

    fn update(&self, _folder: &Path, _update: &Update) -> Result<Report, UpdateError> {
        Err(UpdateError::NoBase)
    }

    fn can_undo(&self, _folder: &Path) -> bool {
        false
    }

    fn undo_unfinished(&self, _folder: &Path) -> bool {
        false
    }

    fn undo(&self, _folder: &Path) -> Result<Reverted, UpdateError> {
        Err(UpdateError::NothingToUndo)
    }
}
