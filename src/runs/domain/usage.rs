use chrono::{Datelike, Days, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOutcome {
    Finished,
    Stopped,
    Failed,
}

/// What one model used and cost inside one task.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCost {
    pub model: String,
    /// Input plus cache creation plus cache reads.
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    pub cost_usd: f64,
}

/// What one task cost, from the log it left.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskCost {
    pub started_on: NaiveDate,
    /// The slash command word. None for a typed task with no leading `/`.
    pub command: Option<String>,
    pub outcome: TaskOutcome,
    pub cost_usd: f64,
    pub models: Vec<ModelCost>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Period {
    #[default]
    Month,
    Days90,
    Year,
    All,
}

impl Period {
    pub const ALL: [Period; 4] = [Period::Month, Period::Days90, Period::Year, Period::All];

    /// The first day the period covers. None when it reaches back without limit.
    pub fn first_day(self, today: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Month => today.with_day(1),
            Self::Days90 => today.checked_sub_days(Days::new(89)),
            Self::Year => today.with_ordinal(1),
            Self::All => None,
        }
    }

    pub fn contains(self, day: NaiveDate, today: NaiveDate) -> bool {
        self.first_day(today).is_none_or(|first| day >= first)
    }
}

/// How strongly a heat-map cell shows a day with this many tasks.
pub fn heat_step(tasks: usize) -> u8 {
    match tasks {
        0 => 0,
        1 => 1,
        2 | 3 => 2,
        _ => 3,
    }
}

/// The model family a model id belongs to, so versions of one family group together.
pub fn model_family(id: &str) -> String {
    let lower = id.to_ascii_lowercase();
    ["Opus", "Sonnet", "Haiku"].into_iter().find(|family| lower.contains(&family.to_ascii_lowercase())).map_or_else(|| id.to_owned(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn the_month_starts_on_the_first() {
        assert_eq!(Period::Month.first_day(today()), Some(day(2026, 10, 1)));
        assert!(Period::Month.contains(day(2026, 10, 1), today()));
        assert!(!Period::Month.contains(day(2026, 9, 30), today()));
    }

    #[test]
    fn ninety_days_include_today_and_the_89_before() {
        assert_eq!(Period::Days90.first_day(today()), Some(day(2026, 8, 3)));
        assert!(Period::Days90.contains(day(2026, 8, 3), today()));
        assert!(!Period::Days90.contains(day(2026, 8, 2), today()));
    }

    #[test]
    fn the_year_starts_on_the_first_of_january() {
        assert_eq!(Period::Year.first_day(today()), Some(day(2026, 1, 1)));
        assert!(!Period::Year.contains(day(2025, 12, 31), today()));
    }

    #[test]
    fn all_reaches_back_without_limit() {
        assert_eq!(Period::All.first_day(today()), None);
        assert!(Period::All.contains(day(2019, 3, 4), today()));
    }

    #[test]
    fn heat_steps_widen_with_the_task_count() {
        let steps: Vec<u8> = [0, 1, 2, 3, 4, 9].into_iter().map(heat_step).collect();
        assert_eq!(steps, vec![0, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn model_ids_fold_into_their_family() {
        assert_eq!(model_family("claude-opus-4-5"), "Opus");
        assert_eq!(model_family("claude-sonnet-4-5-20250929"), "Sonnet");
        assert_eq!(model_family("claude-haiku-4-5"), "Haiku");
        assert_eq!(model_family("acme-model-9"), "acme-model-9");
    }
}
