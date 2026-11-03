use chrono::{Datelike, Days, NaiveDate};

use crate::shared_kernel::visible_text::visible_chars;

/// How far ahead the Today queue looks for what comes up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComingUp {
    Week,
    #[default]
    TwoWeeks,
    Month,
}

impl ComingUp {
    pub const ALL: [ComingUp; 3] = [ComingUp::Week, ComingUp::TwoWeeks, ComingUp::Month];

    pub fn days(self) -> i64 {
        match self {
            ComingUp::Week => 7,
            ComingUp::TwoWeeks => 14,
            ComingUp::Month => 30,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ComingUp::Week => "7 days",
            ComingUp::TwoWeeks => "14 days",
            ComingUp::Month => "30 days",
        }
    }

    /// The span as a screen says it: first as a count, then inside a sentence.
    pub fn words(self) -> (&'static str, &'static str) {
        match self {
            ComingUp::Week => ("1 week", "week"),
            ComingUp::TwoWeeks => ("2 weeks", "two weeks"),
            ComingUp::Month => ("30 days", "30 days"),
        }
    }
}

/// How long Later puts a card away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaterLasts {
    #[default]
    Tomorrow,
    ThreeDays,
    UntilMonday,
}

impl LaterLasts {
    pub const ALL: [LaterLasts; 3] = [LaterLasts::Tomorrow, LaterLasts::ThreeDays, LaterLasts::UntilMonday];

    pub fn label(self) -> &'static str {
        match self {
            LaterLasts::Tomorrow => "Until tomorrow",
            LaterLasts::ThreeDays => "3 days",
            LaterLasts::UntilMonday => "Until Monday",
        }
    }

    /// The day a card put away on `on` comes back. "Until Monday" is the next Monday after `on`, never `on` itself.
    pub fn until(self, on: NaiveDate) -> NaiveDate {
        let days = match self {
            LaterLasts::Tomorrow => 1,
            LaterLasts::ThreeDays => 3,
            LaterLasts::UntilMonday => 7 - u64::from(on.weekday().num_days_from_monday()),
        };
        on.checked_add_days(Days::new(days)).unwrap_or(on)
    }
}

/// Why a reason was not taken, in words the page can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasonRefusal {
    Empty,
    Taken,
    TooLong,
    Full,
}

impl ReasonRefusal {
    pub fn why(self) -> &'static str {
        match self {
            ReasonRefusal::Empty => "Type a reason first.",
            ReasonRefusal::Taken => "That reason is already in the list.",
            ReasonRefusal::TooLong => "A reason fits in 40 characters.",
            ReasonRefusal::Full => "The list holds 8 reasons. Remove one first.",
        }
    }
}

/// The reasons Skip offers, in the order the user keeps them.
#[derive(Debug, Clone, PartialEq)]
pub struct SkipReasons(Vec<String>);

impl SkipReasons {
    pub const MOST_REASONS: usize = 8;
    pub const REASON_CHARS: usize = 40;

    pub fn list(&self) -> &[String] {
        &self.0
    }

    /// No hidden characters, inner whitespace collapsed, ends trimmed.
    fn tidy(text: &str) -> String {
        let visible: String = visible_chars(text).collect();
        visible.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// A reason read back from a file, tidied as a typed one is and cut to `REASON_CHARS`. None when nothing visible is left.
    pub fn clean(text: &str) -> Option<String> {
        let cut: String = Self::tidy(text).chars().take(Self::REASON_CHARS).collect();
        let cut = cut.trim_end();
        (!cut.is_empty()).then(|| cut.to_owned())
    }

    /// The reason as it would be kept: no hidden characters, inner whitespace collapsed, ends trimmed.
    pub fn check(&self, text: &str) -> Result<String, ReasonRefusal> {
        let clean = Self::tidy(text);
        if clean.is_empty() {
            return Err(ReasonRefusal::Empty);
        }
        let lower = clean.to_lowercase();
        if self.0.iter().any(|kept| kept.to_lowercase() == lower) {
            return Err(ReasonRefusal::Taken);
        }
        if clean.chars().count() > Self::REASON_CHARS {
            return Err(ReasonRefusal::TooLong);
        }
        if self.0.len() >= Self::MOST_REASONS {
            return Err(ReasonRefusal::Full);
        }
        Ok(clean)
    }

    pub fn add(&mut self, text: &str) -> Result<(), ReasonRefusal> {
        let clean = self.check(text)?;
        self.0.push(clean);
        Ok(())
    }

    /// Whether a reason was taken out. The match ignores case.
    pub fn remove(&mut self, text: &str) -> bool {
        let lower = text.to_lowercase();
        let Some(at) = self.0.iter().position(|kept| kept.to_lowercase() == lower) else { return false };
        self.0.remove(at);
        true
    }

    /// Reads a saved list through the same rules as typing, so a hand-edited file cannot break them.
    pub fn from_saved<'a>(items: impl IntoIterator<Item = &'a str>) -> Self {
        let mut found = Self(Vec::new());
        for item in items {
            let _ = found.add(item);
        }
        found
    }
}

impl Default for SkipReasons {
    fn default() -> Self {
        Self(["Rate", "Scope", "Domain", "Location"].map(str::to_owned).to_vec())
    }
}

/// What the user set for the Today queue.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TodayQueue {
    pub coming_up: ComingUp,
    pub later: LaterLasts,
    pub skip_reasons: SkipReasons,
}

/// Which jobs the Jobs view hides. Everything off shows every job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobFilters {
    pub below_floor: bool,
    pub too_far: bool,
    pub language: bool,
    pub flagged: bool,
    pub under_score: bool,
    /// The score the "under" filter hides jobs below, from 0 to 100.
    pub score: u8,
}

impl JobFilters {
    pub const DEFAULT_SCORE: u8 = 60;

    pub fn any_on(&self) -> bool {
        self.below_floor || self.too_far || self.language || self.flagged || self.under_score
    }
}

impl Default for JobFilters {
    fn default() -> Self {
        Self { below_floor: false, too_far: false, language: false, flagged: false, under_score: false, score: Self::DEFAULT_SCORE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    #[test]
    fn until_monday_is_the_next_monday_after_the_day() {
        let until = |on| LaterLasts::UntilMonday.until(on);
        assert_eq!(until(day(11, 2)), day(11, 9), "a Monday waits a week");
        assert_eq!(until(day(11, 1)), day(11, 2), "Sunday");
        assert_eq!(until(day(10, 31)), day(11, 2), "Saturday");
        assert_eq!(until(day(10, 30)), day(11, 2), "Friday");
        assert_eq!(until(day(11, 3)), day(11, 9), "Tuesday");
    }

    #[test]
    fn the_other_choices_count_days() {
        assert_eq!(LaterLasts::Tomorrow.until(day(10, 31)), day(11, 1));
        assert_eq!(LaterLasts::ThreeDays.until(day(10, 29)), day(11, 1));
        assert_eq!(LaterLasts::ThreeDays.until(day(12, 30)), NaiveDate::from_ymd_opt(2027, 1, 2).unwrap());
    }

    #[test]
    fn the_spans_say_their_days_and_words() {
        let days: Vec<i64> = ComingUp::ALL.iter().map(|span| span.days()).collect();
        assert_eq!(days, [7, 14, 30]);
        assert_eq!(ComingUp::Week.label(), "7 days");
        assert_eq!(ComingUp::TwoWeeks.words(), ("2 weeks", "two weeks"));
        assert_eq!(ComingUp::Week.words(), ("1 week", "week"));
        assert_eq!(ComingUp::Month.words(), ("30 days", "30 days"));
        let labels: Vec<&str> = LaterLasts::ALL.iter().map(|lasts| lasts.label()).collect();
        assert_eq!(labels, ["Until tomorrow", "3 days", "Until Monday"]);
    }

    #[test]
    fn the_defaults_are_what_the_queue_did_before() {
        let queue = TodayQueue::default();
        assert_eq!((queue.coming_up, queue.later), (ComingUp::TwoWeeks, LaterLasts::Tomorrow));
        assert_eq!(queue.skip_reasons.list(), ["Rate", "Scope", "Domain", "Location"]);
        let filters = JobFilters::default();
        assert!(!filters.any_on());
        assert_eq!(filters.score, 60);
    }

    #[test]
    fn any_filter_on_shows_in_any_on() {
        assert!(JobFilters { below_floor: true, ..JobFilters::default() }.any_on());
        assert!(JobFilters { too_far: true, ..JobFilters::default() }.any_on());
        assert!(JobFilters { language: true, ..JobFilters::default() }.any_on());
        assert!(JobFilters { flagged: true, ..JobFilters::default() }.any_on());
        assert!(JobFilters { under_score: true, ..JobFilters::default() }.any_on());
        assert!(!JobFilters { score: 90, ..JobFilters::default() }.any_on(), "the score alone hides nothing");
    }

    #[test]
    fn a_reason_is_added_at_the_end_and_removed_by_name() {
        let mut reasons = SkipReasons::default();
        assert_eq!(reasons.add("Culture"), Ok(()));
        assert_eq!(reasons.list().last().map(String::as_str), Some("Culture"));
        assert!(reasons.remove("Scope"));
        assert!(!reasons.remove("Scope"));
        assert_eq!(reasons.list(), ["Rate", "Domain", "Location", "Culture"]);
        assert!(reasons.remove("DOMAIN"), "case does not matter");
        assert_eq!(reasons.list(), ["Rate", "Location", "Culture"]);
    }

    #[test]
    fn a_reason_is_trimmed_and_its_spaces_collapsed() {
        let mut reasons = SkipReasons::default();
        assert_eq!(reasons.check("  Pay \t  too   low \n"), Ok("Pay too low".to_owned()));
        reasons.add("  Pay \t  too   low ").unwrap();
        assert_eq!(reasons.list().last().map(String::as_str), Some("Pay too low"));
    }

    #[test]
    fn a_reason_loses_its_hidden_characters() {
        let mut reasons = SkipReasons::default();
        reasons.add("Pa\u{200b}y\u{202e} low\u{1b}[31m").unwrap();
        assert_eq!(reasons.list().last().map(String::as_str), Some("Pay low"));
        assert_eq!(reasons.add("\u{200b}\u{202e}"), Err(ReasonRefusal::Empty), "only hidden characters is empty");
    }

    #[test]
    fn a_read_back_reason_is_tidied_and_cut() {
        assert_eq!(SkipReasons::clean(" Pa\u{200b}y\u{202e}   low\u{1b}[31m "), Some("Pay low".to_owned()));
        assert_eq!(SkipReasons::clean(&"a".repeat(50)), Some("a".repeat(40)));
        assert_eq!(SkipReasons::clean(&format!("{} b", "a".repeat(39))), Some("a".repeat(39)), "a cut never ends in a space");
        assert_eq!(SkipReasons::clean("\u{200b} \u{202e}"), None);
    }

    #[test]
    fn a_duplicate_ignores_case_and_spacing() {
        let mut reasons = SkipReasons::default();
        assert_eq!(reasons.add("rate"), Err(ReasonRefusal::Taken));
        assert_eq!(reasons.add("  ROCKET  "), Ok(()));
        assert_eq!(reasons.add("rocket"), Err(ReasonRefusal::Taken));
        assert_eq!(reasons.list().len(), 5);
    }

    #[test]
    fn an_empty_reason_is_refused() {
        let mut reasons = SkipReasons::default();
        assert_eq!(reasons.add(""), Err(ReasonRefusal::Empty));
        assert_eq!(reasons.add("  \t "), Err(ReasonRefusal::Empty));
        assert_eq!(reasons.list().len(), 4);
    }

    #[test]
    fn a_reason_fits_in_forty_characters() {
        let mut reasons = SkipReasons::default();
        assert_eq!(reasons.add(&"a".repeat(41)), Err(ReasonRefusal::TooLong));
        assert_eq!(reasons.add(&"a".repeat(40)), Ok(()));
        assert_eq!(reasons.add(&"é".repeat(40)), Ok(()), "characters count, not bytes");
        assert_eq!(reasons.list().len(), 6);
    }

    #[test]
    fn the_ninth_reason_is_refused() {
        let mut reasons = SkipReasons::default();
        for n in 0..4 {
            assert_eq!(reasons.add(&format!("Extra {n}")), Ok(()));
        }
        assert_eq!(reasons.list().len(), SkipReasons::MOST_REASONS);
        assert_eq!(reasons.add("One more"), Err(ReasonRefusal::Full));
        assert!(reasons.remove("Extra 0"));
        assert_eq!(reasons.add("One more"), Ok(()));
    }

    #[test]
    fn a_refusal_says_why() {
        assert_eq!(ReasonRefusal::Empty.why(), "Type a reason first.");
        assert_eq!(ReasonRefusal::Taken.why(), "That reason is already in the list.");
        assert_eq!(ReasonRefusal::TooLong.why(), "A reason fits in 40 characters.");
        assert_eq!(ReasonRefusal::Full.why(), "The list holds 8 reasons. Remove one first.");
    }

    #[test]
    fn a_saved_list_goes_through_the_same_rules() {
        let long = "x".repeat(41);
        let saved = ["  Rate ", "rate", "", &long, "Scope", "A", "B", "C", "D", "E", "F"];
        let reasons = SkipReasons::from_saved(saved);
        assert_eq!(reasons.list(), ["Rate", "Scope", "A", "B", "C", "D", "E", "F"]);
        assert!(SkipReasons::from_saved([]).list().is_empty());
    }
}
