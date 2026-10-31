use std::collections::HashMap;
use std::path::Path;

use chrono::NaiveDate;

use crate::discovery::application::JobBoard;
use crate::dossier::application::{ApplicationRef, ArchiveContents};
use crate::insights::application::{SearchInsights, SkillGapView, search_insights};
use crate::overview::application::{Overview, compose};
use crate::shared_kernel::LoadError;
use crate::tracking::application::Pipeline;
use crate::workspace::application::{WorkspaceView, workspace_view};
use crate::{discovery, dossier, insights, tracking, workspace};

/// One consistent view of the workspace. Screens only ever read this.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    pub board: JobBoard,
    pub pipeline: Pipeline,
    pub overview: Overview,
    pub insights: SearchInsights,
    pub skill_gaps: Option<SkillGapView>,
    pub archives: HashMap<String, ArchiveContents>,
    pub workspace: WorkspaceView,
    pub issues: Vec<LoadError>,
}

impl Snapshot {
    /// A feature that fails to load reports an issue and the others keep working.
    pub fn load(root: &Path, today: NaiveDate) -> Self {
        let mut issues = Vec::new();
        let board = discovery::load_board(root, today).unwrap_or_else(|error| {
            issues.push(error);
            JobBoard::default()
        });
        let pipeline = tracking::load_pipeline(root, today).unwrap_or_else(|error| {
            issues.push(error);
            Pipeline::default()
        });
        let skill_gaps = insights::load_skill_gaps(root).unwrap_or_else(|error| {
            issues.push(error);
            None
        });
        let archives = pipeline
            .cards()
            .filter_map(|card| {
                let folder = card.archive_folder.clone()?;
                let reference = ApplicationRef { folder: folder.clone(), company: card.company.clone(), cv_file: card.cv_file.clone(), cover_letter_file: card.cover_letter_file.clone() };
                Some((folder, dossier::load_archive(root, &reference, today)))
            })
            .collect();
        Self { archives, workspace: workspace_view(&workspace::inspect(root), &board), overview: compose(&board, &pipeline), insights: search_insights(&pipeline), board, pipeline, skill_gaps, issues }
    }
}
