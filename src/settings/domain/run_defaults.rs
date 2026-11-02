use chrono::{Datelike, Days, NaiveDate, NaiveDateTime, TimeDelta};

use crate::shared_kernel::ModelChoice;

/// A sum of money in whole cents. It is never zero and never above $100,000.00.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dollars(u32);

impl Dollars {
    const MOST_CENTS: u32 = 10_000_000;

    /// What a user typed: an optional `$`, whole dollars and up to two decimals. No commas, no sign, no zero.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('$').map_or(text, str::trim_start);
        let (whole, fraction) = text.split_once('.').map_or((text, None), |(whole, fraction)| (whole, Some(fraction)));
        let digits = |part: &str| part.chars().all(|c| c.is_ascii_digit());
        if whole.is_empty() || whole.len() > 7 || !digits(whole) {
            return None;
        }
        let cents = match fraction {
            None => 0,
            Some(fraction) if (1..=2).contains(&fraction.len()) && digits(fraction) => {
                let value: u32 = fraction.parse().ok()?;
                if fraction.len() == 1 { value * 10 } else { value }
            }
            Some(_) => return None,
        };
        let total = whole.parse::<u32>().ok()?.checked_mul(100)?.checked_add(cents)?;
        Self::in_range(total)
    }

    /// Whole dollars, as the catalog's suggestions are.
    pub fn whole(dollars: u32) -> Self {
        Self(dollars.saturating_mul(100).min(Self::MOST_CENTS))
    }

    /// None for what is not a usable sum: not finite, zero (after rounding to the cent), negative or too large.
    pub fn from_usd(usd: f64) -> Option<Self> {
        if !usd.is_finite() || usd <= 0.0 || usd > f64::from(Self::MOST_CENTS) / 100.0 {
            return None;
        }
        Self::in_range((usd * 100.0).round() as u32)
    }

    fn in_range(cents: u32) -> Option<Self> {
        (1..=Self::MOST_CENTS).contains(&cents).then_some(Self(cents))
    }

    pub fn usd(self) -> f64 {
        f64::from(self.0) / 100.0
    }

    pub fn cents(self) -> u32 {
        self.0
    }

    /// The sum as a field holds it, without the unit: `3.00`.
    pub fn field_text(self) -> String {
        format!("{}.{:02}", self.0 / 100, self.0 % 100)
    }
}

/// What a task starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartValues {
    pub model: ModelChoice,
    pub budget: Dollars,
}

/// One command's own model and budget, which beat the defaults for every task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDefault {
    pub command: String,
    pub model: ModelChoice,
    pub budget: Dollars,
}

/// The model and budget a new task starts with.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunDefaults {
    pub model: ModelChoice,
    /// None leaves the budget to the command's suggestion.
    pub budget: Option<Dollars>,
    pub commands: Vec<CommandDefault>,
}

impl RunDefaults {
    /// A command's row beats both defaults, even when it picks Claude's default model.
    /// The budget for every task beats the catalog's suggestion. Only an exact command name matches.
    pub fn resolve(&self, command: &str, suggested: Dollars) -> StartValues {
        match self.commands.iter().find(|row| row.command == command) {
            Some(row) => StartValues { model: row.model, budget: row.budget },
            None => StartValues { model: self.model, budget: self.budget.unwrap_or(suggested) },
        }
    }
}

/// A limit on what one month of tasks may cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpendCap {
    pub on: bool,
    pub amount: Option<Dollars>,
}

/// The month's spend when it met the cap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapReached {
    pub spent_usd: f64,
    pub cap: Dollars,
    pub reopens_on: NaiveDate,
}

impl CapReached {
    /// Whether the month went past the cap, not just up to it. Both sides count in cents.
    pub fn over(&self) -> bool {
        spent_cents(self.spent_usd) > i64::from(self.cap.cents())
    }
}

fn spent_cents(spent_usd: f64) -> i64 {
    (spent_usd * 100.0).round() as i64
}

impl SpendCap {
    /// The cap in force: on, with an amount. A switch on without an amount limits nothing.
    pub fn active(&self) -> Option<Dollars> {
        if self.on { self.amount } else { None }
    }

    /// Reached when the spend is at or above the cap, compared in cents. The month turns the cap off by itself.
    pub fn reached(&self, spent_usd: f64, today: NaiveDate) -> Option<CapReached> {
        let cap = self.active()?;
        if spent_usd.is_nan() || spent_cents(spent_usd) < i64::from(cap.cents()) {
            return None;
        }
        Some(CapReached { spent_usd, cap, reopens_on: next_month(today) })
    }
}

fn next_month(today: NaiveDate) -> NaiveDate {
    let first = today.with_day(1).unwrap_or(today);
    first.checked_add_days(Days::new(32)).and_then(|date| date.with_day(1)).unwrap_or(today)
}

/// How long run logs stay before they may be deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Retention {
    Days30,
    Days90,
    #[default]
    Forever,
}

impl Retention {
    pub const ALL: [Retention; 3] = [Retention::Days30, Retention::Days90, Retention::Forever];

    pub fn label(self) -> &'static str {
        match self {
            Retention::Days30 => "30 days",
            Retention::Days90 => "90 days",
            Retention::Forever => "Forever",
        }
    }

    /// Logs last written before this moment may go. None keeps everything.
    /// It is never later than the start of the current month, so this month's logs always stay.
    pub fn keep_after(self, now: NaiveDateTime) -> Option<NaiveDateTime> {
        let days = match self {
            Retention::Days30 => 30,
            Retention::Days90 => 90,
            Retention::Forever => return None,
        };
        let by_age = now.checked_sub_signed(TimeDelta::days(days))?;
        let month_start = now.date().with_day(1)?.and_hms_opt(0, 0, 0)?;
        Some(by_age.min(month_start))
    }
}

/// Whether Jobdesk lets Claude use its tools on its own or asks first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionMode {
    #[default]
    AllowAndNote,
    Ask,
}

impl PermissionMode {
    pub fn asks(self) -> bool {
        self == PermissionMode::Ask
    }

    pub fn label(self) -> &'static str {
        match self {
            PermissionMode::AllowAndNote => "Allow and note",
            PermissionMode::Ask => "Ask first",
        }
    }
}

/// What Jobdesk keeps in its data folder, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DataSizes {
    pub run_logs: u64,
    pub decisions: u64,
}

impl DataSizes {
    pub fn total(&self) -> u64 {
        self.run_logs.saturating_add(self.decisions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn at(year: i32, month: u32, day: u32, hour: u32) -> NaiveDateTime {
        self::day(year, month, day).and_hms_opt(hour, 0, 0).unwrap()
    }

    fn dollars(text: &str) -> Dollars {
        Dollars::parse(text).unwrap()
    }

    fn row(command: &str, model: ModelChoice, budget: &str) -> CommandDefault {
        CommandDefault { command: command.to_owned(), model, budget: dollars(budget) }
    }

    #[test]
    fn without_defaults_a_task_starts_with_the_suggestion() {
        let values = RunDefaults::default().resolve("/rank", Dollars::whole(3));
        assert_eq!(values, StartValues { model: ModelChoice::Default, budget: Dollars::whole(3) });
    }

    #[test]
    fn the_defaults_for_every_task_set_model_and_budget() {
        let defaults = RunDefaults { model: ModelChoice::Opus, budget: Some(dollars("4")), commands: vec![] };
        assert_eq!(defaults.resolve("/rank", Dollars::whole(3)), StartValues { model: ModelChoice::Opus, budget: dollars("4.00") });
    }

    #[test]
    fn no_budget_for_every_task_keeps_the_suggestion() {
        let defaults = RunDefaults { model: ModelChoice::Haiku, budget: None, commands: vec![] };
        assert_eq!(defaults.resolve("/rank", Dollars::whole(3)), StartValues { model: ModelChoice::Haiku, budget: Dollars::whole(3) });
    }

    #[test]
    fn a_command_row_beats_the_defaults_for_every_task() {
        let defaults = RunDefaults { model: ModelChoice::Opus, budget: Some(dollars("4")), commands: vec![row("/rank", ModelChoice::Opus, "4.00")] };
        assert_eq!(defaults.resolve("/rank", Dollars::whole(3)), StartValues { model: ModelChoice::Opus, budget: dollars("4.00") });
        let other = RunDefaults { commands: vec![row("/rank", ModelChoice::Sonnet, "2.50")], ..defaults };
        assert_eq!(other.resolve("/rank", Dollars::whole(3)), StartValues { model: ModelChoice::Sonnet, budget: dollars("2.50") });
    }

    #[test]
    fn a_row_with_the_default_model_still_beats_the_model_for_every_task() {
        let defaults = RunDefaults { model: ModelChoice::Opus, budget: None, commands: vec![row("/rank", ModelChoice::Default, "1")] };
        assert_eq!(defaults.resolve("/rank", Dollars::whole(3)), StartValues { model: ModelChoice::Default, budget: dollars("1.00") });
    }

    #[test]
    fn only_the_exact_command_name_matches() {
        let defaults = RunDefaults { model: ModelChoice::Opus, budget: None, commands: vec![row("/ranking", ModelChoice::Haiku, "1")] };
        assert_eq!(defaults.resolve("/rank", Dollars::whole(3)), StartValues { model: ModelChoice::Opus, budget: Dollars::whole(3) });
    }

    #[test]
    fn dollars_parse_what_a_person_types() {
        for (text, cents) in
            [("3", 300), ("3.5", 350), ("3.50", 350), ("$3.00", 300), ("  $ 12.34 ", 1234), ("0.01", 1), ("100000", 10_000_000), ("100000.00", 10_000_000)]
        {
            assert_eq!(Dollars::parse(text).map(Dollars::cents), Some(cents), "{text:?}");
        }
    }

    #[test]
    fn dollars_refuse_what_is_not_a_usable_sum() {
        for text in [
            "",
            " ",
            "$",
            "abc",
            "0",
            "0.00",
            "-3",
            "+3",
            "3.",
            ".5",
            "3.123",
            "1,000",
            "1 000",
            "3 .00",
            "100000.01",
            "100001",
            "99999999999",
            "1e3",
            "NaN",
            "٣",
            "3.5.0",
            "$$3",
        ] {
            assert_eq!(Dollars::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn dollars_from_a_number_round_to_the_cent() {
        assert_eq!(Dollars::from_usd(3.0).map(Dollars::cents), Some(300));
        assert_eq!(Dollars::from_usd(4.5).map(Dollars::cents), Some(450));
        assert_eq!(Dollars::from_usd(0.014).map(Dollars::cents), Some(1));
        assert_eq!(Dollars::from_usd(100_000.0).map(Dollars::cents), Some(10_000_000));
        for usd in [0.0, 0.004, -1.0, 100_000.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(Dollars::from_usd(usd), None, "{usd}");
        }
    }

    #[test]
    fn dollars_read_back_as_a_number_and_as_field_text() {
        let sum = dollars("3");
        assert_eq!((sum.usd(), sum.field_text(), sum.cents()), (3.0, "3.00".to_owned(), 300));
        assert_eq!(dollars("0.05").field_text(), "0.05");
        assert_eq!(dollars("1234.5").field_text(), "1234.50");
        assert_eq!(Dollars::whole(5), dollars("5.00"));
        assert_eq!(Dollars::from_usd(dollars("12.34").usd()), Some(dollars("12.34")));
    }

    fn cap(amount: &str) -> SpendCap {
        SpendCap { on: true, amount: Dollars::parse(amount) }
    }

    #[test]
    fn a_cap_is_reached_at_or_over_the_amount() {
        let today = day(2026, 10, 31);
        assert_eq!(cap("60").reached(59.99, today), None);
        let at = cap("60").reached(60.0, today).unwrap();
        assert_eq!((at.cap, at.spent_usd, at.over()), (dollars("60"), 60.0, false));
        let over = cap("60").reached(60.4, today).unwrap();
        assert!(over.over());
        assert_eq!(over.reopens_on, day(2026, 11, 1));
    }

    #[test]
    fn a_cap_compares_in_cents() {
        let today = day(2026, 10, 5);
        assert!(cap("60").reached(59.996, today).is_some_and(|reached| !reached.over()), "a float sum a hair short is still at the cap");
        assert_eq!(cap("60").reached(59.994, today), None);
        assert_eq!(cap("60").reached(f64::NAN, today), None);
    }

    #[test]
    fn a_cap_that_is_off_or_has_no_amount_limits_nothing() {
        let today = day(2026, 10, 31);
        assert_eq!(SpendCap { on: false, amount: Dollars::parse("60") }.reached(500.0, today), None);
        assert_eq!(SpendCap { on: true, amount: None }.reached(500.0, today), None);
        assert_eq!(SpendCap { on: false, amount: Dollars::parse("60") }.active(), None);
        assert_eq!(cap("60").active(), Some(dollars("60")));
        assert_eq!(SpendCap::default().active(), None);
    }

    #[test]
    fn a_reached_cap_reopens_on_the_first_of_the_next_month() {
        let reopens = |today| cap("1").reached(5.0, today).unwrap().reopens_on;
        assert_eq!(reopens(day(2026, 12, 15)), day(2027, 1, 1));
        assert_eq!(reopens(day(2026, 12, 31)), day(2027, 1, 1));
        assert_eq!(reopens(day(2026, 1, 31)), day(2026, 2, 1));
        assert_eq!(reopens(day(2026, 2, 28)), day(2026, 3, 1));
        assert_eq!(reopens(day(2026, 11, 1)), day(2026, 12, 1));
    }

    #[test]
    fn forever_keeps_every_log() {
        assert_eq!(Retention::Forever.keep_after(at(2026, 11, 20, 12)), None);
        assert_eq!(Retention::default(), Retention::Forever);
    }

    #[test]
    fn thirty_days_reach_back_thirty_days_in_mid_month() {
        assert_eq!(Retention::Days30.keep_after(at(2026, 11, 20, 12)), Some(at(2026, 10, 21, 12)));
    }

    #[test]
    fn this_months_logs_are_never_deleted() {
        assert_eq!(Retention::Days30.keep_after(at(2026, 10, 31, 9)), Some(at(2026, 10, 1, 0)));
        assert_eq!(Retention::Days30.keep_after(at(2026, 11, 1, 0)), Some(at(2026, 10, 2, 0)));
        for now in [at(2026, 11, 2, 8), at(2026, 3, 31, 23), at(2026, 1, 1, 0)] {
            for retention in [Retention::Days30, Retention::Days90] {
                let cutoff = retention.keep_after(now).unwrap();
                assert!(cutoff <= now.date().with_day(1).unwrap().and_hms_opt(0, 0, 0).unwrap());
            }
        }
    }

    #[test]
    fn ninety_days_reach_back_ninety_days() {
        assert_eq!(Retention::Days90.keep_after(at(2026, 11, 2, 0)), Some(at(2026, 8, 4, 0)));
        assert_eq!(Retention::Days90.keep_after(at(2026, 11, 2, 15)), Some(at(2026, 8, 4, 15)));
    }

    #[test]
    fn retention_and_permission_words() {
        let words: Vec<_> = Retention::ALL.iter().map(|retention| retention.label()).collect();
        assert_eq!(words, ["30 days", "90 days", "Forever"]);
        assert_eq!((PermissionMode::AllowAndNote.label(), PermissionMode::Ask.label()), ("Allow and note", "Ask first"));
        assert_eq!((PermissionMode::default().asks(), PermissionMode::Ask.asks()), (false, true));
    }

    #[test]
    fn sizes_add_up() {
        assert_eq!(DataSizes { run_logs: 900, decisions: 100 }.total(), 1000);
        assert_eq!(DataSizes { run_logs: u64::MAX, decisions: 1 }.total(), u64::MAX);
    }
}
