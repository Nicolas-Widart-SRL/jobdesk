use std::cell::Cell;

use chrono::NaiveDate;
use egui::{Align, Label, Layout, Rect, Ui, UiBuilder, pos2, vec2};

use crate::settings::application::{
    Page, Period, UsagePage, UsageReport, UsageState, YearOfDays, active_days_note, command_name, day_tip, heat_step, index, money, month_marks, percent,
    period_label, period_words, sent_note, tasks_note, tokens, usage_page,
};
use crate::settings::ui::SettingsRequest;
use crate::ui_kit::tokens::{BAR_HEIGHT, EMPTY_GAP, EMPTY_WIDTH, FADED_OPACITY, SKELETON_LINE, TABLE_HEAD_HEIGHT};
use crate::ui_kit::{
    CellRule, DayHeatmap, Figure, HeatDay, Ink, Type, bar, button, card_skeleton, day_heatmap, day_heatmap_loading, divider, enabled_when, figure_band,
    figure_band_loading, hit_mark, link, primary_command_action, rich, score, skeleton_lines, table_cell, table_head, toggle_group,
};

/// The lead while the run logs are read, in place of the page's own.
pub const READING: &str = "Reading your run logs…";

const EMPTY_TITLE: &str = "No tasks yet";
const EMPTY_BODY: &str = "Usage counts the tasks Jobdesk runs, from its own logs. Once your first task ends, its cost, tokens and model show here.";
const NO_FOLDER: &str = "Choose a folder first.";
const YEAR_NOTE: &str = "Last 12 months, one square a day";
const LEGEND: [&str; 4] = ["None", "1 task", "2 or 3 tasks", "4 or more"];
const CACHE_NOTE: &str = "of input read from cache, which costs less";

/// The year's weeks, as `runs` counts them.
const YEAR_WEEKS: usize = 53;
/// The room above the cost per application sent, its gap between parts, and the room under it before its rule.
const PER_LINE: (f32, f32, f32) = (16.0, 14.0, 12.0);
/// The room above a section, and between its heading and what it holds.
const SECTION: (f32, f32) = (20.0, 8.0);
/// The gap between By command and By model, and the least width of each, below which they stack.
const TABLES_GAP: f32 = 28.0;
const TABLE_LEAST: f32 = 390.0;
const ROW_HEIGHT: f32 = 37.0;
const SHARE_WIDTH: f32 = 70.0;
/// The room above the empty state, between its sentence and its actions, and under it before its rule.
const EMPTY_ROOM: (f32, f32, f32) = (10.0, 10.0, 18.0);
/// The loading line in place of the cost per application sent: its blocks' widths and heights.
const PER_SKELETON: [(f32, f32); 3] = [(140.0, SKELETON_LINE), (60.0, 18.0), (380.0, SKELETON_LINE)];
const COMMAND_SKELETON: [f32; 4] = [300.0, 260.0, 280.0, 220.0];
const MODEL_SKELETON: [f32; 3] = [280.0, 240.0, 260.0];

/// The period toggles in the head. The month is named, so they read "October", "90 days", "This year", "All time".
pub fn period_toggles(ui: &mut Ui, period: &Cell<Period>, today: NaiveDate) {
    let labels = Period::ALL.map(|choice| period_label(choice, today));
    let options: Vec<(Period, &str)> = Period::ALL.iter().zip(&labels).map(|(&choice, label)| (choice, label.as_str())).collect();
    if let Some(picked) = toggle_group(ui, Some(period.get()), &options) {
        period.set(picked);
    }
}

/// The body of Usage statistics as things stand. Returns what the user asked of the shell.
pub fn body(ui: &mut Ui, state: UsageState, period: Period, today: NaiveDate, query: &str) -> Option<SettingsRequest> {
    match state {
        UsageState::NoFolder => empty(ui, false, &UsageReport::default(), today),
        UsageState::Reading => {
            loading(ui);
            None
        }
        UsageState::Read { report, today, .. } if report.is_empty() => empty(ui, true, report, today),
        UsageState::Read { report, pipeline, today } => full(ui, &usage_page(report, pipeline, period, today), period, today, query),
    }
}

/// The figures, the cost per application sent, the year of days, then By command beside By model.
fn full(ui: &mut Ui, page: &UsagePage, period: Period, today: NaiveDate, query: &str) -> Option<SettingsRequest> {
    let &[spent, tokens_label, cache, tasks, per_sent, every_day, by_command, by_model] = index(Page::Usage) else { return None };
    let totals = &page.totals;
    let figures = [
        // The monthly cap goes here once it is a setting: a `slip_bar` of Spent against it, with the cap in the note.
        Figure { label: spent, figure: money(totals.spent_usd), note: period_words(period, today), bar: None },
        Figure { label: tokens_label, figure: tokens(totals.tokens_in), note: format!("in, and {} out", tokens(totals.tokens_out)), bar: None },
        Figure { label: cache, figure: percent(totals.cache_share().unwrap_or(0.0)), note: CACHE_NOTE.to_owned(), bar: None },
        Figure { label: tasks, figure: totals.tasks.to_string(), note: tasks_note(totals), bar: None },
    ];
    figure_band(ui, &figures);
    let mut request = None;
    if let Some((cost, sent)) = page.per_application {
        let (above, gap, under) = PER_LINE;
        ui.add_space(above);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            hit_mark(ui, per_sent, Type::STRONG, Ink::Text, query);
            score(ui, money(cost));
            // The link goes in first, at the right, so the sentence wraps in the room it leaves.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if link(ui, "Open Applications").clicked() {
                    request = Some(SettingsRequest::OpenApplications);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(Label::new(rich(ui, sent_note(sent, period, today), Type::BODY, Ink::Muted)).wrap());
                });
            });
        });
        ui.add_space(under);
        divider(ui);
    }
    section_heading(ui, every_day, query);
    year_of_days(ui, &page.year, today, false);
    let share = |spent: f64| if totals.spent_usd > 0.0 { (spent / totals.spent_usd) as f32 } else { 0.0 };
    let commands = |ui: &mut Ui| {
        section_heading(ui, by_command, query);
        let columns = [("Command", None, false), ("Tasks", Some(70.0), true), ("Spent", Some(90.0), true), ("Share", Some(100.0), false)];
        table(ui, &columns, page.commands.len(), |ui, row, column| {
            let row = &page.commands[row];
            match column {
                0 => cell_text(ui, command_name(row), Type::MONO),
                1 => cell_text(ui, row.tasks.to_string(), Type::BODY),
                2 => cell_text(ui, money(row.spent_usd), Type::BODY),
                _ => {
                    ui.allocate_ui(vec2(SHARE_WIDTH, BAR_HEIGHT), |ui| bar(ui, share(row.spent_usd), Ink::Text));
                }
            }
        });
    };
    let models = |ui: &mut Ui| {
        section_heading(ui, by_model, query);
        let columns = [("Model", None, false), ("Tasks", Some(64.0), true), ("In", Some(70.0), true), ("Out", Some(70.0), true), ("Spent", Some(84.0), true)];
        table(ui, &columns, page.models.len(), |ui, row, column| {
            let row = &page.models[row];
            match column {
                0 => cell_text(ui, row.model.as_str(), Type::BODY),
                1 => cell_text(ui, row.tasks.to_string(), Type::BODY),
                2 => cell_text(ui, tokens(row.tokens_in), Type::BODY),
                3 => cell_text(ui, tokens(row.tokens_out), Type::BODY),
                _ => cell_text(ui, money(row.spent_usd), Type::BODY),
            }
        });
    };
    side_by_side(ui, commands, models);
    request
}

/// No task yet, or no folder to read: what Usage counts, the two ways to a first task, then the blank year, faded.
/// Without a folder both actions wait for one.
fn empty(ui: &mut Ui, ready: bool, report: &UsageReport, today: NaiveDate) -> Option<SettingsRequest> {
    let (above, before_actions, under) = EMPTY_ROOM;
    let mut request = None;
    ui.add_space(above);
    ui.scope(|ui| {
        ui.set_max_width(EMPTY_WIDTH);
        ui.spacing_mut().item_spacing.y = EMPTY_GAP;
        ui.add(Label::new(rich(ui, EMPTY_TITLE, Type::SLIP_TITLE, Ink::Text)));
        ui.add(Label::new(rich(ui, EMPTY_BODY, Type::TAKE, Ink::Muted)).wrap());
        ui.add_space(before_actions - EMPTY_GAP);
        ui.horizontal(|ui| {
            enabled_when(ui, ready, NO_FOLDER, |ui| primary_command_action(ui, "Find new jobs…", "/scrape"));
            if enabled_when(ui, ready, NO_FOLDER, |ui| button(ui, "New task…")).clicked() {
                request = Some(SettingsRequest::NewTask);
            }
        });
    });
    ui.add_space(under);
    divider(ui);
    let &[.., every_day, _, _] = index(Page::Usage) else { return request };
    ui.scope(|ui| {
        ui.multiply_opacity(FADED_OPACITY);
        ui.add_space(SECTION.0 - SECTION.1);
        ui.add(Label::new(rich(ui, every_day, Type::TABLE_HEAD, Ink::Muted)));
        ui.add_space(SECTION.1);
    });
    year_of_days(ui, &report.year(today), today, true);
    request
}

/// Grey blocks shaped like the page, while the run logs are read.
fn loading(ui: &mut Ui) {
    let &[.., every_day, by_command, by_model] = index(Page::Usage) else { return };
    figure_band_loading(ui, 4);
    let (above, gap, under) = PER_LINE;
    ui.add_space(above);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (width, height) in PER_SKELETON {
            card_skeleton(ui, width, height);
        }
    });
    ui.add_space(under);
    divider(ui);
    section_heading(ui, every_day, "");
    day_heatmap_loading(ui, YEAR_WEEKS);
    let commands = |ui: &mut Ui| {
        section_heading(ui, by_command, "");
        skeleton_lines(ui, &COMMAND_SKELETON, card_skeleton);
    };
    let models = |ui: &mut Ui| {
        section_heading(ui, by_model, "");
        skeleton_lines(ui, &MODEL_SKELETON, card_skeleton);
    };
    side_by_side(ui, commands, models);
}

fn section_heading(ui: &mut Ui, title: &str, query: &str) {
    ui.add_space(SECTION.0);
    hit_mark(ui, title, Type::SECTION, Ink::Text, query);
    ui.add_space(SECTION.1);
}

/// `left` beside `right` in two equal columns, or `right` under `left` when the body is narrower than both.
fn side_by_side(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let room = ui.available_width();
    if room < TABLE_LEAST * 2.0 + TABLES_GAP {
        left(ui);
        right(ui);
        return;
    }
    let width = (room - TABLES_GAP) / 2.0;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = TABLES_GAP;
        ui.vertical(|ui| {
            ui.set_width(width);
            left(ui);
        });
        ui.vertical(|ui| {
            ui.set_width(width);
            right(ui);
        });
    });
}

/// The last 12 months, a square a day, with each day's tasks and spend on hover.
fn year_of_days(ui: &mut Ui, year: &YearOfDays, today: NaiveDate, faded: bool) {
    let weeks: Vec<[Option<HeatDay>; 7]> =
        year.weeks.iter().map(|days| days.map(|day| day.map(|day| HeatDay { step: heat_step(day.tasks), tip: day_tip(&day) }))).collect();
    let marks = month_marks(year);
    let months: Vec<(usize, &str)> = marks.iter().map(|(week, name)| (*week, name.as_str())).collect();
    let days_note = active_days_note(year, today);
    let notes = [YEAR_NOTE, days_note.as_str()];
    day_heatmap(ui, &DayHeatmap { weeks: &weeks, today: Some(year.today_at), months: &months, notes: &notes, legend: LEGEND, faded });
}

/// A plain table: each column's head, its width (`None` shares what the others leave) and whether it reads from the right.
/// `cell(ui, row, column)` fills one cell.
fn table(ui: &mut Ui, columns: &[(&str, Option<f32>, bool)], rows: usize, mut cell: impl FnMut(&mut Ui, usize, usize)) {
    let room = ui.available_width();
    let fixed: f32 = columns.iter().filter_map(|(_, width, _)| *width).sum();
    let widths: Vec<f32> = columns.iter().map(|(_, width, _)| width.unwrap_or((room - fixed).max(0.0))).collect();
    let line = |ui: &mut Ui, height: f32, rule: CellRule, fill: &mut dyn FnMut(&mut Ui, usize)| {
        let row = Rect::from_min_size(ui.cursor().min, vec2(room, height));
        let mut left = row.left();
        for (column, &width) in widths.iter().enumerate() {
            let rect = Rect::from_min_max(pos2(left, row.top()), pos2(left + width, row.bottom()));
            left += width;
            let mut part = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)));
            part.shrink_clip_rect(rect);
            table_cell(&mut part, rule, |ui| {
                if columns[column].2 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| fill(ui, column));
                } else {
                    fill(ui, column);
                }
            });
        }
        ui.advance_cursor_after_rect(row);
    };
    line(ui, TABLE_HEAD_HEIGHT, CellRule::Head, &mut |ui, column| {
        table_head(ui, columns[column].0);
    });
    for row in 0..rows {
        let rule = if row + 1 == rows { CellRule::Last } else { CellRule::Row };
        line(ui, ROW_HEIGHT, rule, &mut |ui, column| cell(ui, row, column));
    }
}

fn cell_text(ui: &mut Ui, text: impl Into<String>, style: Type) {
    ui.add(Label::new(rich(ui, text, style, Ink::Text)).truncate());
}
