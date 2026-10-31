use crate::discovery::application::{JobBoard, JobView, Lane};
use crate::overview::domain::{AttentionItem, most_urgent_first};
use crate::tracking::application::{ApplicationCard, DeadlineState, FOLLOW_UP_DAYS, Pipeline, StatusBucket};

pub use crate::overview::domain::AttentionKind;

const CALIBRATION_MINIMUM: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Jobs,
    Tracker,
    Insights,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStage {
    pub name: &'static str,
    pub count: usize,
    pub detail: String,
    pub command: &'static str,
    pub destination: Destination,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overview {
    pub stages: Vec<WorkflowStage>,
    pub attention: Vec<AttentionItem>,
    pub resolved: usize,
    pub can_calibrate: bool,
}

pub fn compose(board: &JobBoard, pipeline: &Pipeline) -> Overview {
    let mut attention: Vec<AttentionItem> = board.jobs_in(Lane::Shortlist).iter().flat_map(job_attention).collect();
    attention.extend(board.jobs_in(Lane::BelowThreshold).iter().filter_map(guard_attention));
    attention.extend(pipeline.cards().flat_map(application_attention));
    most_urgent_first(&mut attention);

    let resolved = pipeline.cards().filter(|card| card.is_final).count();
    Overview { stages: stages(board, pipeline), attention, resolved, can_calibrate: resolved >= CALIBRATION_MINIMUM }
}

fn stages(board: &JobBoard, pipeline: &Pipeline) -> Vec<WorkflowStage> {
    let in_process = pipeline.count(StatusBucket::Active) + pipeline.count(StatusBucket::Interview) + pipeline.count(StatusBucket::Offer);
    let closed = pipeline.count(StatusBucket::Closed) + pipeline.count(StatusBucket::Hired);
    vec![
        WorkflowStage { name: "Scrape", count: board.count(Lane::New), detail: format!("new of {} seen", board.total_seen), command: "/scrape", destination: Destination::Jobs },
        WorkflowStage { name: "Rank", count: board.count(Lane::Shortlist), detail: "on shortlist".into(), command: "/rank", destination: Destination::Jobs },
        WorkflowStage { name: "Apply", count: pipeline.count(StatusBucket::Drafted), detail: "drafted, not sent".into(), command: "/apply", destination: Destination::Tracker },
        WorkflowStage {
            name: "In process",
            count: in_process,
            detail: format!("{} interview, {} offer", pipeline.count(StatusBucket::Interview), pipeline.count(StatusBucket::Offer)),
            command: "/interview",
            destination: Destination::Tracker,
        },
        WorkflowStage { name: "Outcome", count: closed, detail: "closed".into(), command: "/outcome", destination: Destination::Insights },
    ]
}

fn job_attention(job: &JobView) -> Vec<AttentionItem> {
    let title = format!("{} · {}", job.company, job.title);
    let mut items = Vec::new();
    if job.closing_soon {
        let days = job.days_to_deadline.unwrap_or_default();
        items.push(AttentionItem {
            kind: AttentionKind::ClosingSoon,
            urgency: days,
            title: title.clone(),
            why: format!("Shortlisted, deadline in {days} days, no application yet"),
            command: apply_command(job),
        });
    }
    items.extend(guard_attention(job));
    items
}

fn apply_command(job: &JobView) -> String {
    job.url.as_ref().map_or_else(|| "/apply".to_owned(), |url| format!("/apply {}", url.as_str()))
}

fn guard_attention(job: &JobView) -> Option<AttentionItem> {
    job.guard_flagged.then(|| AttentionItem {
        kind: AttentionKind::GuardFlag,
        urgency: 0,
        title: format!("{} · {}", job.company, job.title),
        why: "Posting guard flagged this posting. Read it before applying.".into(),
        command: apply_command(job),
    })
}

fn application_attention(card: &ApplicationCard) -> Option<AttentionItem> {
    let title = format!("{} · {}", card.company, card.role);
    let company = card.company.to_lowercase();
    let item = |kind, urgency, why: String, command: String| Some(AttentionItem { kind, urgency, title: title.clone(), why, command });
    if card.bucket == StatusBucket::Drafted {
        return match card.deadline_state {
            Some(DeadlineState::Passed) => item(AttentionKind::DeadlinePassed, 0, "Drafted, never sent, deadline has passed".into(), format!("/outcome {company}")),
            Some(DeadlineState::ClosingIn(days)) => item(AttentionKind::NotSent, days, format!("Drafted, deadline in {days} days"), format!("/outcome {company}")),
            _ => None,
        };
    }
    let quiet = card.days_quiet.unwrap_or_default();
    if card.stale {
        return item(AttentionKind::Stale, -quiet, format!("Quiet {quiet} days, {} follow-ups sent", card.follow_ups_sent), "/outcome stale".into());
    }
    if card.follow_up_due {
        return item(AttentionKind::FollowUpDue, -quiet, format!("Quiet {quiet} days, past the {FOLLOW_UP_DAYS}-day mark"), format!("/outcome followup {company}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::application::{LaneView, Signal};
    use crate::tracking::application::{PipelineLane, ReachedStage};

    fn card(bucket: StatusBucket) -> ApplicationCard {
        ApplicationCard {
            company: "Acme".into(), role: "Lead".into(), sector: String::new(), channel: String::new(),
            status_label: String::new(), bucket, applied_on: String::new(), deadline: None, deadline_state: None,
            contact_person: String::new(), fit_rating: None, notes: String::new(), source: None, days_quiet: None,
            follow_ups_sent: 0, follow_up_due: false, stale: false, reached: ReachedStage::Applied,
            is_rejection: false, is_final: false, cv_file: String::new(), cover_letter_file: String::new(),
            archive_folder: None, outcome: None,
        }
    }

    fn job(days: i64) -> JobView {
        JobView {
            key: "k".into(), title: "Lead".into(), company: "Beta".into(), location: None, url: None, portal: None,
            deadline: None, days_to_deadline: Some(days), score: Some(80), verdict: None, scraper_fit: None,
            location_gate: None, language_gate: None, language_note: None, guard: None, strengths: vec![], gaps: vec![],
            fetch_attempts: None, fetch_error: None, signals: vec![Signal::ClosingIn { days }], closing_soon: true, guard_flagged: false,
        }
    }

    #[test]
    fn orders_by_urgency_and_never_chases_drafts() {
        let board = JobBoard { lanes: vec![LaneView { lane: Lane::Shortlist, jobs: vec![job(4)] }], total_seen: 1 };
        let quiet = ApplicationCard { follow_up_due: true, days_quiet: Some(12), ..card(StatusBucket::Active) };
        let passed = ApplicationCard { deadline_state: Some(DeadlineState::Passed), ..card(StatusBucket::Drafted) };
        let draft = ApplicationCard { follow_up_due: true, ..card(StatusBucket::Drafted) };
        let pipeline = Pipeline { lanes: vec![
            PipelineLane { bucket: StatusBucket::Active, cards: vec![quiet] },
            PipelineLane { bucket: StatusBucket::Drafted, cards: vec![draft, passed] },
        ] };

        let kinds: Vec<_> = compose(&board, &pipeline).attention.iter().map(|item| item.kind).collect();

        assert_eq!(kinds, [AttentionKind::DeadlinePassed, AttentionKind::ClosingSoon, AttentionKind::FollowUpDue]);
    }

    #[test]
    fn calibration_needs_three_resolved() {
        let resolved = ApplicationCard { is_final: true, ..card(StatusBucket::Closed) };
        let pipeline = Pipeline { lanes: vec![PipelineLane { bucket: StatusBucket::Closed, cards: vec![resolved; 3] }] };
        assert!(compose(&JobBoard::default(), &pipeline).can_calibrate);
    }
}
