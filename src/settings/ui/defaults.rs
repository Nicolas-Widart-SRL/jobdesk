use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use egui::{ComboBox, Label, Popup, Response, ScrollArea, Ui, vec2};

use crate::settings::application::{Dollars, Page, Period, Settings, UsageState, addable_commands, index, money, period_label};
use crate::settings::ui::usage::table;
use crate::shared_kernel::ModelChoice;
use crate::ui_kit::tokens::{BAR_HEIGHT, CHECK_BOX, FIELD_HEIGHT, ITEM_GAP, SETTING_CHECK_GAP, SETTING_ROW_PADDING};
use crate::ui_kit::{
    Ink, Tone, Type, amount_field, enabled_when, hand, rich, select_row, setting_check_amount, setting_row, setting_section, slip_bar, small_button,
};

/// How wide every task's model box and budget field are, and the same in a command's row.
const MODEL_WIDTH: f32 = 200.0;
const BUDGET_WIDTH: f32 = 120.0;
const ROW_MODEL_WIDTH: f32 = 170.0;
const ROW_BUDGET_WIDTH: f32 = 110.0;
/// The table's columns: the command, its model, its budget, then Remove at the right.
const COLUMNS: [(&str, Option<f32>, bool); 4] =
    [("Command", Some(145.0), false), ("Model", Some(347.0), false), ("Spend at most", None, false), ("", Some(110.0), true)];
const ROW_HEIGHT: f32 = 44.0;
/// The room above Add a command, and the most its list grows before it scrolls.
const ADD_GAP: f32 = 8.0;
const ADD_LIST_HEIGHT: f32 = 320.0;
/// How long the bar of this month's spend is.
const SPEND_BAR_WIDTH: f32 = 220.0;

const SUGGESTED: &str = "Suggested";
const ALL_LISTED: &str = "Every command has its own row.";

/// The amounts as typed, kept while their field has the focus, so a half-typed amount stays as it is.
/// An amount saves once it reads as one, and a field that loses the focus shows what is saved.
#[derive(Default)]
pub struct Edits {
    budget: Option<String>,
    cap: Option<String>,
    commands: HashMap<String, String>,
}

/// Every task's model and budget, the commands with their own, then the monthly cap with this month's spend against it.
pub fn body(ui: &mut Ui, settings: &mut Settings, edits: &mut Edits, usage: UsageState, query: &str) {
    let &[every, every_note, model, model_help, budget, budget_help, per, per_note, cap_title, before, after, cap_help] = index(Page::Defaults) else {
        return;
    };
    setting_section(ui, every, Some(every_note), query);
    setting_row(ui, model, model_help, query, |ui| {
        if let Some(choice) = model_box(ui, "settings_model", settings.preferences().run_defaults.model, MODEL_WIDTH, model) {
            settings.set_model(choice);
        }
    });
    setting_row(ui, budget, budget_help, query, |ui| {
        let saved = settings.preferences().run_defaults.budget.map(Dollars::field_text).unwrap_or_default();
        let mut text = edits.budget.clone().unwrap_or(saved);
        let field = amount_field(ui, &mut text, BUDGET_WIDTH, SUGGESTED);
        ui.ctx().accesskit_node_builder(field.id, |node| node.set_label(budget));
        // Emptied, every task starts at its command's suggestion again.
        match typed(&mut edits.budget, text, &field).as_deref().map(str::trim) {
            Some("") => settings.set_budget(None),
            Some(text) => {
                if let Some(amount) = Dollars::parse(text) {
                    settings.set_budget(Some(amount));
                }
            }
            None => {}
        }
    });
    setting_section(ui, per, Some(per_note), query);
    commands(ui, settings, edits);
    setting_section(ui, cap_title, None, query);
    cap(ui, settings, edits, usage, [before, after, cap_help], query);
}

/// The table of commands with their own model and budget, then Add a command.
fn commands(ui: &mut Ui, settings: &mut Settings, edits: &mut Edits) {
    let rows = settings.preferences().run_defaults.commands.clone();
    let mut removed = None;
    if !rows.is_empty() {
        table(ui, &COLUMNS, rows.len(), ROW_HEIGHT, |ui, at, column| {
            let row = &rows[at];
            // A row's controls keep their ids when a row above them goes.
            ui.push_id(&row.command, |ui| match column {
                // The name is matched against the catalog when the file is read, and shows as plain text.
                0 => {
                    ui.add(Label::new(rich(ui, &row.command, Type::MONO, Ink::Text)).truncate());
                }
                1 => {
                    let name = format!("Model for {}", row.command);
                    if let Some(choice) = model_box(ui, "settings_command_model", row.model, ROW_MODEL_WIDTH, &name) {
                        settings.set_command_model(&row.command, choice);
                    }
                }
                2 => {
                    let mut edit = edits.commands.remove(&row.command);
                    let mut text = edit.clone().unwrap_or_else(|| row.budget.field_text());
                    let field = amount_field(ui, &mut text, ROW_BUDGET_WIDTH, "");
                    let name = format!("Spend at most for {}", row.command);
                    ui.ctx().accesskit_node_builder(field.id, |node| node.set_label(name));
                    // A row always has a budget, so an amount that does not read as one saves nothing.
                    if let Some(amount) = typed(&mut edit, text, &field).as_deref().and_then(Dollars::parse) {
                        settings.set_command_budget(&row.command, amount);
                    }
                    if let Some(edit) = edit {
                        edits.commands.insert(row.command.clone(), edit);
                    }
                }
                _ => {
                    let remove = small_button(ui, "Remove");
                    let name = format!("Remove {}", row.command);
                    ui.ctx().accesskit_node_builder(remove.id, |node| node.set_label(name));
                    if remove.clicked() {
                        removed = Some(row.command.clone());
                    }
                }
            });
        });
    }
    if let Some(command) = removed {
        edits.commands.remove(&command);
        settings.remove_command(&command);
    }
    ui.add_space(ADD_GAP);
    let addable = addable_commands(&settings.preferences().run_defaults);
    let add = ui.horizontal(|ui| enabled_when(ui, !addable.is_empty(), ALL_LISTED, |ui| small_button(ui, "Add a command"))).inner;
    let mut picked = None;
    Popup::menu(&add).show(|ui| {
        ScrollArea::vertical().max_height(ADD_LIST_HEIGHT).show(ui, |ui| {
            for spec in &addable {
                if select_row(ui, spec.name, Type::MONO, false).clicked() {
                    picked = Some(*spec);
                }
            }
        });
    });
    if let Some(spec) = picked {
        settings.add_command(spec);
    }
}

/// The cap's checkbox with its amount, then this month's spend against it once the run logs are read.
fn cap(ui: &mut Ui, settings: &mut Settings, edits: &mut Edits, usage: UsageState, [before, after, help]: [&str; 3], query: &str) {
    let saved = settings.preferences().cap;
    let mut on = saved.on;
    let mut text = edits.cap.clone().unwrap_or_else(|| saved.amount.map(Dollars::field_text).unwrap_or_default());
    let (check, field) = setting_check_amount(ui, &mut on, &mut text, before, after, help, query);
    if check.changed() {
        settings.set_cap_on(on);
    }
    // Emptied, the cap has no amount and holds nothing back.
    match typed(&mut edits.cap, text, &field).as_deref().map(str::trim) {
        Some("") => settings.set_cap_amount(None),
        Some(text) => {
            if let Some(amount) = Dollars::parse(text) {
                settings.set_cap_amount(Some(amount));
            }
        }
        None => {}
    }
    let cap = settings.preferences().cap;
    let (Some(amount), UsageState::Read { report, today, .. }) = (cap.active(), usage) else { return };
    // The same sum the launcher checks, so the bar fills when Run stops.
    let spent = report.totals(Period::Month, today).spent_usd;
    let ink = if cap.reached(spent, today).is_some() { Tone::Closed.into() } else { Ink::Text };
    let filled = (spent / amount.usd()).clamp(0.0, 1.0) as f32;
    let words = format!("{} of {} in {}", money(spent), money(amount.usd()), period_label(Period::Month, today));
    // The line belongs to the cap's row, so it takes back the row's bottom padding and sits under the help.
    ui.add_space(ITEM_GAP - SETTING_ROW_PADDING);
    ui.horizontal(|ui| {
        ui.add_space(CHECK_BOX + SETTING_CHECK_GAP);
        ui.spacing_mut().item_spacing.x = ITEM_GAP * 2.0;
        ui.allocate_ui(vec2(SPEND_BAR_WIDTH, BAR_HEIGHT), |ui| slip_bar(ui, filled, ink));
        ui.label(rich(ui, words, Type::NOTE, Ink::Muted));
    });
    ui.add_space(SETTING_ROW_PADDING);
}

/// A combo box of the launcher's models, named `name` for a screen reader. Returns the model picked this frame.
fn model_box(ui: &mut Ui, id: impl Hash + Debug, current: ModelChoice, width: f32, name: &str) -> Option<ModelChoice> {
    ui.spacing_mut().interact_size.y = FIELD_HEIGHT;
    let mut at = ModelChoice::ALL.iter().position(|&choice| choice == current).unwrap_or(0);
    let combo = ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(current.label())
        .show_index(ui, &mut at, ModelChoice::ALL.len(), |index| ModelChoice::ALL[index].label());
    // Its label is the row's own text, so the box is named here.
    let name = name.to_owned();
    ui.ctx().accesskit_node_builder(combo.id, |node| node.set_label(name));
    hand(combo).changed().then(|| ModelChoice::ALL[at])
}

/// Keeps a field's `text` in `edit` while the field has the focus, and lets it go once the focus leaves, so the field
/// shows what is saved again. Returns the text on a frame it changed.
fn typed(edit: &mut Option<String>, text: String, field: &Response) -> Option<String> {
    if !field.has_focus() {
        *edit = None;
    } else if field.changed() {
        *edit = Some(text.clone());
    }
    field.changed().then_some(text)
}
