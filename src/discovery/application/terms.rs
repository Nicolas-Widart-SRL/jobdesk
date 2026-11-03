use crate::discovery::domain::{CommuteTier, FreelanceTerms};
use crate::shared_kernel::LoadError;

/// The one contract term the gates know. `/rank` stores no contract field, so it is not read from a file.
pub const CONTRACT_TERM: &str = "Freelance only";

/// One part of your terms: what the file said, or why it could not be read.
/// A file that is not there reads as empty, because that only means the term is not stated.
#[derive(Debug, Clone, PartialEq)]
pub enum TermPart<T> {
    Read(T),
    Failed(LoadError),
}

impl<T: Default> Default for TermPart<T> {
    fn default() -> Self {
        Self::Read(T::default())
    }
}

/// The commute tiers and the place they are measured from.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Commute {
    pub origin: Option<String>,
    pub tiers: Vec<CommuteTier>,
}

/// What the gates judge a posting against, with the state of each part.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct YourTerms {
    pub floor: TermPart<Option<u32>>,
    pub commute: TermPart<Commute>,
}

impl YourTerms {
    /// What the gates read. A part that failed gives nothing, exactly as a missing file does.
    pub fn gates(&self) -> FreelanceTerms {
        FreelanceTerms {
            day_rate_floor: match &self.floor {
                TermPart::Read(floor) => *floor,
                TermPart::Failed(_) => None,
            },
            commute: match &self.commute {
                TermPart::Read(commute) => commute.tiers.clone(),
                TermPart::Failed(_) => Vec::new(),
            },
        }
    }

    /// How many parts were read and say nothing.
    pub fn not_stated(&self) -> usize {
        let floor = matches!(self.floor, TermPart::Read(None));
        let commute = matches!(&self.commute, TermPart::Read(commute) if commute.tiers.is_empty());
        usize::from(floor) + usize::from(commute)
    }

    pub fn failures(&self) -> Vec<&LoadError> {
        let floor = match &self.floor {
            TermPart::Failed(error) => Some(error),
            TermPart::Read(_) => None,
        };
        let commute = match &self.commute {
            TermPart::Failed(error) => Some(error),
            TermPart::Read(_) => None,
        };
        floor.into_iter().chain(commute).collect()
    }

    /// Where the tiers are measured from, as a sentence: `From Namur (by car).`
    /// Only when there are tiers, because without them the commute is not stated.
    pub fn origin_line(&self) -> Option<String> {
        let TermPart::Read(Commute { origin: Some(origin), tiers }) = &self.commute else { return None };
        if tiers.is_empty() {
            return None;
        }
        let origin = origin.trim();
        let mut raised = false;
        let line: String = origin
            .chars()
            .flat_map(|c| {
                let raise = !raised && c.is_alphabetic();
                raised |= raise;
                if raise { c.to_uppercase().collect::<Vec<_>>() } else { vec![c] }
            })
            .collect();
        Some(if line.ends_with('.') { line } else { format!("{line}.") })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier() -> CommuteTier {
        CommuteTier { name: "ideal".into(), places: vec!["Namur".into()], note: String::new() }
    }

    fn failed() -> LoadError {
        LoadError::new("01-candidate-profile.md", "denied")
    }

    #[test]
    fn a_failed_part_gives_the_gates_nothing() {
        let terms = YourTerms { floor: TermPart::Failed(failed()), commute: TermPart::Read(Commute { origin: None, tiers: vec![tier()] }) };
        assert_eq!(terms.gates(), FreelanceTerms { day_rate_floor: None, commute: vec![tier()] });
        let terms = YourTerms { floor: TermPart::Read(Some(700)), commute: TermPart::Failed(failed()) };
        assert_eq!(terms.gates(), FreelanceTerms { day_rate_floor: Some(700), commute: vec![] });
    }

    #[test]
    fn counts_the_parts_read_without_a_value() {
        assert_eq!(YourTerms::default().not_stated(), 2);
        let stated = YourTerms { floor: TermPart::Read(Some(700)), commute: TermPart::Read(Commute { origin: None, tiers: vec![tier()] }) };
        assert_eq!(stated.not_stated(), 0);
        let failing = YourTerms { floor: TermPart::Failed(failed()), commute: TermPart::Failed(failed()) };
        assert_eq!(failing.not_stated(), 0);
        assert_eq!(failing.failures().len(), 2);
        assert!(stated.failures().is_empty());
    }

    #[test]
    fn the_origin_reads_as_a_sentence() {
        let with = |origin: Option<&str>, tiers: Vec<CommuteTier>| YourTerms {
            floor: TermPart::default(),
            commute: TermPart::Read(Commute { origin: origin.map(str::to_owned), tiers }),
        };
        assert_eq!(with(Some("from Namur (by car)"), vec![tier()]).origin_line().as_deref(), Some("From Namur (by car)."));
        assert_eq!(with(Some("from Namur."), vec![tier()]).origin_line().as_deref(), Some("From Namur."));
        assert_eq!(with(Some("(from Namur)"), vec![tier()]).origin_line().as_deref(), Some("(From Namur)."));
        assert_eq!(with(None, vec![tier()]).origin_line(), None);
        assert_eq!(with(Some("from Namur"), vec![]).origin_line(), None);
        let failing = YourTerms { floor: TermPart::default(), commute: TermPart::Failed(failed()) };
        assert_eq!(failing.origin_line(), None);
    }
}
