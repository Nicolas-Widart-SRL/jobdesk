use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    pub name: String,
    pub reached: bool,
    pub date: Option<NaiveDate>,
}

impl Stage {
    fn is_offer(&self) -> bool {
        self.name.to_lowercase().contains("offer")
    }
}

/// The parsed `outcome.md` of an application's archive folder.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutcomeRecord {
    pub status: Option<String>,
    pub resolved_on: Option<NaiveDate>,
    pub stages: Vec<Stage>,
    pub notes: String,
}

impl OutcomeRecord {
    pub fn reached_interview(&self) -> bool {
        self.stages.iter().any(|stage| stage.reached && !stage.is_offer())
    }

    pub fn reached_offer(&self) -> bool {
        self.stages.iter().any(|stage| stage.reached && stage.is_offer())
    }
}
