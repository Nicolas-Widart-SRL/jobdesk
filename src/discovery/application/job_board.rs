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
    /// The day `/scrape` first saw the posting.
    pub first_seen: Option<NaiveDate>,
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

    /// How many saved jobs came from `portal`, a folder name such as `linkedin-search`, and the latest day one of them was first seen.
    pub fn portal_jobs(&self, portal: &str) -> (usize, Option<NaiveDate>) {
        let jobs = self.lanes.iter().flat_map(|view| &view.jobs).filter(|job| job.portal.as_deref() == Some(portal));
        jobs.fold((0, None), |(count, latest), job| (count + 1, latest.max(job.first_seen)))
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
                in_lane.sort_by_key(|job| (Reverse(job.score()), Reverse(job.is_closing_soon(today)), Reverse(job.scraper_fit), job.company.to_lowercase()));
                LaneView { lane, jobs: in_lane.into_iter().map(|job| view_of(job, today, terms)).collect() }
            })
            .collect();
        Ok(JobBoard { lanes, total_seen })
    }
}

fn view_of(job: &Job, today: NaiveDate, terms: &FreelanceTerms) -> JobView {
    let assessment = job.assessment.as_ref();
    let (fetch_attempts, fetch_error) =
        job.fetch_failure.as_ref().map_or((None, None), |FetchFailure { attempts, last_error }| (Some(*attempts), last_error.clone()));
    JobView {
        key: job.key.clone(),
        title: job.title.clone(),
        company: job.company.clone(),
        location: job.location.clone(),
        url: job.url.clone(),
        portal: job.portal.clone(),
        found_on: job.portal.as_deref().map(portal_name).filter(|name| !name.is_empty()),
        first_seen: job.first_seen,
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
        Job { company: company.into(), deadline, assessment: Some(assessed(score, Gate::Pass, Gate::Pass)), ..job(JobStatus::Ranked) }
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

    #[test]
    fn counts_a_portals_jobs_and_its_latest_first_seen() {
        let day = |d| NaiveDate::from_ymd_opt(2026, 10, d);
        let on = |portal: &str, first_seen| JobView { portal: Some(portal.into()), first_seen, ..fixtures::view(portal) };
        let jobs = [on("north-search", day(28)), on("north-search", day(30)), on("north-search", None), on("south-search", day(31))];
        let board = fixtures::board(vec![(Lane::Shortlist, jobs[..2].to_vec()), (Lane::New, jobs[2..].to_vec())]);

        assert_eq!(board.portal_jobs("north-search"), (3, day(30)));
        assert_eq!(board.portal_jobs("south-search"), (1, day(31)));
        assert_eq!(board.portal_jobs("west-search"), (0, None));
    }
}

/// Jobs and boards for tests, built in memory.
#[cfg(test)]
pub mod fixtures {
    use super::*;
    use crate::discovery::domain::GateReading;

    /// A scored job that passes every check, keyed and named by `key`.
    pub fn view(key: &str) -> JobView {
        let pass = || GateReading { gate: Gate::Pass, value: "x".into() };
        JobView {
            key: key.into(),
            title: format!("Lead {key}"),
            company: format!("Company {key}"),
            location: None,
            url: None,
            portal: None,
            found_on: None,
            first_seen: None,
            deadline: None,
            days_to_deadline: None,
            score: Some(80),
            verdict: Some(Verdict::Good),
            scraper_fit: None,
            location_gate: Some(Gate::Pass),
            language_gate: Some(Gate::Pass),
            language_note: None,
            freelance: FreelanceGates { contract: pass(), day_rate: pass(), commute: pass() },
            guard: Some("clean".into()),
            strengths: vec![],
            gaps: vec![],
            fetch_attempts: None,
            fetch_error: None,
            signals: vec![],
            closing_soon: false,
            guard_flagged: false,
        }
    }

    /// A board with these jobs in these lists and every other list empty.
    pub fn board(lanes: Vec<(Lane, Vec<JobView>)>) -> JobBoard {
        let total_seen = lanes.iter().map(|(_, jobs)| jobs.len()).sum();
        JobBoard { lanes: lanes.into_iter().map(|(lane, jobs)| LaneView { lane, jobs }).collect(), total_seen }
    }
}
