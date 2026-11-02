use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use chrono::NaiveDate;

use crate::discovery::application::JobBoard;
use crate::dossier::application::{ApplicationRef, ArchiveContents};
use crate::insights::application::{SearchInsights, SkillGapView, WorkflowBand, search_insights, workflow_band};
use crate::profile::application::{ProfileView, topic_counts};
use crate::runs::application::UsageReport;
use crate::shared_kernel::{LoadError, Source};
use crate::tracking::application::Pipeline;
use crate::workspace::application::{WorkspaceView, workspace_view};
use crate::{discovery, dossier, insights, profile, tracking, workspace};

/// One consistent view of the workspace. Screens only ever read this.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    pub board: JobBoard,
    pub pipeline: Pipeline,
    pub insights: SearchInsights,
    pub band: WorkflowBand,
    pub skill_gaps: Option<SkillGapView>,
    pub archives: HashMap<String, ArchiveContents>,
    pub workspace: WorkspaceView,
    pub profile: ProfileView,
    pub usage: UsageReport,
    /// Whether the run logs were read into `usage`. False until then, so Usage statistics says it is reading.
    pub usage_read: bool,
    pub issues: Vec<Issue>,
    pub today: NaiveDate,
    /// When the load began, so the shell can tell a snapshot read after some moment, such as a run's end.
    /// `None` for a snapshot not read from a folder.
    pub read_at: Option<Instant>,
}

/// A source that could not be read, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub source: Source,
    pub error: LoadError,
}

impl Snapshot {
    /// A feature that fails to load reports an issue and the others keep working. The run logs are not read here:
    /// `usage` stays empty and unread.
    pub fn load(root: &Path, today: NaiveDate) -> Self {
        let read_at = Some(Instant::now());
        let mut issues = Vec::new();
        let board = discovery::load_board(root, today).unwrap_or_else(|error| {
            issues.push(Issue { source: Source::Jobs, error });
            JobBoard::default()
        });
        let pipeline = tracking::load_pipeline(root, today).unwrap_or_else(|error| {
            issues.push(Issue { source: Source::Applications, error });
            Pipeline::default()
        });
        let skill_gaps = insights::load_skill_gaps(root).unwrap_or_else(|error| {
            issues.push(Issue { source: Source::SkillGaps, error });
            None
        });
        let archives = pipeline
            .cards()
            .filter_map(|card| {
                let folder = card.archive_folder.clone()?;
                let reference = ApplicationRef {
                    folder: folder.clone(),
                    company: card.company.clone(),
                    cv_file: card.cv_file.clone(),
                    cover_letter_file: card.cover_letter_file.clone(),
                };
                Some((folder, dossier::load_archive(root, &reference, today)))
            })
            .collect();
        let (profile, errors) = profile::load(root);
        issues.extend(errors.into_iter().map(|error| Issue { source: Source::Profile, error }));
        let workspace = workspace_view(&workspace::inspect(root), &board, &profile);
        let insights = search_insights(&pipeline);
        let counts = topic_counts(&profile);
        let band = workflow_band(&insights, &board, (counts.filled, counts.total, counts.hidden));
        let (usage, usage_read) = (UsageReport::default(), false);
        Self { archives, workspace, profile, insights, band, board, pipeline, skill_gaps, usage, usage_read, issues, today, read_at }
    }
}
