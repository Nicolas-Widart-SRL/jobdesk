use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use chrono::NaiveDate;

use crate::discovery::application::{JobBoard, YourTerms};
use crate::dossier::application::{ApplicationRef, ArchiveContents};
use crate::insights::application::{SearchInsights, SkillGapView, WorkflowBand, search_insights, workflow_band};
use crate::portals::application::PortalsView;
use crate::profile::application::{CvShelf, ProfileView, topic_counts};
use crate::runs::application::UsageReport;
use crate::shared_kernel::{LoadError, Source};
use crate::tracking::application::Pipeline;
use crate::workspace::application::{WorkspaceView, workspace_view};
use crate::{discovery, dossier, insights, portals, profile, tracking, workspace};

/// One consistent view of the workspace. Screens only ever read this.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    pub board: JobBoard,
    /// Your day-rate floor and commute tiers, with whether each file could be read.
    pub terms: YourTerms,
    pub pipeline: Pipeline,
    pub insights: SearchInsights,
    pub band: WorkflowBand,
    pub skill_gaps: Option<SkillGapView>,
    pub archives: HashMap<String, ArchiveContents>,
    pub workspace: WorkspaceView,
    /// The installed portals and the search terms they share.
    pub portals: PortalsView,
    pub profile: ProfileView,
    /// The CV documents of `cv/`, or that the folder could not be read.
    pub cv_documents: CvShelf,
    pub usage: UsageReport,
    /// Whether the run logs were read into `usage`. False until then, so Usage statistics says it is reading.
    pub usage_read: bool,
    /// The last reload asked for before the run logs were read into `usage`. A task that ends asks for one, and its cost
    /// is in `usage` only from the snapshot that carries that ask.
    pub usage_asked: u64,
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
        let terms = discovery::load_terms(root);
        let board = discovery::load_board(root, today, &terms.gates());
        let (portals, portals_error) = portals::load(root, board.as_ref().ok());
        let board = board.unwrap_or_else(|error| {
            issues.push(Issue { source: Source::Jobs, error });
            JobBoard::default()
        });
        issues.extend(portals_error.map(|error| Issue { source: Source::Portals, error }));
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
        let cv_documents = profile::load_cv(root).unwrap_or_else(|error| {
            issues.push(Issue { source: Source::CvDocuments, error });
            CvShelf::Unread
        });
        let workspace = workspace_view(&workspace::inspect(root), &portals, &profile);
        let insights = search_insights(&pipeline);
        let counts = topic_counts(&profile);
        let band = workflow_band(&insights, &board, (counts.filled, counts.total, counts.hidden));
        let (usage, usage_read, usage_asked) = (UsageReport::default(), false, 0);
        Self {
            archives,
            workspace,
            portals,
            profile,
            cv_documents,
            insights,
            band,
            board,
            terms,
            pipeline,
            skill_gaps,
            usage,
            usage_read,
            usage_asked,
            issues,
            today,
            read_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::application::TermPart;
    use crate::profile::application::{CvTag, Freshness, state_line};

    #[test]
    fn load_fills_your_terms_from_the_checkout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample_workspace");
        let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        assert_eq!(snapshot.terms.floor, TermPart::Read(Some(700)));
        assert_eq!(snapshot.terms.gates().commute.len(), 4);
    }

    #[test]
    fn load_reads_the_cv_documents_of_the_sample() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample_workspace");
        let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        assert!(snapshot.issues.is_empty(), "{:?}", snapshot.issues);
        let CvShelf::Read { documents, build_cv } = &snapshot.cv_documents else { panic!("cv/ reads") };
        let mut stems: Vec<&str> = documents.iter().map(|doc| doc.stem.as_str()).collect();
        stems.sort();
        assert_eq!(stems, ["main_harbor", "main_tessel_group_solution_architect"]);
        // With no `.tex` beside it, a PDF has no source to be older than, so its tag reads Current.
        assert!(documents.iter().all(|doc| doc.tex.is_none() && doc.freshness == Freshness::NoSource), "the sample holds no .tex");
        let now = snapshot.today.and_hms_opt(9, 0, 0).unwrap();
        assert!(documents.iter().all(|doc| state_line(doc, false, *build_cv, now).tag == CvTag::Current));
        assert!(!build_cv, "the sample holds no /build-cv");
    }

    /// Whether the folder has `/build-cv` comes with the CV documents, read again on each load.
    #[test]
    fn load_says_whether_the_folder_has_build_cv() {
        let dir = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let build_cv = |root: &Path| match Snapshot::load(root, today).cv_documents {
            CvShelf::Read { build_cv, .. } => build_cv,
            CvShelf::Unread => panic!("cv/ reads"),
        };
        assert!(!build_cv(dir.path()));
        std::fs::create_dir_all(dir.path().join(".claude/commands")).unwrap();
        std::fs::write(dir.path().join(".claude/commands/build-cv.md"), "Build a CV.\n").unwrap();
        assert!(build_cv(dir.path()));
    }

    #[test]
    fn a_cv_folder_that_cannot_be_read_is_one_issue_and_an_unread_shelf() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cv"), b"").unwrap();
        let snapshot = Snapshot::load(dir.path(), NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        assert_eq!(snapshot.cv_documents, CvShelf::Unread);
        let issues: Vec<(Source, &str)> =
            snapshot.issues.iter().filter(|issue| issue.source == Source::CvDocuments).map(|i| (i.source, i.error.source.as_str())).collect();
        assert_eq!(issues, [(Source::CvDocuments, "cv/")]);
    }

    #[test]
    fn load_reads_the_portals_of_the_sample() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample_workspace");
        let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        let portals: Vec<(&str, bool, Option<usize>)> =
            snapshot.portals.portals.iter().map(|portal| (portal.folder.as_str(), portal.on, portal.jobs.map(|(count, _)| count))).collect();
        assert_eq!(portals, [("jobnet-search", false, Some(0)), ("linkedin-search", true, Some(6))]);
        assert!(snapshot.portals.installed_read && snapshot.portals.plan_read);
    }

    /// A `.agents/skills` that is no folder is one issue, and the view says the portals are unread, not none.
    #[test]
    fn a_portals_folder_that_cannot_be_read_is_one_issue() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".agents")).unwrap();
        std::fs::write(dir.path().join(".agents/skills"), b"").unwrap();
        let snapshot = Snapshot::load(dir.path(), NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        let issues: Vec<(Source, &str)> =
            snapshot.issues.iter().filter(|issue| issue.source == Source::Portals).map(|i| (i.source, i.error.source.as_str())).collect();
        assert_eq!(issues, [(Source::Portals, ".agents/skills")]);
        assert!(!snapshot.portals.installed_read);
    }

    /// The profile already reports a search file it cannot read, and Setup never shows the search terms, so the
    /// portals only mark their plan unread.
    #[test]
    fn a_search_file_that_cannot_be_read_is_no_portals_issue() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude/skills/job-scraper/search-queries.md")).unwrap();
        let snapshot = Snapshot::load(dir.path(), NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        assert!(snapshot.issues.iter().all(|issue| issue.source != Source::Portals), "{:?}", snapshot.issues);
        assert!(!snapshot.portals.plan_read);
        assert!(snapshot.portals.installed_read);
    }
}
