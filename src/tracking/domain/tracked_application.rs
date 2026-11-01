use chrono::NaiveDate;

use crate::shared_kernel::WebUrl;
use crate::shared_kernel::dates::{days_from, iso_dates_in};
use crate::tracking::domain::{AppliedOn, ApplicationStatus, OutcomeRecord};

const CLOSING_SOON_DAYS: i64 = 7;
pub const MAX_FOLLOW_UPS: usize = 2;
const FOLLOW_UP_MARKER: &str = "followed up ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlineState {
    Open,
    ClosingIn(i64),
    Passed,
}

/// The furthest point an application got to, from history rather than current status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReachedStage {
    Drafted,
    Applied,
    Interview,
    Offer,
    Hired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedApplication {
    pub company: String,
    pub role: String,
    pub sector: String,
    pub channel: String,
    pub status: ApplicationStatus,
    pub applied_on: AppliedOn,
    pub deadline: Option<NaiveDate>,
    pub contact_person: String,
    pub fit_rating: Option<u8>,
    pub notes: String,
    pub source: Option<WebUrl>,
    pub cv_file: String,
    pub cover_letter_file: String,
    pub outcome: Option<OutcomeRecord>,
}

impl TrackedApplication {
    pub fn is_open(&self) -> bool {
        !self.status.is_final()
    }

    /// Nobody is late replying to something that was never sent.
    fn was_submitted(&self) -> bool {
        self.status != ApplicationStatus::Drafted
    }

    fn follow_up_dates(&self) -> Vec<NaiveDate> {
        self.notes
            .match_indices(FOLLOW_UP_MARKER)
            .flat_map(|(at, marker)| iso_dates_in(&self.notes[at + marker.len()..]).into_iter().take(1))
            .collect()
    }

    pub fn follow_ups_sent(&self) -> usize {
        self.follow_up_dates().len()
    }

    /// Days since `date` or the latest dated note, whichever is more recent.
    pub fn days_quiet(&self, today: NaiveDate) -> Option<i64> {
        if !self.was_submitted() {
            return None;
        }
        let last = self.applied_on.date().into_iter().chain(iso_dates_in(&self.notes)).max()?;
        Some(days_from(last, today))
    }

    /// Days since the application went out, from its date alone. A date ahead of today reads as today.
    pub fn days_since_sent(&self, today: NaiveDate) -> Option<i64> {
        let sent = self.applied_on.date().filter(|_| self.was_submitted())?;
        Some(days_from(sent, today).max(0))
    }

    pub fn follow_up_is_due(&self, today: NaiveDate, threshold_days: i64) -> bool {
        let sent = self.follow_up_dates();
        let last = sent.iter().copied().max().or(self.applied_on.date());
        self.is_open()
            && self.was_submitted()
            && sent.len() < MAX_FOLLOW_UPS
            && last.is_some_and(|last| days_from(last, today) >= threshold_days)
            && self.days_quiet(today).is_some_and(|days| days >= threshold_days)
    }

    pub fn is_stale(&self, today: NaiveDate, threshold_days: i64) -> bool {
        matches!(self.status, ApplicationStatus::Applied | ApplicationStatus::Interview)
            && self.days_quiet(today).is_some_and(|days| days >= threshold_days)
    }

    pub fn deadline_state(&self, today: NaiveDate) -> Option<DeadlineState> {
        let days = days_from(today, self.deadline?);
        Some(match days {
            ..0 => DeadlineState::Passed,
            0..=CLOSING_SOON_DAYS => DeadlineState::ClosingIn(days),
            _ => DeadlineState::Open,
        })
    }

    pub fn reached(&self) -> ReachedStage {
        let by_status = match self.status {
            ApplicationStatus::Drafted => ReachedStage::Drafted,
            ApplicationStatus::Interview => ReachedStage::Interview,
            ApplicationStatus::Offer | ApplicationStatus::OfferDeclined => ReachedStage::Offer,
            ApplicationStatus::Hired => ReachedStage::Hired,
            _ => ReachedStage::Applied,
        };
        let by_outcome = match &self.outcome {
            Some(outcome) if outcome.reached_offer() => ReachedStage::Offer,
            Some(outcome) if outcome.reached_interview() => ReachedStage::Interview,
            _ => ReachedStage::Drafted,
        };
        by_status.max(by_outcome)
    }
}

#[cfg(test)]
pub mod fixtures {
    use super::*;

    pub fn application(status: ApplicationStatus, applied_on: &str, notes: &str) -> TrackedApplication {
        TrackedApplication {
            company: "Acme".into(),
            role: "Lead".into(),
            sector: String::new(),
            channel: String::new(),
            status,
            applied_on: AppliedOn::parse(applied_on),
            deadline: None,
            contact_person: String::new(),
            fit_rating: None,
            notes: notes.into(),
            source: None,
            cv_file: String::new(),
            cover_letter_file: String::new(),
            outcome: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::application;
    use super::*;
    use crate::tracking::domain::Stage;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
    }

    #[test]
    fn quiet_days_count_from_the_latest_dated_note() {
        let app = application(ApplicationStatus::Applied, "2026-10-01", "called 2026-10-21");
        assert_eq!(app.days_quiet(today()), Some(10));
    }

    #[test]
    fn days_since_sent_ignore_notes_and_never_go_negative() {
        let followed = application(ApplicationStatus::Applied, "2026-10-01", "followed up 2026-10-28");
        assert_eq!((followed.days_since_sent(today()), followed.days_quiet(today())), (Some(30), Some(3)));
        let ahead = application(ApplicationStatus::Applied, "2026-11-05", "");
        assert_eq!(ahead.days_since_sent(today()), Some(0));
        assert_eq!(application(ApplicationStatus::Drafted, "2026-10-01", "").days_since_sent(today()), None);
    }

    #[test]
    fn drafted_rows_are_never_quiet_or_chased() {
        let app = application(ApplicationStatus::Drafted, "2026-01-01", "");
        assert_eq!(app.days_quiet(today()), None);
        assert!(!app.follow_up_is_due(today(), 10));
        assert!(!app.is_stale(today(), 60));
    }

    #[test]
    fn follow_up_threshold_restarts_after_the_last_marker() {
        let recent = application(ApplicationStatus::Applied, "2026-09-01", "followed up 2026-10-29");
        let waiting = application(ApplicationStatus::Applied, "2026-09-01", "followed up 2026-10-10");
        assert_eq!(recent.follow_ups_sent(), 1);
        assert!(!recent.follow_up_is_due(today(), 10));
        assert!(waiting.follow_up_is_due(today(), 10));
    }

    #[test]
    fn recent_contact_postpones_the_follow_up() {
        let app = application(ApplicationStatus::Offer, "2026-09-01", "offer received 2026-10-27");
        assert!(!app.follow_up_is_due(today(), 10));
    }

    #[test]
    fn no_third_follow_up() {
        let app = application(ApplicationStatus::Applied, "2026-08-01", "followed up 2026-08-20 followed up 2026-09-10");
        assert!(!app.follow_up_is_due(today(), 10));
    }

    #[test]
    fn stale_uses_only_submitted_open_statuses() {
        assert!(application(ApplicationStatus::Applied, "2026-08-01", "").is_stale(today(), 60));
        assert!(!application(ApplicationStatus::Offer, "2026-08-01", "").is_stale(today(), 60));
    }

    #[test]
    fn deadline_states() {
        let with = |deadline| TrackedApplication { deadline: Some(deadline), ..application(ApplicationStatus::Drafted, "2026-10-01", "") };
        assert_eq!(with(today().pred_opt().unwrap()).deadline_state(today()), Some(DeadlineState::Passed));
        assert_eq!(with(today()).deadline_state(today()), Some(DeadlineState::ClosingIn(0)));
        assert_eq!(with(NaiveDate::from_ymd_opt(2026, 11, 8).unwrap()).deadline_state(today()), Some(DeadlineState::Open));
    }

    #[test]
    fn rejected_after_interview_still_reached_interview() {
        let outcome = OutcomeRecord { stages: vec![Stage { name: "Phone screen".into(), reached: true, date: None }], ..Default::default() };
        let app = TrackedApplication { outcome: Some(outcome), ..application(ApplicationStatus::Rejected, "2026-10-01", "") };
        assert_eq!(app.reached(), ReachedStage::Interview);
    }

    #[test]
    fn hired_reached_every_stage() {
        assert_eq!(application(ApplicationStatus::Hired, "2026-10-01", "").reached(), ReachedStage::Hired);
    }
}
