use std::collections::HashSet;

use crate::discovery::application::{Gate, JobBoard, JobView, Lane};
use crate::settings::application::JobFilters;

/// Whether the Jobs view filters hide `job`. `kept` says the user kept its flagged ad in Today, so the safety filter lets it through.
/// A job with no score is never hidden by the score filter. Filters only hide rows: the job stays in its list, which `/rank` owns.
pub fn hides(job: &JobView, filters: &JobFilters, kept: bool) -> bool {
    (filters.below_floor && job.freelance.day_rate.gate == Gate::Fail)
        || (filters.too_far && job.freelance.commute.gate == Gate::Fail)
        || (filters.language && job.language_gate == Some(Gate::Fail))
        || (filters.under_score && job.score.is_some_and(|score| score < filters.score))
        || (filters.flagged && job.guard_flagged && !kept)
}

/// The jobs of one list the filters let through, in the list's order, and how many they hide.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown<'a> {
    pub jobs: Vec<&'a JobView>,
    pub hidden: usize,
}

/// `kept` holds the key of every job whose flagged ad the user kept in Today.
pub fn shown_in<'a>(board: &'a JobBoard, lane: Lane, filters: &JobFilters, kept: &HashSet<String>) -> Shown<'a> {
    let (hidden, jobs): (Vec<&JobView>, Vec<&JobView>) = board.jobs_in(lane).iter().partition(|job| hides(job, filters, kept.contains(&job.key)));
    Shown { jobs, hidden: hidden.len() }
}

/// How many jobs the filters hide across every list, and how many jobs there are in all.
pub fn hidden_total(board: &JobBoard, filters: &JobFilters, kept: &HashSet<String>) -> (usize, usize) {
    let jobs = board.lanes.iter().flat_map(|view| &view.jobs);
    let hidden = jobs.clone().filter(|job| hides(job, filters, kept.contains(&job.key))).count();
    (hidden, jobs.count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::application::fixtures::{board, view};
    use crate::discovery::application::{FreelanceGates, GateReading};

    fn failed() -> GateReading {
        GateReading { gate: Gate::Fail, value: "x".into() }
    }

    fn all_on() -> JobFilters {
        JobFilters { below_floor: true, too_far: true, language: true, flagged: true, under_score: true, score: 60 }
    }

    #[test]
    fn no_filter_hides_anything() {
        let job = JobView { score: Some(10), language_gate: Some(Gate::Fail), guard_flagged: true, ..view("a") };
        assert!(!hides(&job, &JobFilters::default(), false));
        assert!(hides(&job, &all_on(), false));
    }

    #[test]
    fn below_floor_hides_a_failed_day_rate_only() {
        let filters = JobFilters { below_floor: true, ..JobFilters::default() };
        let low = JobView { freelance: FreelanceGates { day_rate: failed(), ..view("a").freelance }, ..view("a") };
        assert!(hides(&low, &filters, false));
        assert!(!hides(&view("b"), &filters, false));
        assert!(!hides(&low, &JobFilters { below_floor: false, ..all_on() }, false), "the other filters leave a low rate alone");
    }

    #[test]
    fn too_far_hides_a_failed_commute_only() {
        let filters = JobFilters { too_far: true, ..JobFilters::default() };
        let far = JobView { freelance: FreelanceGates { commute: failed(), ..view("a").freelance }, ..view("a") };
        assert!(hides(&far, &filters, false));
        assert!(!hides(&view("b"), &filters, false));
        assert!(!hides(&far, &JobFilters { too_far: false, ..all_on() }, false));
    }

    #[test]
    fn language_hides_a_failed_language_only() {
        let filters = JobFilters { language: true, ..JobFilters::default() };
        assert!(hides(&JobView { language_gate: Some(Gate::Fail), ..view("a") }, &filters, false));
        assert!(!hides(&JobView { language_gate: Some(Gate::Flag), ..view("a") }, &filters, false));
        assert!(!hides(&JobView { language_gate: None, ..view("a") }, &filters, false));
    }

    #[test]
    fn under_score_hides_scores_below_the_line() {
        let filters = JobFilters { under_score: true, score: 70, ..JobFilters::default() };
        assert!(hides(&JobView { score: Some(69), ..view("a") }, &filters, false));
        assert!(!hides(&JobView { score: Some(70), ..view("a") }, &filters, false));
        assert!(!hides(&JobView { score: Some(10), ..view("a") }, &JobFilters { under_score: false, ..filters }, false));
    }

    #[test]
    fn an_unscored_job_survives_the_score_filter() {
        let filters = JobFilters { under_score: true, score: 100, ..JobFilters::default() };
        assert!(!hides(&JobView { score: None, ..view("a") }, &filters, false));
    }

    #[test]
    fn a_kept_flagged_ad_survives_the_safety_filter() {
        let filters = JobFilters { flagged: true, ..JobFilters::default() };
        let flagged = JobView { guard_flagged: true, ..view("a") };
        assert!(hides(&flagged, &filters, false));
        assert!(!hides(&flagged, &filters, true));
        assert!(!hides(&view("b"), &filters, false));
    }

    #[test]
    fn hidden_and_shown_add_up_and_no_job_changes_list() {
        let low = JobView { score: Some(40), ..view("low") };
        let flagged = JobView { guard_flagged: true, ..view("flagged") };
        let kept_ad = JobView { guard_flagged: true, ..view("kept") };
        let board = board(vec![(Lane::Shortlist, vec![view("top"), low, flagged, kept_ad]), (Lane::New, vec![JobView { score: None, ..view("new") }])]);
        let before = board.clone();
        let kept = HashSet::from(["kept".to_owned()]);
        let filters = JobFilters { under_score: true, flagged: true, ..JobFilters::default() };

        for lane in Lane::ALL {
            let shown = shown_in(&board, lane, &filters, &kept);
            assert_eq!(shown.hidden + shown.jobs.len(), board.count(lane));
        }
        let shortlist = shown_in(&board, Lane::Shortlist, &filters, &kept);
        let keys: Vec<_> = shortlist.jobs.iter().map(|job| job.key.as_str()).collect();
        assert_eq!(keys, ["top", "kept"]);
        assert_eq!(shortlist.hidden, 2);
        assert_eq!(hidden_total(&board, &filters, &kept), (2, 5));
        assert_eq!(board, before, "filters never move a job to another list");
    }
}
