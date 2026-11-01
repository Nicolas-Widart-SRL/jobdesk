use std::cmp::Reverse;

use chrono::NaiveDate;

use crate::discovery::domain::{FetchFailure, FitLevel, FreelanceGates, FreelanceTerms, Gate, Job, JobRepository, Lane, Signal, Verdict, portal_name};
use crate::shared_kernel::{LoadError, WebUrl};

#[derive(Debug, Clone, PartialEq)]
pub struct JobView {
    pub key: String,
    pub title: String,
    pub company: String,
    pub location: Option<String>,
    pub url: Option<WebUrl>,
    pub portal: Option<String>,
    /// The site the posting came from, in words: `linkedin-search` reads as LinkedIn.
    pub found_on: Option<String>,
    pub deadline: Option<NaiveDate>,
    pub days_to_deadline: Option<i64>,
    pub score: Option<u8>,
    pub verdict: Option<Verdict>,
    pub scraper_fit: Option<FitLevel>,
    pub location_gate: Option<Gate>,
    pub language_gate: Option<Gate>,
    pub language_note: Option<String>,
    pub freelance: FreelanceGates,
    pub guard: Option<String>,
    pub strengths: Vec<String>,
    pub gaps: Vec<String>,
    pub fetch_attempts: Option<u32>,
    pub fetch_error: Option<String>,
    pub signals: Vec<Signal>,
    pub closing_soon: bool,
    pub guard_flagged: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaneView {
    pub lane: Lane,
    pub jobs: Vec<JobView>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct JobBoard {
    pub lanes: Vec<LaneView>,
    pub total_seen: usize,
}

impl JobBoard {
    pub fn jobs_in(&self, lane: Lane) -> &[JobView] {
        self.lanes.iter().find(|view| view.lane == lane).map_or(&[], |view| &view.jobs)
    }

    pub fn seen_on_portal(&self, portal: &str) -> usize {
        self.lanes.iter().flat_map(|view| &view.jobs).filter(|job| job.portal.as_deref() == Some(portal)).count()
    }

    pub fn count(&self, lane: Lane) -> usize {
        self.jobs_in(lane).len()
    }
}

pub struct JobBoardService<R> {
    jobs: R,
}

impl<R: JobRepository> JobBoardService<R> {
    pub fn new(jobs: R) -> Self {
        Self { jobs }
    }

    pub fn board(&self, today: NaiveDate, terms: &FreelanceTerms) -> Result<JobBoard, LoadError> {
        let jobs = self.jobs.all()?;
        let total_seen = jobs.len();
        let lanes = Lane::ALL
            .into_iter()
            .map(|lane| {
                let mut in_lane: Vec<&Job> = jobs.iter().filter(|job| job.lane() == lane).collect();
                in_lane.sort_by_key(|job| {
                    (Reverse(job.score()), Reverse(job.is_closing_soon(today)), Reverse(job.scraper_fit), job.company.to_lowercase())
                });
                LaneView { lane, jobs: in_lane.into_iter().map(|job| view_of(job, today, terms)).collect() }
            })
            .collect();
        Ok(JobBoard { lanes, total_seen })
    }
}

fn view_of(job: &Job, today: NaiveDate, terms: &FreelanceTerms) -> JobView {
    let assessment = job.assessment.as_ref();
    let (fetch_attempts, fetch_error) = job
        .fetch_failure
        .as_ref()
        .map_or((None, None), |FetchFailure { attempts, last_error }| (Some(*attempts), last_error.clone()));
    JobView {
        key: job.key.clone(),
        title: job.title.clone(),
        company: job.company.clone(),
        location: job.location.clone(),
        url: job.url.clone(),
        portal: job.portal.clone(),
        found_on: job.portal.as_deref().map(portal_name).filter(|name| !name.is_empty()),
        deadline: job.deadline,
        days_to_deadline: job.days_to_deadline(today),
        score: job.score(),
        verdict: assessment.map(|a| a.verdict),
        scraper_fit: job.scraper_fit,
        location_gate: assessment.map(|a| a.location),
        language_gate: assessment.map(|a| a.language),
        language_note: assessment.and_then(|a| a.language_note.clone()),
        freelance: job.freelance_gates(terms),
        guard: job.guard.clone(),
        strengths: assessment.map(|a| a.strengths.clone()).unwrap_or_default(),
        gaps: assessment.map(|a| a.gaps.clone()).unwrap_or_default(),
        fetch_attempts,
        fetch_error,
        signals: job.signals(today, terms),
        closing_soon: job.is_closing_soon(today),
        guard_flagged: job.guard_flagged(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::domain::JobStatus;
    use crate::discovery::domain::fixtures::{assessed, job};

    struct InMemory(Vec<Job>);

    impl JobRepository for InMemory {
        fn all(&self) -> Result<Vec<Job>, LoadError> {
            Ok(self.0.clone())
        }
    }

    fn ranked(company: &str, score: u8, deadline: Option<NaiveDate>) -> Job {
        Job {
            company: company.into(),
            deadline,
            assessment: Some(assessed(score, Gate::Pass, Gate::Pass)),
            ..job(JobStatus::Ranked)
        }
    }

    #[test]
    fn shortlist_sorts_by_score_then_urgency() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        let soon = NaiveDate::from_ymd_opt(2026, 11, 2);
        let service = JobBoardService::new(InMemory(vec![ranked("Calm", 80, None), ranked("Urgent", 80, soon), ranked("Top", 90, None)]));

        let board = service.board(today, &FreelanceTerms::default()).unwrap();

        let order: Vec<_> = board.jobs_in(Lane::Shortlist).iter().map(|j| j.company.as_str()).collect();
        assert_eq!(order, ["Top", "Urgent", "Calm"]);
        assert_eq!(board.total_seen, 3);
    }
}
