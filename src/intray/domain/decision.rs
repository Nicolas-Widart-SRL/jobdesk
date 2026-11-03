use chrono::NaiveDate;

use crate::shared_kernel::LoadError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Followed,
    Later,
    Skipped,
    Kept,
}

impl Outcome {
    pub fn verb(self) -> &'static str {
        match self {
            Self::Followed => "Task opened",
            Self::Later => "Set aside",
            Self::Skipped => "Skipped",
            Self::Kept => "Kept",
        }
    }
}

/// What the user did with a card. Cards are derived, so this is the only tray state stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub key: String,
    pub name: String,
    pub outcome: Outcome,
    /// Why a card was skipped, as the user picked it. A reason later taken off the list stays.
    pub reason: Option<String>,
    /// The key of the task a followed card opened. See `task_name`.
    pub task: Option<String>,
    pub on: NaiveDate,
    /// The day a card set aside with Later comes back. None means the day after `on`.
    pub until: Option<NaiveDate>,
}

impl Decision {
    /// A followed card leaves only for the day: if its command never ran, the same card returns.
    pub fn clears_card(&self, today: NaiveDate) -> bool {
        match self.outcome {
            Outcome::Skipped | Outcome::Kept => true,
            Outcome::Followed => self.on == today,
            Outcome::Later => false,
        }
    }

    /// The day a card set aside comes back.
    pub fn returns_on(&self) -> NaiveDate {
        self.until.unwrap_or_else(|| self.on.succ_opt().unwrap_or(self.on))
    }

    /// The day a card set aside comes back, given the card's date as it stands now. A date after the day it was set aside
    /// brings it back on that date when that comes first, so a deadline added or moved while the card is away never
    /// passes. A date on or before that day brings it back the next day at the latest, as Later does for a card due or
    /// late when it is pressed.
    pub fn returns_by(&self, due: Option<NaiveDate>) -> NaiveDate {
        let until = self.returns_on();
        match due {
            Some(due) if due > self.on => until.min(due),
            Some(_) => until.min(self.on.succ_opt().unwrap_or(until)),
            None => until,
        }
    }

    /// Later is not a decision on the card, so it clears nothing and is never logged. It holds from its day until the card comes back.
    pub fn sets_aside(&self, today: NaiveDate) -> bool {
        self.keeps_aside(today, None)
    }

    /// Whether the card is still away today, given its date as it stands now. See `returns_by`.
    pub fn keeps_aside(&self, today: NaiveDate, due: Option<NaiveDate>) -> bool {
        self.outcome == Outcome::Later && self.on <= today && today < self.returns_by(due)
    }

    pub fn is_logged_on(&self, today: NaiveDate) -> bool {
        self.on == today && self.outcome != Outcome::Later
    }

    pub fn label(&self) -> String {
        let verb = self.outcome.verb();
        if self.outcome == Outcome::Followed {
            return self.task.as_deref().and_then(task_name).map_or_else(|| verb.to_owned(), |name| format!("{verb}: {name}"));
        }
        self.reason.as_ref().map_or_else(|| verb.to_owned(), |reason| format!("{verb} for {}", in_sentence(reason)))
    }

    /// What the undo row shows, and what a screen reader hears of it. `asks_reason` is whether there are reasons to offer.
    pub fn note(&self, asks_reason: bool) -> String {
        match (self.outcome, &self.reason) {
            (Outcome::Later, _) => format!("Set aside until {}", until_words(self.on, self.returns_on())),
            (Outcome::Skipped, None) if asks_reason => "Skipped. Why? Optional.".to_owned(),
            (Outcome::Skipped, None) => "Skipped.".to_owned(),
            _ => self.label(),
        }
    }
}

/// A reason inside a sentence: its first letter is lowered unless its first word is an acronym. "Domain" reads "domain",
/// "Not on LinkedIn" reads "not on LinkedIn", and "IR35" stays as written.
fn in_sentence(reason: &str) -> String {
    let mut chars = reason.chars();
    let Some(first) = chars.next() else { return String::new() };
    let rest = chars.as_str();
    // The rest of the first word: empty when the reason is a one-letter word followed by others.
    let first_word = rest.split(char::is_whitespace).next().unwrap_or_default();
    if first_word.chars().any(char::is_uppercase) { reason.to_owned() } else { first.to_lowercase().chain(rest.chars()).collect() }
}

/// When a card comes back, said from `on`: "tomorrow", "Monday", "next Monday", or a date further out.
pub fn until_words(on: NaiveDate, until: NaiveDate) -> String {
    match (until - on).num_days() {
        1 => "tomorrow".to_owned(),
        2..=6 => until.format("%A").to_string(),
        7 => until.format("next %A").to_string(),
        _ => until.format("%-d %B").to_string(),
    }
}

/// The plain name of a task a card can open, by the key its action carries. An unknown key has none.
pub fn task_name(key: &str) -> Option<&'static str> {
    match key {
        "draft" => Some("draft application"),
        "sent" => Some("record as sent"),
        "follow_up" => Some("follow-up"),
        "close" => Some("close as no response"),
        _ => None,
    }
}

pub trait DecisionStore {
    fn load(&self) -> Result<Vec<Decision>, LoadError>;
    fn save(&self, decisions: &[Decision]) -> Result<(), LoadError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    fn decided(outcome: Outcome) -> Decision {
        Decision { key: "k".into(), name: "Acme".into(), outcome, reason: None, task: None, on: day(10, 31), until: None }
    }

    #[test]
    fn a_followed_card_returns_the_next_day_and_a_skipped_one_never() {
        let today = day(10, 31);
        let tomorrow = today.succ_opt().unwrap();
        assert!(decided(Outcome::Followed).clears_card(today));
        assert!(!decided(Outcome::Followed).clears_card(tomorrow));
        for outcome in [Outcome::Skipped, Outcome::Kept] {
            assert!(decided(outcome).clears_card(tomorrow));
        }
    }

    #[test]
    fn a_date_that_comes_while_the_card_is_away_brings_it_back_that_day() {
        let later = Decision { until: Some(day(11, 2)), ..decided(Outcome::Later) };
        assert_eq!(later.returns_by(Some(day(11, 1))), day(11, 1));
        assert!(later.keeps_aside(day(10, 31), Some(day(11, 1))));
        assert!(!later.keeps_aside(day(11, 1), Some(day(11, 1))));
        assert_eq!(later.returns_by(Some(day(11, 5))), day(11, 2), "a later date leaves the stay alone");
        assert_eq!(later.returns_by(None), day(11, 2));
    }

    #[test]
    fn a_date_on_or_before_the_day_it_was_set_aside_brings_it_back_the_next_day() {
        let later = Decision { until: Some(day(11, 2)), ..decided(Outcome::Later) };
        assert_eq!(later.returns_by(Some(day(10, 31))), day(11, 1), "due the day it was set aside");
        assert_eq!(later.returns_by(Some(day(10, 29))), day(11, 1), "already late");
        let due_when_pressed = Decision { until: Some(day(11, 1)), ..decided(Outcome::Later) };
        assert_eq!(due_when_pressed.returns_by(Some(day(10, 31))), day(11, 1), "a card due when L was pressed still goes away for the day");
        assert!(due_when_pressed.keeps_aside(day(10, 31), Some(day(10, 31))));
    }

    #[test]
    fn later_sets_aside_for_the_day_and_clears_nothing() {
        let today = day(10, 31);
        let later = decided(Outcome::Later);
        assert!(later.sets_aside(today));
        assert!(!later.sets_aside(today.succ_opt().unwrap()));
        assert!(!later.sets_aside(today.pred_opt().unwrap()));
        assert!(!later.clears_card(today));
        assert!(!later.is_logged_on(today));
        assert!(decided(Outcome::Skipped).is_logged_on(today));
    }

    #[test]
    fn later_holds_until_its_day() {
        let later = Decision { until: Some(day(11, 3)), ..decided(Outcome::Later) };
        assert!(later.sets_aside(day(10, 31)));
        assert!(later.sets_aside(day(11, 2)));
        assert!(!later.sets_aside(day(11, 3)));
        assert!(!later.sets_aside(day(10, 30)));
    }

    #[test]
    fn label_names_the_reason() {
        let skipped = Decision { reason: Some("Rate".into()), ..decided(Outcome::Skipped) };
        assert_eq!(skipped.label(), "Skipped for rate");
        let custom = Decision { reason: Some("Agency, not the client".into()), ..decided(Outcome::Skipped) };
        assert_eq!(custom.label(), "Skipped for agency, not the client");
        let old = Decision { reason: Some("rate".into()), ..decided(Outcome::Skipped) };
        assert_eq!(old.label(), "Skipped for rate");
    }

    #[test]
    fn a_reason_keeps_the_capitals_it_needs() {
        let label = |reason: &str| Decision { reason: Some(reason.into()), ..decided(Outcome::Skipped) }.label();
        assert_eq!(label("IR35"), "Skipped for IR35");
        assert_eq!(label("Not on LinkedIn"), "Skipped for not on LinkedIn");
        assert_eq!(label("B2B only"), "Skipped for B2B only");
        assert_eq!(label("A Big agency"), "Skipped for a Big agency");
        assert_eq!(label("Domain"), "Skipped for domain");
        assert_eq!(label("Écart de salaire"), "Skipped for écart de salaire");
    }

    #[test]
    fn the_note_tells_what_is_still_open_to_the_user() {
        assert_eq!(decided(Outcome::Later).note(true), "Set aside until tomorrow");
        assert_eq!(decided(Outcome::Skipped).note(true), "Skipped. Why? Optional.");
        assert_eq!(Decision { reason: Some("Scope".into()), ..decided(Outcome::Skipped) }.note(true), "Skipped for scope");
        assert_eq!(decided(Outcome::Kept).note(false), "Kept");
    }

    #[test]
    fn a_skip_with_no_reasons_to_offer_asks_nothing() {
        assert_eq!(decided(Outcome::Skipped).note(false), "Skipped.");
    }

    #[test]
    fn the_note_names_the_day_a_card_comes_back() {
        let until = |until| Decision { until: Some(until), ..decided(Outcome::Later) }.note(false);
        assert_eq!(until(day(11, 1)), "Set aside until tomorrow");
        assert_eq!(until(day(11, 2)), "Set aside until Monday");
        assert_eq!(until(day(11, 3)), "Set aside until Tuesday");
        assert_eq!(until(day(11, 7)), "Set aside until next Saturday");
        assert_eq!(until(day(11, 6)), "Set aside until Friday");
        assert_eq!(until(day(11, 8)), "Set aside until 8 November");
    }

    #[test]
    fn until_words_count_from_the_day_given() {
        assert_eq!(until_words(day(11, 2), day(11, 9)), "next Monday");
        assert_eq!(until_words(day(11, 1), day(11, 2)), "tomorrow");
        assert_eq!(until_words(day(10, 31), day(11, 2)), "Monday");
        assert_eq!(until_words(day(10, 31), day(11, 6)), "Friday");
    }

    #[test]
    fn a_followed_decision_names_the_task_it_opened() {
        let opened = |task: &str| Decision { task: Some(task.into()), ..decided(Outcome::Followed) }.label();
        assert_eq!(opened("draft"), "Task opened: draft application");
        assert_eq!(opened("sent"), "Task opened: record as sent");
        assert_eq!(opened("follow_up"), "Task opened: follow-up");
        assert_eq!(opened("close"), "Task opened: close as no response");
        assert_eq!(opened("renamed_later"), "Task opened");
        assert_eq!(decided(Outcome::Followed).label(), "Task opened");
    }
}
