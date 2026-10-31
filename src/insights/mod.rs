//! How the search is going: the data behind `/html-report` and `/upskill`.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;

use crate::insights::application::{SkillGapService, SkillGapView};
use crate::insights::infrastructure::UpskillReports;
use crate::shared_kernel::LoadError;

pub fn load_skill_gaps(root: &Path) -> Result<Option<SkillGapView>, LoadError> {
    SkillGapService::new(UpskillReports::in_workspace(root)).latest()
}
