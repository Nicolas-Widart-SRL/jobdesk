//! Finding and triaging postings: the output of `/scrape` and `/rank`.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;

use chrono::NaiveDate;

use crate::discovery::application::{Commute, FreelanceTerms, JobBoard, JobBoardService, TermPart, YourTerms};
use crate::discovery::infrastructure::{SeenJobsFile, read_your_terms};
use crate::shared_kernel::LoadError;

pub fn load_board(root: &Path, today: NaiveDate, terms: &FreelanceTerms) -> Result<JobBoard, LoadError> {
    JobBoardService::new(SeenJobsFile::in_workspace(root)).board(today, terms)
}

/// Your day-rate floor and commute tiers, each with whether its file could be read.
pub fn load_terms(root: &Path) -> YourTerms {
    let read = read_your_terms(root);
    YourTerms {
        floor: match read.floor {
            Ok(floor) => TermPart::Read(floor),
            Err(error) => TermPart::Failed(error),
        },
        commute: match read.commute {
            Ok((origin, tiers)) => TermPart::Read(Commute { origin, tiers }),
            Err(error) => TermPart::Failed(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::discovery::application::CommuteTier;

    const SKILLS: &str = ".claude/skills/job-application-assistant";
    const PROFILE: &str = "01-candidate-profile.md";
    const EVALUATION: &str = "04-job-evaluation.md";
    const TIERS: &str = "Commute tiers from Namur (by car): **ideal** Namur (<45 min)";

    fn checkout(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(SKILLS)).unwrap();
        for (name, bytes) in files {
            fs::write(dir.path().join(SKILLS).join(name), bytes).unwrap();
        }
        dir
    }

    fn tiers_of(terms: &YourTerms) -> Vec<CommuteTier> {
        match &terms.commute {
            TermPart::Read(commute) => commute.tiers.clone(),
            TermPart::Failed(error) => panic!("unexpected failure: {error}"),
        }
    }

    #[test]
    fn a_missing_profile_reads_as_not_stated() {
        let dir = checkout(&[(EVALUATION, TIERS.as_bytes())]);
        let terms = load_terms(dir.path());
        assert_eq!(terms.floor, TermPart::Read(None));
        assert_eq!(terms.not_stated(), 1);
        assert!(terms.failures().is_empty());
        assert_eq!(terms.gates(), FreelanceTerms { day_rate_floor: None, commute: tiers_of(&terms) });
    }

    #[test]
    fn both_missing_are_not_stated() {
        let terms = load_terms(checkout(&[]).path());
        assert_eq!(terms.not_stated(), 2);
        assert_eq!(terms.gates(), FreelanceTerms::default());
    }

    #[test]
    fn an_unreadable_profile_fails_its_own_part_only() {
        let dir = checkout(&[(PROFILE, &[0xff, 0xfe]), (EVALUATION, TIERS.as_bytes())]);
        let terms = load_terms(dir.path());
        let TermPart::Failed(error) = &terms.floor else { panic!("the floor should have failed") };
        assert_eq!(error.source, PROFILE);
        assert!(matches!(terms.commute, TermPart::Read(_)));
        assert_eq!(terms.not_stated(), 0);
        assert_eq!(terms.gates(), FreelanceTerms { day_rate_floor: None, commute: tiers_of(&terms) });
    }

    #[test]
    fn an_unreadable_evaluation_fails_its_own_part_only() {
        let dir = checkout(&[(PROFILE, b"- day-rate floor EUR 700."), (EVALUATION, &[0xff, 0xfe])]);
        let terms = load_terms(dir.path());
        let TermPart::Failed(error) = &terms.commute else { panic!("the commute should have failed") };
        assert_eq!(error.source, EVALUATION);
        assert_eq!(terms.floor, TermPart::Read(Some(700)));
        assert_eq!(terms.gates(), FreelanceTerms { day_rate_floor: Some(700), commute: vec![] });
    }

    #[test]
    fn the_sample_workspace_states_both_terms() {
        let terms = load_terms(Path::new("tests/sample_workspace"));
        assert_eq!(terms.floor, TermPart::Read(Some(700)));
        let TermPart::Read(commute) = &terms.commute else { panic!("the commute should have been read") };
        assert_eq!(commute.origin.as_deref(), Some("from Namur (by car)"));
        assert_eq!(commute.tiers.len(), 4);
        assert_eq!(terms.not_stated(), 0);
        assert_eq!(terms.origin_line().as_deref(), Some("From Namur (by car)."));
    }

    #[test]
    fn a_tier_loses_its_bidi_override() {
        let dir = checkout(&[(EVALUATION, "Commute tiers from Namur: **ideal** Na\u{202e}mur (<45 min)".as_bytes())]);
        assert_eq!(tiers_of(&load_terms(dir.path()))[0].places, vec!["Namur".to_owned()]);
    }
}
