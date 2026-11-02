use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Days, NaiveDate};

use crate::runs::domain::{Period, TaskCost, TaskOutcome, model_family};

const WEEKS_SHOWN: usize = 53;
const DAYS_IN_WEEK: usize = 7;

/// What past tasks cost, ready for the Usage page. Dates the screens ask for are the only clock it knows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UsageReport {
    tasks: Vec<TaskCost>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UsageTotals {
    pub spent_usd: f64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    pub tasks: usize,
    pub finished: usize,
    pub stopped: usize,
    pub failed: usize,
}

impl UsageTotals {
    /// The share of input tokens served from the cache. None when nothing was read.
    pub fn cache_share(&self) -> Option<f32> {
        (self.tokens_in > 0).then(|| self.cache_read as f32 / self.tokens_in as f32)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandRow {
    /// None groups the typed tasks.
    pub command: Option<String>,
    pub tasks: usize,
    pub spent_usd: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRow {
    pub model: String,
    pub tasks: usize,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub spent_usd: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayCount {
    pub date: NaiveDate,
    pub tasks: usize,
    pub spent_usd: f64,
}

/// The last 53 weeks as columns, Monday first, with today in the last column.
#[derive(Debug, Clone, PartialEq)]
pub struct YearOfDays {
    pub weeks: Vec<[Option<DayCount>; 7]>,
    /// The column and row of today.
    pub today_at: (usize, usize),
    /// The earliest task on record up to today, inside the grid or not.
    pub first_task: Option<NaiveDate>,
    /// Days from `first_task` to today that ran at least one task, inside the grid or not.
    pub active_days: usize,
}

impl UsageReport {
    pub fn new(tasks: Vec<TaskCost>) -> Self {
        Self { tasks }
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    fn within(&self, period: Period, today: NaiveDate) -> impl Iterator<Item = &TaskCost> {
        self.tasks.iter().filter(move |task| period.contains(task.started_on, today))
    }

    pub fn totals(&self, period: Period, today: NaiveDate) -> UsageTotals {
        let mut totals = UsageTotals::default();
        for task in self.within(period, today) {
            totals.spent_usd += task.cost_usd;
            totals.tasks += 1;
            match task.outcome {
                TaskOutcome::Finished => totals.finished += 1,
                TaskOutcome::Stopped => totals.stopped += 1,
                TaskOutcome::Failed => totals.failed += 1,
            }
            for model in &task.models {
                totals.tokens_in += model.tokens_in;
                totals.tokens_out += model.tokens_out;
                totals.cache_read += model.cache_read;
            }
        }
        totals
    }

    pub fn by_command(&self, period: Period, today: NaiveDate) -> Vec<CommandRow> {
        let mut rows: BTreeMap<Option<&str>, CommandRow> = BTreeMap::new();
        for task in self.within(period, today) {
            let row = rows.entry(task.command.as_deref()).or_insert_with(|| CommandRow { command: task.command.clone(), tasks: 0, spent_usd: 0.0 });
            row.tasks += 1;
            row.spent_usd += task.cost_usd;
        }
        let mut rows: Vec<_> = rows.into_values().collect();
        rows.sort_by(|a, b| b.spent_usd.total_cmp(&a.spent_usd).then_with(|| a.command.cmp(&b.command)));
        rows
    }

    /// A task that used two models of one family counts once for that family.
    pub fn by_model(&self, period: Period, today: NaiveDate) -> Vec<ModelRow> {
        let mut rows: HashMap<String, ModelRow> = HashMap::new();
        for task in self.within(period, today) {
            let mut seen: Vec<String> = Vec::new();
            for model in &task.models {
                let family = model_family(&model.model);
                let row =
                    rows.entry(family.clone()).or_insert_with(|| ModelRow { model: family.clone(), tasks: 0, tokens_in: 0, tokens_out: 0, spent_usd: 0.0 });
                if !seen.contains(&family) {
                    row.tasks += 1;
                    seen.push(family);
                }
                row.tokens_in += model.tokens_in;
                row.tokens_out += model.tokens_out;
                row.spent_usd += model.cost_usd;
            }
        }
        let mut rows: Vec<_> = rows.into_values().collect();
        rows.sort_by(|a, b| b.spent_usd.total_cmp(&a.spent_usd).then_with(|| a.model.cmp(&b.model)));
        rows
    }

    pub fn year(&self, today: NaiveDate) -> YearOfDays {
        let this_monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
        let first_monday = this_monday - Days::new(7 * (WEEKS_SHOWN as u64 - 1));
        let mut per_day: HashMap<NaiveDate, (usize, f64)> = HashMap::new();
        for task in self.tasks.iter().filter(|task| task.started_on >= first_monday && task.started_on <= today) {
            let entry = per_day.entry(task.started_on).or_default();
            entry.0 += 1;
            entry.1 += task.cost_usd;
        }
        let weeks = (0..WEEKS_SHOWN)
            .map(|week| {
                std::array::from_fn(|row| {
                    let date = first_monday + Days::new((week * DAYS_IN_WEEK + row) as u64);
                    (date <= today).then(|| {
                        let (tasks, spent_usd) = per_day.get(&date).copied().unwrap_or_default();
                        DayCount { date, tasks, spent_usd }
                    })
                })
            })
            .collect();
        let on_record: HashSet<NaiveDate> = self.tasks.iter().map(|task| task.started_on).filter(|&day| day <= today).collect();
        YearOfDays {
            weeks,
            today_at: (WEEKS_SHOWN - 1, today.weekday().num_days_from_monday() as usize),
            first_task: on_record.iter().min().copied(),
            active_days: on_record.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::domain::ModelCost;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn model(id: &str, tokens_in: u64, tokens_out: u64, cache_read: u64, cost_usd: f64) -> ModelCost {
        ModelCost { model: id.into(), tokens_in, tokens_out, cache_read, cost_usd }
    }

    fn task(on: NaiveDate, command: Option<&str>, outcome: TaskOutcome, models: Vec<ModelCost>) -> TaskCost {
        TaskCost { started_on: on, command: command.map(str::to_owned), outcome, cost_usd: models.iter().map(|m| m.cost_usd).sum(), models }
    }

    fn finished(on: NaiveDate, command: Option<&str>, cost_usd: f64) -> TaskCost {
        task(on, command, TaskOutcome::Finished, vec![model("claude-sonnet-4-5", 100, 10, 0, cost_usd)])
    }

    #[test]
    fn totals_add_only_what_falls_inside_the_period() {
        let report = UsageReport::new(vec![
            task(day(2026, 10, 3), Some("/find-jobs"), TaskOutcome::Finished, vec![model("claude-opus-4-5", 1_000, 200, 800, 1.5)]),
            task(day(2026, 10, 20), Some("/apply"), TaskOutcome::Stopped, vec![model("claude-opus-4-5", 500, 50, 100, 0.25)]),
            task(day(2026, 10, 21), None, TaskOutcome::Failed, vec![model("claude-haiku-4-5", 40, 5, 0, 0.05)]),
            task(day(2026, 9, 30), Some("/apply"), TaskOutcome::Finished, vec![model("claude-opus-4-5", 9_000, 900, 0, 9.0)]),
        ]);

        let totals = report.totals(Period::Month, today());

        assert_eq!((totals.tasks, totals.finished, totals.stopped, totals.failed), (3, 1, 1, 1));
        assert_eq!((totals.tokens_in, totals.tokens_out, totals.cache_read), (1_540, 255, 900));
        assert!((totals.spent_usd - 1.8).abs() < 1e-9);
        assert_eq!(report.totals(Period::All, today()).tasks, 4);
    }

    #[test]
    fn the_cache_share_is_the_cached_part_of_the_input() {
        let report = UsageReport::new(vec![task(day(2026, 10, 3), None, TaskOutcome::Finished, vec![model("claude-opus-4-5", 100, 5, 82, 1.0)])]);
        assert_eq!(report.totals(Period::Month, today()).cache_share(), Some(0.82));
        assert_eq!(UsageTotals::default().cache_share(), None);
    }

    #[test]
    fn commands_sort_by_spend_and_typed_tasks_group_under_none() {
        let report = UsageReport::new(vec![
            finished(day(2026, 10, 3), Some("/apply"), 0.5),
            finished(day(2026, 10, 4), None, 2.0),
            finished(day(2026, 10, 5), Some("/find-jobs"), 0.5),
            finished(day(2026, 10, 6), None, 1.0),
            finished(day(2026, 10, 7), Some("/apply"), 0.25),
        ]);

        let rows = report.by_command(Period::Month, today());

        let seen: Vec<_> = rows.iter().map(|row| (row.command.as_deref(), row.tasks)).collect();
        assert_eq!(seen, vec![(None, 2), (Some("/apply"), 2), (Some("/find-jobs"), 1)]);
        assert!((rows[0].spent_usd - 3.0).abs() < 1e-9);
    }

    #[test]
    fn equal_spend_sorts_by_command_name() {
        let report = UsageReport::new(vec![finished(day(2026, 10, 3), Some("/outcome"), 1.0), finished(day(2026, 10, 3), Some("/apply"), 1.0)]);
        let names: Vec<_> = report.by_command(Period::Month, today()).into_iter().map(|row| row.command).collect();
        assert_eq!(names, vec![Some("/apply".to_owned()), Some("/outcome".to_owned())]);
    }

    #[test]
    fn models_merge_by_family_and_a_task_counts_once() {
        let report = UsageReport::new(vec![
            task(
                day(2026, 10, 3),
                Some("/apply"),
                TaskOutcome::Finished,
                vec![model("claude-opus-4-5", 100, 10, 0, 1.0), model("claude-opus-4-1", 50, 5, 0, 0.5), model("claude-haiku-4-5", 20, 2, 0, 0.1)],
            ),
            task(day(2026, 10, 4), Some("/apply"), TaskOutcome::Finished, vec![model("claude-opus-4-5", 10, 1, 0, 0.2)]),
            task(day(2026, 8, 4), Some("/apply"), TaskOutcome::Finished, vec![model("claude-opus-4-5", 7, 7, 0, 7.0)]),
        ]);

        let rows = report.by_model(Period::Month, today());

        assert_eq!(rows.len(), 2);
        let opus = &rows[0];
        assert_eq!((opus.model.as_str(), opus.tasks, opus.tokens_in, opus.tokens_out), ("Opus", 2, 160, 16));
        assert!((opus.spent_usd - 1.7).abs() < 1e-9);
        assert_eq!((rows[1].model.as_str(), rows[1].tasks), ("Haiku", 1));
    }

    #[test]
    fn the_year_has_53_weeks_ending_with_today() {
        let report = UsageReport::new(vec![]);

        let year = report.year(today());

        assert_eq!(year.weeks.len(), 53);
        assert_eq!(year.today_at, (52, 5));
        assert_eq!(year.weeks[0][0].map(|cell| cell.date), Some(day(2025, 10, 27)));
        assert_eq!(year.weeks[52][5].map(|cell| cell.date), Some(today()));
        assert!(year.weeks[52][6].is_none());
        assert!(year.weeks[52][..6].iter().all(Option::is_some));
        assert!(year.weeks[..52].iter().flatten().all(Option::is_some));
    }

    #[test]
    fn the_year_counts_tasks_by_day() {
        let report = UsageReport::new(vec![
            finished(day(2026, 10, 31), Some("/apply"), 0.5),
            finished(day(2026, 10, 31), Some("/apply"), 0.25),
            finished(day(2026, 10, 12), None, 1.0),
            finished(day(2025, 10, 27), None, 1.0),
            finished(day(2025, 10, 26), None, 1.0),
            finished(day(2026, 11, 1), None, 1.0),
            finished(day(2019, 3, 4), None, 1.0),
        ]);

        let year = report.year(today());

        let cell = year.weeks[52][5].unwrap();
        assert_eq!((cell.tasks, cell.spent_usd), (2, 0.75));
        assert_eq!(year.weeks[0][0].unwrap().tasks, 1);
        assert_eq!(year.weeks[51][0].unwrap().tasks, 0);
        assert_eq!(year.active_days, 5, "every day on record up to today, inside the grid or not");
        assert_eq!(year.first_task, Some(day(2019, 3, 4)));
        assert_eq!(UsageReport::default().year(today()).first_task, None);
    }

    #[test]
    fn a_task_after_today_is_not_on_record_yet() {
        let report = UsageReport::new(vec![finished(day(2026, 11, 1), Some("/apply"), 1.0)]);
        let year = report.year(today());
        assert_eq!((year.first_task, year.active_days), (None, 0));
        assert_eq!(report.totals(Period::All, today()).tasks, 0);
    }

    #[test]
    fn a_monday_today_sits_in_the_first_row() {
        let monday = day(2026, 11, 2);
        let year = UsageReport::default().year(monday);
        assert_eq!(year.today_at, (52, 0));
        assert!(year.weeks[52][1..].iter().all(Option::is_none));
    }

    #[test]
    fn an_empty_report_says_so() {
        assert!(UsageReport::default().is_empty());
        assert!(!UsageReport::new(vec![finished(today(), None, 0.1)]).is_empty());
    }
}
