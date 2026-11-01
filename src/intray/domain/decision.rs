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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Rate,
    Scope,
    Domain,
    Location,
}

impl SkipReason {
    pub const ALL: [Self; 4] = [Self::Rate, Self::Scope, Self::Domain, Self::Location];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rate => "Rate",
            Self::Scope => "Scope",
            Self::Domain => "Domain",
            Self::Location => "Location",
        }
    }
}

/// What the user did with a card. Cards are derived, so this is the only tray state stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub key: String,
    pub name: String,
    pub outcome: Outcome,
    pub reason: Option<SkipReason>,
    /// The key of the task a followed card opened. See `task_name`.
    pub task: Option<String>,
    pub on: NaiveDate,
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

    /// Later is not a decision on the card, so it clears nothing and is never logged.
    pub fn sets_aside(&self, today: NaiveDate) -> bool {
        self.outcome == Outcome::Later && self.on == today
    }

    pub fn is_logged_on(&self, today: NaiveDate) -> bool {
        self.on == today && self.outcome != Outcome::Later
    }

    pub fn label(&self) -> String {
        let verb = self.outcome.verb();
        if self.outcome == Outcome::Followed {
            return self.task.as_deref().and_then(task_name).map_or_else(|| verb.to_owned(), |name| format!("{verb}: {name}"));
        }
        self.reason.map_or_else(|| verb.to_owned(), |reason| format!("{verb} for {}", reason.label().to_lowercase()))
    }

    /// What the undo row shows, and what a screen reader hears of it.
    pub fn note(&self) -> String {
        match (self.outcome, self.reason) {
            (Outcome::Later, _) => "Set aside until tomorrow".to_owned(),
            (Outcome::Skipped, None) => "Skipped. Why? Optional.".to_owned(),
            _ => self.label(),
        }
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

    fn decided(outcome: Outcome) -> Decision {
        Decision { key: "k".into(), name: "Acme".into(), outcome, reason: None, task: None, on: NaiveDate::from_ymd_opt(2026, 10, 31).unwrap() }
    }

    #[test]
    fn a_followed_card_returns_the_next_day_and_a_skipped_one_never() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        let tomorrow = today.succ_opt().unwrap();
        assert!(decided(Outcome::Followed).clears_card(today));
        assert!(!decided(Outcome::Followed).clears_card(tomorrow));
        for outcome in [Outcome::Skipped, Outcome::Kept] {
            assert!(decided(outcome).clears_card(tomorrow));
        }
    }

    #[test]
    fn later_sets_aside_for_the_day_and_clears_nothing() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        let later = decided(Outcome::Later);
        assert!(later.sets_aside(today));
        assert!(!later.sets_aside(today.succ_opt().unwrap()));
        assert!(!later.clears_card(today));
        assert!(!later.is_logged_on(today));
        assert!(decided(Outcome::Skipped).is_logged_on(today));
    }

    #[test]
    fn label_names_the_reason() {
        let skipped = Decision { reason: Some(SkipReason::Rate), ..decided(Outcome::Skipped) };
        assert_eq!(skipped.label(), "Skipped for rate");
    }

    #[test]
    fn the_note_tells_what_is_still_open_to_the_user() {
        assert_eq!(decided(Outcome::Later).note(), "Set aside until tomorrow");
        assert_eq!(decided(Outcome::Skipped).note(), "Skipped. Why? Optional.");
        assert_eq!(Decision { reason: Some(SkipReason::Scope), ..decided(Outcome::Skipped) }.note(), "Skipped for scope");
        assert_eq!(decided(Outcome::Kept).note(), "Kept");
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
