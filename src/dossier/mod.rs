//! One application in depth: documents, evaluation, prep packs, outcome and follow-ups.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;
use std::sync::Arc;

use chrono::NaiveDate;

use crate::dossier::application::{ApplicationRef, ArchiveContents, ArchiveRepository, PageStore};
use crate::dossier::infrastructure::{ArchiveFiles, HayroRasterizer};

pub fn load_archive(root: &Path, application: &ApplicationRef, today: NaiveDate) -> ArchiveContents {
    ArchiveFiles::in_workspace(root).load(application, today)
}

pub fn page_store(repaint: Arc<dyn Fn() + Send + Sync>) -> PageStore {
    PageStore::new(Arc::new(HayroRasterizer), repaint)
}
