//! Applications from draft to resolution: the output of `/apply`, `/outcome` and `/gmail-sync`.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;

use chrono::NaiveDate;

use crate::shared_kernel::LoadError;
use crate::tracking::application::{Pipeline, PipelineService};
use crate::tracking::infrastructure::TrackerFiles;

pub fn load_pipeline(root: &Path, today: NaiveDate) -> Result<Pipeline, LoadError> {
    PipelineService::new(TrackerFiles::in_workspace(root)).pipeline(today)
}
