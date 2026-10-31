use chrono::NaiveDate;

use crate::discovery::domain::{Gate, RankAssessment};
use crate::shared_kernel::WebUrl;
use crate::shared_kernel::dates::days_from;

const SHORTLIST_FLOOR: u8 = 60;
const CLOSING_SOON_DAYS: i64 = 7;
const STALE_AFTER_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FitLevel {
    Low,
    Medium,
    High,
}

impl FitLevel {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "high" => Some(Self::High),
            "medium" => Some(Self::Medium),
            "low" => Some(Self::Low),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    New,
    Skipped,
    Ranked,
    Expired,
    Unreachable,
    Other(String),
}

impl JobStatus {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "new" => Self::New,
            "skipped" => Self::Skipped,
            "ranked" => Self::Ranked,
            "expired" => Self::Expired,
            "unreachable" => Self::Unreachable,
            other => Self::Other(other.to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchFailure {
    pub attempts: u32,
    pub last_error: Option<String>,
}

/// Where a job belongs in triage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Shortlist,
    BelowThreshold,
    Excluded,
    CouldNotFetch,
    New,
    Skipped,
    Expired,
}

impl Lane {
    pub const ALL: [Self; 7] = [Self::Shortlist, Self::BelowThreshold, Self::Excluded, Self::CouldNotFetch, Self::New, Self::Skipped, Self::Expired];
}

/// A warning shown beside a job, never a veto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    ClosingIn { days: i64 },
    PostedDaysAgo { days: i64 },
    LocationFlag,
    LanguageFlag,
    LocationFail,
    LanguageFail,
    GuardFlag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub key: String,
    pub title: String,
    pub company: String,
    pub location: Option<String>,
    pub url: Option<WebUrl>,
    pub portal: Option<String>,
    pub first_seen: Option<NaiveDate>,
    pub posted_on: Option<NaiveDate>,
    pub deadline: Option<NaiveDate>,
    pub scraper_fit: Option<FitLevel>,
    pub status: JobStatus,
    pub assessment: Option<RankAssessment>,
    pub fetch_failure: Option<FetchFailure>,
    pub guard: Option<String>,
}

impl Job {
    pub fn lane(&self) -> Lane {
        match (&self.status, &self.assessment) {
            (JobStatus::Ranked, Some(a)) if a.is_vetoed() => Lane::Excluded,
            (JobStatus::Ranked, Some(a)) if a.score >= SHORTLIST_FLOOR => Lane::Shortlist,
            (JobStatus::Ranked, _) => Lane::BelowThreshold,
            (JobStatus::Unreachable, _) => Lane::CouldNotFetch,
            (JobStatus::Skipped, _) => Lane::Skipped,
            (JobStatus::Expired, _) => Lane::Expired,
            (JobStatus::New | JobStatus::Other(_), _) => Lane::New,
        }
    }

    pub fn score(&self) -> Option<u8> {
        self.assessment.as_ref().map(|a| a.score)
    }

    pub fn days_to_deadline(&self, today: NaiveDate) -> Option<i64> {
        self.deadline.map(|deadline| days_from(today, deadline))
    }

    pub fn is_closing_soon(&self, today: NaiveDate) -> bool {
        self.days_to_deadline(today).is_some_and(|days| (0..=CLOSING_SOON_DAYS).contains(&days))
    }

    pub fn guard_flagged(&self) -> bool {
        self.guard.as_deref().is_some_and(|line| line.trim() != "clean")
    }

    pub fn signals(&self, today: NaiveDate) -> Vec<Signal> {
        let mut signals = Vec::new();
        if self.is_closing_soon(today) {
            signals.extend(self.days_to_deadline(today).map(|days| Signal::ClosingIn { days }));
        }
        if let Some(a) = &self.assessment {
            match a.location {
                Gate::Fail => signals.push(Signal::LocationFail),
                Gate::Flag => signals.push(Signal::LocationFlag),
                Gate::Pass => {}
            }
            match a.language {
                Gate::Fail => signals.push(Signal::LanguageFail),
                Gate::Flag => signals.push(Signal::LanguageFlag),
                Gate::Pass => {}
            }
        }
        if let Some(days) = self.posted_on.map(|posted| days_from(posted, today)) {
            if days > STALE_AFTER_DAYS {
                signals.push(Signal::PostedDaysAgo { days });
            }
        }
        if self.guard_flagged() {
            signals.push(Signal::GuardFlag);
        }
        signals
    }
}

#[cfg(test)]
pub mod fixtures {
    use super::*;
    use crate::discovery::domain::Verdict;

    pub fn job(status: JobStatus) -> Job {
        Job {
            key: "acme_lead".into(),
            title: "Lead".into(),
            company: "Acme".into(),
            location: None,
            url: None,
            portal: None,
            first_seen: None,
            posted_on: None,
            deadline: None,
            scraper_fit: None,
            status,
            assessment: None,
            fetch_failure: None,
            guard: None,
        }
    }

    pub fn assessed(score: u8, location: Gate, language: Gate) -> RankAssessment {
        RankAssessment {
            score,
            verdict: Verdict::from_score(score),
            location,
            language,
            language_note: None,
            strengths: vec![],
            gaps: vec![],
            ranked_on: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{assessed, job};
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn ranked(score: u8, location: Gate, language: Gate) -> Job {
        Job { assessment: Some(assessed(score, location, language)), ..job(JobStatus::Ranked) }
    }

    #[test]
    fn veto_beats_a_high_score() {
        assert_eq!(ranked(90, Gate::Fail, Gate::Pass).lane(), Lane::Excluded);
        assert_eq!(ranked(90, Gate::Pass, Gate::Fail).lane(), Lane::Excluded);
    }

    #[test]
    fn flag_stays_ranked() {
        assert_eq!(ranked(90, Gate::Flag, Gate::Flag).lane(), Lane::Shortlist);
        assert_eq!(ranked(59, Gate::Pass, Gate::Pass).lane(), Lane::BelowThreshold);
    }

    #[test]
    fn unknown_status_is_treated_as_new() {
        assert_eq!(job(JobStatus::parse("archived")).lane(), Lane::New);
    }

    #[test]
    fn closing_soon_is_seven_days_and_not_past() {
        let today = date(2026, 10, 31);
        let with = |deadline| Job { deadline: Some(deadline), ..job(JobStatus::Ranked) };
        assert!(with(date(2026, 11, 7)).is_closing_soon(today));
        assert!(!with(date(2026, 11, 8)).is_closing_soon(today));
        assert!(!with(date(2026, 10, 30)).is_closing_soon(today));
    }

    #[test]
    fn stale_flag_needs_a_posted_date() {
        let today = date(2026, 10, 31);
        assert!(job(JobStatus::New).signals(today).is_empty());
        let old = Job { posted_on: Some(date(2026, 9, 1)), ..job(JobStatus::New) };
        assert_eq!(old.signals(today), vec![Signal::PostedDaysAgo { days: 60 }]);
    }

    #[test]
    fn clean_guard_is_not_a_flag() {
        let clean = Job { guard: Some("clean".into()), ..job(JobStatus::New) };
        let flagged = Job { guard: Some("flagged: 1 hidden block".into()), ..job(JobStatus::New) };
        assert!(!clean.guard_flagged());
        assert!(flagged.guard_flagged());
    }
}
