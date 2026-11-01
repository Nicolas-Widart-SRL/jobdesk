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
            Self::Followed => "Command opened",
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
    /// The label of the action a followed card opened, so the log can name the task.
    pub action: Option<String>,
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
        if self.outcome == Outcome::Followed {
            return self.action.as_ref().map_or_else(|| "Task opened".to_owned(), |action| format!("{action} opened"));
        }
        let verb = self.outcome.verb();
        self.reason.map_or_else(|| verb.to_owned(), |reason| format!("{verb}, {}", reason.label().to_lowercase()))
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
        Decision { key: "k".into(), name: "Acme".into(), outcome, reason: None, action: None, on: NaiveDate::from_ymd_opt(2026, 10, 31).unwrap() }
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
        assert_eq!(skipped.label(), "Skipped, rate");
    }

    #[test]
    fn a_followed_decision_names_the_task_it_opened() {
        let followed = Decision { action: Some("Draft application".into()), ..decided(Outcome::Followed) };
        assert_eq!(followed.label(), "Draft application opened");
        assert_eq!(decided(Outcome::Followed).label(), "Task opened");
    }
}
