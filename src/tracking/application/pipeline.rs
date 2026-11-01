use std::cmp::Reverse;

use chrono::NaiveDate;

use crate::shared_kernel::{LoadError, WebUrl};
use crate::tracking::domain::{ApplicationRepository, DeadlineState, OutcomeRecord, ReachedStage, StatusBucket, TrackedApplication, archive_folder};

/// The `/outcome` defaults.
pub const FOLLOW_UP_DAYS: i64 = 10;
pub const STALE_DAYS: i64 = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct ApplicationCard {
    pub company: String,
    pub role: String,
    pub sector: String,
    pub channel: String,
    pub status_label: String,
    pub bucket: StatusBucket,
    pub applied_on: String,
    pub deadline: Option<NaiveDate>,
    pub deadline_state: Option<DeadlineState>,
    pub contact_person: String,
    pub fit_rating: Option<u8>,
    pub notes: String,
    pub source: Option<WebUrl>,
    /// Days since the latest dated note or the applied date, so it restarts with each follow-up.
    pub days_quiet: Option<i64>,
    /// Days since the applied date alone, never below zero.
    pub days_sent: Option<i64>,
    pub follow_ups_sent: usize,
    pub follow_up_due: bool,
    pub stale: bool,
    pub reached: ReachedStage,
    pub is_rejection: bool,
    pub is_final: bool,
    pub cv_file: String,
    pub cover_letter_file: String,
    pub archive_folder: Option<String>,
    pub outcome: Option<OutcomeRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineLane {
    pub bucket: StatusBucket,
    pub cards: Vec<ApplicationCard>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pipeline {
    pub lanes: Vec<PipelineLane>,
}

impl Pipeline {
    pub fn cards(&self) -> impl Iterator<Item = &ApplicationCard> {
        self.lanes.iter().flat_map(|lane| &lane.cards)
    }

    pub fn count(&self, bucket: StatusBucket) -> usize {
        self.lanes.iter().find(|lane| lane.bucket == bucket).map_or(0, |lane| lane.cards.len())
    }

    pub fn open_count(&self) -> usize {
        self.cards().filter(|card| !card.is_final).count()
    }

    pub fn follow_ups_due(&self) -> usize {
        self.cards().filter(|card| card.follow_up_due).count()
    }

    pub fn stale_count(&self) -> usize {
        self.cards().filter(|card| card.stale).count()
    }

    pub fn card_in_folder(&self, folder: &str) -> Option<&ApplicationCard> {
        self.cards().find(|card| card.archive_folder.as_deref() == Some(folder))
    }

    pub fn is_empty(&self) -> bool {
        self.cards().next().is_none()
    }
}

pub struct PipelineService<R> {
    applications: R,
}

impl<R: ApplicationRepository> PipelineService<R> {
    pub fn new(applications: R) -> Self {
        Self { applications }
    }

    pub fn pipeline(&self, today: NaiveDate) -> Result<Pipeline, LoadError> {
        let applications = self.applications.all()?;
        let lanes = StatusBucket::ALL
            .into_iter()
            .map(|bucket| {
                let mut cards: Vec<_> = applications.iter().filter(|a| a.status.bucket() == bucket).map(|a| card_of(a, today)).collect();
                cards.sort_by_key(|card| (Reverse(card.days_quiet), Reverse(card.applied_on.clone())));
                PipelineLane { bucket, cards }
            })
            .collect();
        Ok(Pipeline { lanes })
    }
}

fn card_of(application: &TrackedApplication, today: NaiveDate) -> ApplicationCard {
    ApplicationCard {
        company: application.company.clone(),
        role: application.role.clone(),
        sector: application.sector.clone(),
        channel: application.channel.clone(),
        status_label: application.status.label().to_owned(),
        bucket: application.status.bucket(),
        applied_on: application.applied_on.label(),
        deadline: application.deadline,
        deadline_state: application.deadline_state(today),
        contact_person: application.contact_person.clone(),
        fit_rating: application.fit_rating,
        notes: application.notes.clone(),
        source: application.source.clone(),
        days_quiet: application.days_quiet(today),
        days_sent: application.days_since_sent(today),
        follow_ups_sent: application.follow_ups_sent(),
        follow_up_due: application.follow_up_is_due(today, FOLLOW_UP_DAYS),
        stale: application.is_stale(today, STALE_DAYS),
        reached: application.reached(),
        is_rejection: application.status.is_rejection(),
        is_final: application.status.is_final(),
        cv_file: application.cv_file.clone(),
        cover_letter_file: application.cover_letter_file.clone(),
        archive_folder: archive_folder(&application.company, &application.role),
        outcome: application.outcome.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracking::domain::ApplicationStatus;
    use crate::tracking::domain::fixtures::application;

    struct InMemory(Vec<TrackedApplication>);

    impl ApplicationRepository for InMemory {
        fn all(&self) -> Result<Vec<TrackedApplication>, LoadError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn groups_cards_and_counts_what_needs_action() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        let pipeline = PipelineService::new(InMemory(vec![
            application(ApplicationStatus::Applied, "2026-08-01", ""),
            application(ApplicationStatus::Applied, "2026-10-15", ""),
            application(ApplicationStatus::Drafted, "2026-10-30", ""),
            application(ApplicationStatus::Rejected, "2026-05-01", ""),
        ]))
        .pipeline(today)
        .unwrap();

        assert_eq!(pipeline.count(StatusBucket::Active), 2);
        assert_eq!(pipeline.open_count(), 3);
        assert_eq!(pipeline.follow_ups_due(), 2);
        assert_eq!(pipeline.stale_count(), 1);
        assert_eq!(pipeline.lanes[1].cards[0].days_quiet, Some(91));
    }
}
