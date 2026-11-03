use crate::discovery::application::{TermPart, YourTerms};
use crate::settings::domain::{JobFilters, TermsDrawn};
use crate::shared_kernel::visible_text::visible_chars;

/// The line Update with Claude… opens the launcher on: `/setup`'s goals, which hold the floor and the commute.
pub const TERMS_LINE: &str = "/setup --section goals";

/// Your terms as the open folder gives them.
#[derive(Debug, Clone, Copy)]
pub enum TermsState<'a> {
    NoFolder,
    /// The folder is open and not read yet.
    Reading,
    Read(&'a YourTerms),
}

/// What the rail says beside Your terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermsWord {
    /// Nothing to say yet: no folder, or not read.
    None,
    FromProfile,
    /// How many parts the profile does not state.
    NotStated(usize),
    /// A part could not be read.
    Hidden,
}

impl TermsWord {
    pub fn text(self) -> String {
        match self {
            TermsWord::None => String::new(),
            TermsWord::FromProfile => "From profile".to_owned(),
            TermsWord::NotStated(n) => format!("{n} not stated"),
            TermsWord::Hidden => "Hidden".to_owned(),
        }
    }
}

/// A part that failed hides the page, so it outranks one not stated.
pub fn terms_word(state: TermsState) -> TermsWord {
    let TermsState::Read(terms) = state else { return TermsWord::None };
    match terms.not_stated() {
        _ if !terms.failures().is_empty() => TermsWord::Hidden,
        0 => TermsWord::FromProfile,
        n => TermsWord::NotStated(n),
    }
}

/// Which parts of Your terms the page draws, so a search matches only those: a part whose file failed is hidden.
pub fn terms_drawn(state: TermsState) -> TermsDrawn {
    let TermsState::Read(terms) = state else { return TermsDrawn::Nothing };
    match (&terms.floor, &terms.commute) {
        (TermPart::Read(_), TermPart::Read(_)) => TermsDrawn::Whole,
        (TermPart::Read(_), TermPart::Failed(_)) => TermsDrawn::RateOnly,
        (TermPart::Failed(_), TermPart::Read(_)) => TermsDrawn::CommuteOnly,
        (TermPart::Failed(_), TermPart::Failed(_)) => TermsDrawn::Nothing,
    }
}

/// Text read from the profile or from an error, as plain text: no control, escape, bidi or zero-width characters.
pub fn plain(text: &str) -> String {
    visible_chars(text).collect::<String>().trim().to_owned()
}

/// A tier's name as a row's label: the file keeps it in lower case, so its first letter is raised.
pub fn tier_title(name: &str) -> String {
    let name = plain(name);
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => name,
    }
}

/// The floor as the page states it: `EUR 700 a day`.
pub fn floor_words(floor: u32) -> String {
    format!("EUR {floor} a day")
}

/// Which jobs the commute check reads Unclear for without tiers: a fully remote job still passes it, and a place the
/// location check ruled out still fails it.
const UNCLEAR_COMMUTE: &str = "for a job that is not fully remote, unless the location check ruled it out.";

/// What the alert says still works while a part of your terms cannot be read, naming the check that reads Unclear.
pub fn terms_failed_line(terms: &YourTerms) -> String {
    let floor = matches!(terms.floor, TermPart::Failed(_));
    let commute = matches!(terms.commute, TermPart::Failed(_));
    let unclear = match (floor, commute) {
        (true, true) => format!("Until the files are fixed, its day-rate check reads Unclear, and so does its commute check {UNCLEAR_COMMUTE}"),
        (false, true) => format!("Until the file is fixed, its commute check reads Unclear {UNCLEAR_COMMUTE}"),
        _ => "Its day-rate check reads Unclear until the file is fixed.".to_owned(),
    };
    format!("Jobs still reads every job you saved. {unclear}")
}

/// The day-rate filter's help, from your terms: the floor it holds jobs to, or why no job fails it.
pub fn floor_filter_help(state: TermsState) -> String {
    match state {
        TermsState::Read(YourTerms { floor: TermPart::Read(Some(floor)), .. }) => format!("{}, from Your terms.", floor_words(*floor)),
        TermsState::Read(YourTerms { floor: TermPart::Failed(_), .. }) => "Your terms could not be read, so no job fails this check.".to_owned(),
        TermsState::Read(_) => "Your terms state no floor, so no job fails this check.".to_owned(),
        TermsState::NoFolder | TermsState::Reading => "The floor in Your terms, once your profile is read.".to_owned(),
    }
}

/// The note at the top of Jobs view: how many jobs the filters hide of all of them. `count` is `(hidden, total)`, None
/// until the saved jobs are read.
pub fn jobs_count_note(filters: &JobFilters, count: Option<(usize, usize)>) -> String {
    match count {
        None => "Jobs counts what these filters hide once your saved jobs are read.".to_owned(),
        Some((_, 0)) => "No jobs saved yet, so these filters have nothing to hide.".to_owned(),
        Some((_, total)) if !filters.any_on() => match total {
            1 => "No filter is on, so Jobs shows its 1 job.".to_owned(),
            total => format!("No filter is on, so Jobs shows all {total} jobs."),
        },
        Some((hidden, total)) => {
            let total = if total == 1 { "1 job".to_owned() } else { format!("{total} jobs") };
            format!("Jobs hides {hidden} of {total} with these filters. Hidden jobs stay in your folder, and Jobs shows how many it hides.")
        }
    }
}

/// The rail's word beside Jobs view: how many jobs it hides, while any filter is on and the jobs are read.
pub fn hidden_word(filters: &JobFilters, count: Option<(usize, usize)>) -> String {
    match count {
        Some((hidden, _)) if filters.any_on() => format!("{hidden} hidden"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::application::{Commute, CommuteTier};
    use crate::shared_kernel::LoadError;

    fn tier(name: &str) -> CommuteTier {
        CommuteTier { name: name.into(), places: vec!["Namur".into()], note: String::new() }
    }

    fn read(floor: Option<u32>, tiers: Vec<CommuteTier>) -> YourTerms {
        YourTerms { floor: TermPart::Read(floor), commute: TermPart::Read(Commute { origin: Some("from Namur".into()), tiers }) }
    }

    fn failed() -> LoadError {
        LoadError::new("01-candidate-profile.md", "not UTF-8 text")
    }

    #[test]
    fn the_rail_word_follows_the_terms() {
        assert_eq!(terms_word(TermsState::NoFolder), TermsWord::None);
        assert_eq!(terms_word(TermsState::Reading), TermsWord::None);
        let stated = read(Some(700), vec![tier("ideal")]);
        assert_eq!(terms_word(TermsState::Read(&stated)), TermsWord::FromProfile);
        let no_floor = read(None, vec![tier("ideal")]);
        assert_eq!(terms_word(TermsState::Read(&no_floor)), TermsWord::NotStated(1));
        assert_eq!(TermsWord::NotStated(1).text(), "1 not stated");
        let broken = YourTerms { floor: TermPart::Failed(failed()), commute: TermPart::Read(Commute::default()) };
        assert_eq!(terms_word(TermsState::Read(&broken)), TermsWord::Hidden, "a failure outranks the commute not stated");
        assert_eq!(TermsWord::Hidden.text(), "Hidden");
    }

    #[test]
    fn a_tier_reads_as_a_label_without_hidden_characters() {
        assert_eq!(tier_title("too far"), "Too far");
        assert_eq!(tier_title("ideal\u{202e}"), "Ideal");
        assert_eq!(tier_title(""), "");
        assert_eq!(plain(" Na\u{200b}mur\u{1b}[31m "), "Namur");
    }

    #[test]
    fn the_failed_line_names_the_check_that_reads_unclear() {
        let floor = YourTerms { floor: TermPart::Failed(failed()), commute: TermPart::Read(Commute::default()) };
        assert_eq!(terms_failed_line(&floor), "Jobs still reads every job you saved. Its day-rate check reads Unclear until the file is fixed.");
        let commute = YourTerms { floor: TermPart::Read(Some(700)), commute: TermPart::Failed(failed()) };
        assert_eq!(
            terms_failed_line(&commute),
            "Jobs still reads every job you saved. Until the file is fixed, its commute check reads Unclear for a job that is not fully remote, unless the location check ruled it out."
        );
        let both = YourTerms { floor: TermPart::Failed(failed()), commute: TermPart::Failed(failed()) };
        assert_eq!(
            terms_failed_line(&both),
            "Jobs still reads every job you saved. Until the files are fixed, its day-rate check reads Unclear, and so does its commute check for a job that is not fully remote, unless the location check ruled it out."
        );
        assert!(!terms_failed_line(&both).contains("lists every job"), "a filter on Jobs view can hide a job from the list");
    }

    #[test]
    fn the_floor_filter_says_what_it_holds_jobs_to() {
        let stated = read(Some(700), vec![]);
        assert_eq!(floor_filter_help(TermsState::Read(&stated)), "EUR 700 a day, from Your terms.");
        let none = read(None, vec![]);
        assert_eq!(floor_filter_help(TermsState::Read(&none)), "Your terms state no floor, so no job fails this check.");
    }

    #[test]
    fn the_count_note_says_what_the_filters_hide() {
        let on = JobFilters { below_floor: true, ..JobFilters::default() };
        assert_eq!(
            jobs_count_note(&on, Some((3, 14))),
            "Jobs hides 3 of 14 jobs with these filters. Hidden jobs stay in your folder, and Jobs shows how many it hides."
        );
        assert_eq!(jobs_count_note(&JobFilters::default(), Some((0, 14))), "No filter is on, so Jobs shows all 14 jobs.");
        assert_eq!(jobs_count_note(&on, None), "Jobs counts what these filters hide once your saved jobs are read.");
        for filters in [on, JobFilters::default()] {
            assert_eq!(jobs_count_note(&filters, Some((0, 0))), "No jobs saved yet, so these filters have nothing to hide.", "never all 0 jobs");
        }
        assert_eq!(hidden_word(&on, Some((3, 14))), "3 hidden");
        assert_eq!(hidden_word(&on, None), "");
        assert_eq!(hidden_word(&JobFilters::default(), Some((0, 14))), "", "no filter, no word");
    }
}
