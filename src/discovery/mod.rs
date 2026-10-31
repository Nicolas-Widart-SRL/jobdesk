//! Finding and triaging postings: the output of `/scrape` and `/rank`.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;

use chrono::NaiveDate;

use crate::discovery::application::{JobBoard, JobBoardService};
use crate::discovery::infrastructure::SeenJobsFile;
use crate::shared_kernel::LoadError;

pub fn load_board(root: &Path, today: NaiveDate) -> Result<JobBoard, LoadError> {
    JobBoardService::new(SeenJobsFile::in_workspace(root)).board(today)
}
