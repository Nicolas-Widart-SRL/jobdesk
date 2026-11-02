use chrono::{Datelike, NaiveDate};

use crate::runs::application::{CommandRow, DayCount, ModelRow, Period, UsageReport, UsageTotals, YearOfDays};
use crate::shared_kernel::visible_text::visible_chars;
use crate::tracking::application::Pipeline;

/// What Usage statistics has to show: no folder to read, logs still being read, or the report as read.
#[derive(Debug, Clone, Copy)]
pub enum UsageState<'a> {
    NoFolder,
    Reading,
    Read { report: &'a UsageReport, pipeline: &'a Pipeline, today: NaiveDate },
}

/// One period of usage, ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct UsagePage {
    pub totals: UsageTotals,
    /// What the period's tasks cost per application sent in it, with how many were sent. None when none was sent.
    pub per_application: Option<(f64, usize)>,
    pub commands: Vec<CommandRow>,
    pub models: Vec<ModelRow>,
    /// Always the last 12 months, whatever the period.
    pub year: YearOfDays,
}

/// The period's figures and tables. Applications sent come from tracking, counted over the same days as the tasks.
pub fn usage_page(report: &UsageReport, pipeline: &Pipeline, period: Period, today: NaiveDate) -> UsagePage {
    let totals = report.totals(period, today);
    let sent = pipeline.sent_between(period.first_day(today), today);
    let per_application = (sent > 0).then(|| (totals.spent_usd / sent as f64, sent));
    UsagePage { per_application, commands: report.by_command(period, today), models: report.by_model(period, today), year: report.year(today), totals }
}

/// Dollars to the cent, with thousands set apart: `$41.20`, `$1,204.00`.
pub fn money(usd: f64) -> String {
    let cents = (usd.max(0.0) * 100.0).round() as u64;
    let dollars = (cents / 100).to_string();
    let mut grouped = String::new();
    for (at, digit) in dollars.chars().enumerate() {
        if at > 0 && (dollars.len() - at).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("${grouped}.{:02}", cents % 100)
}

/// A token count at a glance: `4.1M`, `310K`, `950`.
pub fn tokens(count: u64) -> String {
    let thousands = (count as f64 / 1_000.0).round();
    if count < 1_000 {
        count.to_string()
    } else if thousands < 1_000.0 {
        format!("{thousands}K")
    } else {
        let millions = format!("{:.1}", count as f64 / 1_000_000.0);
        format!("{}M", millions.trim_end_matches(".0"))
    }
}

/// A share as a whole percent: `82%`.
pub fn percent(share: f32) -> String {
    format!("{}%", (share.clamp(0.0, 1.0) * 100.0).round())
}

/// How the tasks ended: "33 finished, 3 stopped, 1 failed". Stopped and failed show only when some did.
pub fn tasks_note(totals: &UsageTotals) -> String {
    let mut parts = vec![format!("{} finished", totals.finished)];
    if totals.stopped > 0 {
        parts.push(format!("{} stopped", totals.stopped));
    }
    if totals.failed > 0 {
        parts.push(format!("{} failed", totals.failed));
    }
    parts.join(", ")
}

/// The period's toggle: the month by name, "90 days", "This year" or "All time".
pub fn period_label(period: Period, today: NaiveDate) -> String {
    match period {
        Period::Month => today.format("%B").to_string(),
        Period::Days90 => "90 days".to_owned(),
        Period::Year => "This year".to_owned(),
        Period::All => "All time".to_owned(),
    }
}

/// The period as words that end a sentence: "in October", "in the last 90 days", "this year", "in all".
pub fn period_words(period: Period, today: NaiveDate) -> String {
    match period {
        Period::Month => format!("in {}", today.format("%B")),
        Period::Days90 => "in the last 90 days".to_owned(),
        Period::Year => "this year".to_owned(),
        Period::All => "in all".to_owned(),
    }
}

/// The sentence beside the cost per application sent.
pub fn sent_note(sent: usize, period: Period, today: NaiveDate) -> String {
    let applications = if sent == 1 { "1 application".to_owned() } else { format!("{sent} applications") };
    format!("You marked {applications} sent {}. Every task's cost counts, not only drafting.", period_words(period, today))
}

/// A day's tooltip: "Tue 13 Oct: 4 tasks, $6.10", "1 task", or "no tasks".
pub fn day_tip(day: &DayCount) -> String {
    let what = match day.tasks {
        0 => "no tasks".to_owned(),
        1 => format!("1 task, {}", money(day.spent_usd)),
        tasks => format!("{tasks} tasks, {}", money(day.spent_usd)),
    };
    format!("{}: {what}", day.date.format("%a %-d %b"))
}

/// The note beside the year: how many days ran a task, and since when. "No task yet" before the first.
pub fn active_days_note(year: &YearOfDays, today: NaiveDate) -> String {
    let Some(first) = year.first_task else { return "No task yet".to_owned() };
    let days = if year.active_days == 1 { "1 day".to_owned() } else { format!("{} days", year.active_days) };
    let since = if first.year() == today.year() { first.format("%-d %B") } else { first.format("%-d %B %Y") };
    format!("{days} with a task, since {since}")
}

/// Each month's short name over the first week that starts in it, leaving out the first column's month.
pub fn month_marks(year: &YearOfDays) -> Vec<(usize, String)> {
    year.weeks
        .iter()
        .enumerate()
        .filter_map(|(week, days)| {
            let monday = days[0]?.date;
            (week > 0 && monday.day() <= 7).then(|| (week, monday.format("%b").to_string()))
        })
        .collect()
}

/// A By command row's name: the slash command as typed, its `/` included, or "Other tasks" for the ones typed in full.
pub fn command_name(row: &CommandRow) -> String {
    row.command.clone().unwrap_or_else(|| "Other tasks".to_owned())
}

/// A By model row's name. A model outside the known families shows its id from the log, so only its visible characters show.
pub fn model_name(row: &ModelRow) -> String {
    visible_chars(&row.model).collect()
}

/// The word beside Usage statistics in the rail: "$41.20 in Oct", or "No tasks yet" before the first task.
pub fn rail_word(report: &UsageReport, today: NaiveDate) -> String {
    if report.is_empty() {
        return "No tasks yet".to_owned();
    }
    format!("{} in {}", money(report.totals(Period::Month, today).spent_usd), today.format("%b"))
}

/// A pipeline of fictional applications, one sent on each of `days`, for the tests of Usage statistics.
#[cfg(test)]
pub fn sent_on(days: &[NaiveDate]) -> Pipeline {
    use crate::tracking::application::{ApplicationCard, PipelineLane, ReachedStage, StatusBucket};
    let card = |day: &NaiveDate| ApplicationCard {
        company: "Acme".into(),
        role: "Lead".into(),
        sector: String::new(),
        channel: String::new(),
        status_label: "Applied".into(),
        bucket: StatusBucket::Active,
        applied_on: day.to_string(),
        sent_on: Some(*day),
        deadline: None,
        deadline_state: None,
        contact_person: String::new(),
        fit_rating: None,
        notes: String::new(),
        source: None,
        days_quiet: None,
        days_sent: None,
        days_followed_up: None,
        follow_ups_sent: 0,
        follow_up_due: false,
        stale: false,
        reached: ReachedStage::Applied,
        is_rejection: false,
        is_final: false,
        cv_file: String::new(),
        cover_letter_file: String::new(),
        archive_folder: None,
        outcome: None,
    };
    Pipeline { lanes: vec![PipelineLane { bucket: StatusBucket::Active, cards: days.iter().map(card).collect() }] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::application::{ModelCost, TaskCost, TaskOutcome};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn today() -> NaiveDate {
        day(2026, 10, 31)
    }

    fn task(on: NaiveDate, command: Option<&str>, cost_usd: f64) -> TaskCost {
        let model = ModelCost { model: "claude-sonnet-4-5".into(), tokens_in: 1_000, tokens_out: 100, cache_read: 800, cost_usd };
        TaskCost { started_on: on, command: command.map(str::to_owned), outcome: TaskOutcome::Finished, cost_usd, models: vec![model] }
    }

    #[test]
    fn money_shows_cents_and_groups_thousands() {
        assert_eq!(money(41.2), "$41.20");
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(6.105), "$6.11");
        assert_eq!(money(1_204.0), "$1,204.00");
        assert_eq!(money(1_234_567.891), "$1,234,567.89");
    }

    #[test]
    fn tokens_read_in_millions_thousands_or_plain() {
        assert_eq!(tokens(4_100_000), "4.1M");
        assert_eq!(tokens(2_000_000), "2M");
        assert_eq!(tokens(310_000), "310K");
        assert_eq!(tokens(999_700), "1M");
        assert_eq!(tokens(1_000), "1K");
        assert_eq!(tokens(950), "950");
        assert_eq!(tokens(0), "0");
    }

    #[test]
    fn percent_rounds_to_a_whole_number() {
        assert_eq!(percent(0.82), "82%");
        assert_eq!(percent(0.8249), "82%");
        assert_eq!(percent(1.0), "100%");
        assert_eq!(percent(0.0), "0%");
    }

    #[test]
    fn the_tasks_note_names_only_the_endings_that_happened() {
        let all = UsageTotals { tasks: 37, finished: 33, stopped: 3, failed: 1, ..UsageTotals::default() };
        assert_eq!(tasks_note(&all), "33 finished, 3 stopped, 1 failed");
        let clean = UsageTotals { tasks: 5, finished: 5, ..UsageTotals::default() };
        assert_eq!(tasks_note(&clean), "5 finished");
        let failed = UsageTotals { tasks: 1, failed: 1, ..UsageTotals::default() };
        assert_eq!(tasks_note(&failed), "0 finished, 1 failed");
    }

    #[test]
    fn periods_have_a_label_and_words() {
        let labels: Vec<String> = Period::ALL.iter().map(|&period| period_label(period, today())).collect();
        assert_eq!(labels, ["October", "90 days", "This year", "All time"]);
        let words: Vec<String> = Period::ALL.iter().map(|&period| period_words(period, today())).collect();
        assert_eq!(words, ["in October", "in the last 90 days", "this year", "in all"]);
    }

    #[test]
    fn the_sent_note_counts_applications_in_the_period() {
        assert_eq!(sent_note(6, Period::Month, today()), "You marked 6 applications sent in October. Every task's cost counts, not only drafting.");
        assert_eq!(sent_note(1, Period::Year, today()), "You marked 1 application sent this year. Every task's cost counts, not only drafting.");
    }

    #[test]
    fn a_day_tip_names_the_day_its_tasks_and_their_cost() {
        let on = day(2026, 10, 13);
        assert_eq!(day_tip(&DayCount { date: on, tasks: 4, spent_usd: 6.1 }), "Tue 13 Oct: 4 tasks, $6.10");
        assert_eq!(day_tip(&DayCount { date: on, tasks: 1, spent_usd: 0.8 }), "Tue 13 Oct: 1 task, $0.80");
        assert_eq!(day_tip(&DayCount { date: on, tasks: 0, spent_usd: 0.0 }), "Tue 13 Oct: no tasks");
    }

    #[test]
    fn the_rail_word_is_this_months_spend_or_no_tasks_yet() {
        assert_eq!(rail_word(&UsageReport::default(), today()), "No tasks yet");
        let report = UsageReport::new(vec![task(day(2026, 10, 3), Some("/apply"), 40.0), task(day(2026, 10, 4), None, 1.2), task(day(2026, 9, 4), None, 9.0)]);
        assert_eq!(rail_word(&report, today()), "$41.20 in Oct");
        assert_eq!(rail_word(&report, day(2026, 11, 2)), "$0.00 in Nov", "a new month starts at nothing");
    }

    #[test]
    fn the_page_divides_spend_by_the_applications_sent_in_the_period() {
        let report = UsageReport::new(vec![task(day(2026, 10, 3), Some("/apply"), 30.0), task(day(2026, 8, 3), Some("/rank"), 10.0)]);
        let pipeline = sent_on(&[day(2026, 10, 5), day(2026, 10, 20), day(2026, 9, 1)]);
        let page = usage_page(&report, &pipeline, Period::Month, today());
        assert_eq!(page.per_application, Some((15.0, 2)));
        assert_eq!(page.totals.tasks, 1);
        assert_eq!(page.commands.len(), 1);
        assert_eq!(page.models.len(), 1);
        assert_eq!(page.year.weeks.len(), 53);
        let all = usage_page(&report, &pipeline, Period::All, today());
        assert_eq!((all.totals.tasks, all.per_application), (2, Some((40.0 / 3.0, 3))));
    }

    #[test]
    fn with_nothing_sent_there_is_no_cost_per_application() {
        let report = UsageReport::new(vec![task(day(2026, 10, 3), Some("/apply"), 30.0)]);
        assert_eq!(usage_page(&report, &Pipeline::default(), Period::Month, today()).per_application, None);
        let summer = sent_on(&[day(2026, 7, 1)]);
        assert_eq!(usage_page(&report, &summer, Period::Days90, today()).per_application, None, "sent before the 90 days");
    }

    #[test]
    fn the_days_note_counts_active_days_since_the_first_task() {
        let report = UsageReport::new(vec![task(day(2026, 7, 13), None, 1.0), task(day(2026, 10, 3), None, 1.0), task(day(2026, 10, 3), None, 1.0)]);
        assert_eq!(active_days_note(&report.year(today()), today()), "2 days with a task, since 13 July");
        let older = UsageReport::new(vec![task(day(2024, 3, 4), None, 1.0)]);
        assert_eq!(active_days_note(&older.year(today()), today()), "1 day with a task, since 4 March 2024", "a day before the grid still counts");
        let both = UsageReport::new(vec![task(day(2024, 3, 4), None, 1.0), task(day(2026, 10, 3), None, 1.0)]);
        assert_eq!(active_days_note(&both.year(today()), today()), "2 days with a task, since 4 March 2024");
        assert_eq!(active_days_note(&UsageReport::default().year(today()), today()), "No task yet");
    }

    #[test]
    fn months_are_marked_over_their_first_week() {
        let marks = month_marks(&UsageReport::default().year(today()));
        let names: Vec<&str> = marks.iter().map(|(_, name)| name.as_str()).collect();
        assert_eq!(names, ["Nov", "Dec", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct"]);
        assert_eq!(marks[0].0, 1, "the week of Monday 3 November");
    }

    #[test]
    fn typed_tasks_are_other_tasks() {
        assert_eq!(command_name(&CommandRow { command: Some("/apply".into()), tasks: 1, spent_usd: 0.0 }), "/apply");
        assert_eq!(command_name(&CommandRow { command: None, tasks: 1, spent_usd: 0.0 }), "Other tasks");
    }

    #[test]
    fn a_command_from_the_report_keeps_one_slash() {
        let report = UsageReport::new(vec![task(day(2026, 10, 3), Some("/rank"), 1.0)]);
        let names: Vec<String> = report.by_command(Period::Month, today()).iter().map(command_name).collect();
        assert_eq!(names, ["/rank"]);
    }

    #[test]
    fn an_unknown_model_shows_only_its_visible_characters() {
        let row = |model: &str| ModelRow { model: model.into(), tasks: 1, tokens_in: 0, tokens_out: 0, spent_usd: 0.0 };
        assert_eq!(model_name(&row("acme\u{200b}-model\u{1b}[31m-9\u{7}")), "acme-model-9");
        assert_eq!(model_name(&row("Opus")), "Opus");
    }
}
