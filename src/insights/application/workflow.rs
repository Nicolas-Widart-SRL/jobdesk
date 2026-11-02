use crate::discovery::application::{JobBoard, Lane};
use crate::insights::application::SearchInsights;

/// The whole workflow in counts, from the profile to the outcome of each application.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkflowBand {
    pub profile_filled: usize,
    pub profile_total: usize,
    pub new_postings: usize,
    pub seen: usize,
    pub shortlisted: usize,
    pub drafted: usize,
    pub active: usize,
    pub interview: usize,
    pub offer: usize,
    pub hired: usize,
    pub closed: usize,
}

/// `profile` is `(filled, total)` skill files.
pub fn workflow_band(report: &SearchInsights, board: &JobBoard, profile: (usize, usize)) -> WorkflowBand {
    let tally = &report.tally;
    WorkflowBand {
        profile_filled: profile.0,
        profile_total: profile.1,
        new_postings: board.count(Lane::New),
        seen: board.total_seen,
        shortlisted: board.count(Lane::Shortlist),
        drafted: tally.drafted,
        active: tally.active,
        interview: tally.interview,
        offer: tally.offer,
        hired: tally.hired,
        closed: tally.closed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::application::{FreelanceGates, Gate, GateReading, JobView, LaneView};
    use crate::insights::domain::{ApplicationFact, Furthest, Progress, SearchReport};

    fn job(key: &str) -> JobView {
        let pass = GateReading { gate: Gate::Pass, value: String::new() };
        JobView {
            key: key.into(),
            title: "Lead".into(),
            company: "Acme".into(),
            location: None,
            url: None,
            portal: None,
            found_on: None,
            deadline: None,
            days_to_deadline: None,
            score: None,
            verdict: None,
            scraper_fit: None,
            location_gate: None,
            language_gate: None,
            language_note: None,
            freelance: FreelanceGates { contract: pass.clone(), day_rate: pass.clone(), commute: pass },
            guard: None,
            strengths: vec![],
            gaps: vec![],
            fetch_attempts: None,
            fetch_error: None,
            signals: vec![],
            closing_soon: false,
            guard_flagged: false,
        }
    }

    fn fact(progress: Progress) -> ApplicationFact {
        ApplicationFact { progress, furthest: Furthest::Applied, sector: String::new(), channel: String::new(), is_rejection: false, is_final: false }
    }

    #[test]
    fn reads_the_board_the_report_and_the_profile() {
        let board = JobBoard {
            lanes: vec![
                LaneView { lane: Lane::New, jobs: vec![job("a"), job("b")] },
                LaneView { lane: Lane::Shortlist, jobs: vec![job("c")] },
                LaneView { lane: Lane::Skipped, jobs: vec![job("d")] },
            ],
            total_seen: 4,
        };
        let report = SearchReport::from_facts(&[
            fact(Progress::Drafted),
            fact(Progress::Active),
            fact(Progress::Active),
            fact(Progress::Offer),
            fact(Progress::Hired),
            fact(Progress::Closed),
        ]);

        let band = workflow_band(&report, &board, (7, 9));

        assert_eq!((band.profile_filled, band.profile_total), (7, 9));
        assert_eq!((band.new_postings, band.seen, band.shortlisted), (2, 4, 1));
        assert_eq!((band.drafted, band.active, band.interview, band.offer, band.hired, band.closed), (1, 2, 0, 1, 1, 1));
    }
}
