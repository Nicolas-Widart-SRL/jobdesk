/// The model a task asks for. `Default` leaves the choice to the `claude` CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelChoice {
    #[default]
    Default,
    Opus,
    Sonnet,
    Haiku,
}

impl ModelChoice {
    pub const ALL: [ModelChoice; 4] = [ModelChoice::Default, ModelChoice::Opus, ModelChoice::Sonnet, ModelChoice::Haiku];

    pub fn label(self) -> &'static str {
        match self {
            ModelChoice::Default => "Claude default",
            ModelChoice::Opus => "Opus",
            ModelChoice::Sonnet => "Sonnet",
            ModelChoice::Haiku => "Haiku",
        }
    }

    /// The name `claude --model` takes. None leaves the flag off.
    pub fn alias(self) -> Option<&'static str> {
        match self {
            ModelChoice::Default => None,
            ModelChoice::Opus => Some("opus"),
            ModelChoice::Sonnet => Some("sonnet"),
            ModelChoice::Haiku => Some("haiku"),
        }
    }

    /// An alias nobody knows reads as the default, so a stale file never picks a model by accident.
    pub fn from_alias(alias: &str) -> ModelChoice {
        match alias {
            "opus" => ModelChoice::Opus,
            "sonnet" => ModelChoice::Sonnet,
            "haiku" => ModelChoice::Haiku,
            _ => ModelChoice::Default,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_choice_round_trips_through_its_alias() {
        for choice in ModelChoice::ALL {
            assert_eq!(ModelChoice::from_alias(choice.alias().unwrap_or("default")), choice);
        }
    }

    #[test]
    fn labels_and_aliases_are_the_launchers() {
        let words: Vec<_> = ModelChoice::ALL.iter().map(|choice| (choice.label(), choice.alias())).collect();
        assert_eq!(words, vec![("Claude default", None), ("Opus", Some("opus")), ("Sonnet", Some("sonnet")), ("Haiku", Some("haiku"))]);
    }

    #[test]
    fn an_unknown_alias_is_the_default() {
        for alias in ["", "Opus", "gpt", "default", "opus "] {
            assert_eq!(ModelChoice::from_alias(alias), ModelChoice::Default, "{alias:?}");
        }
        assert_eq!(ModelChoice::default(), ModelChoice::Default);
    }
}
