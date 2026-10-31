use std::collections::HashMap;

use egui::{ComboBox, Frame, Margin, ScrollArea, TextEdit, Ui, vec2};

use crate::runs::application::{CATALOG, CommandGroup, CommandSpec, FieldSpec, RunDesk, StartOptions, compose_line, spec_for_line};
use crate::ui_kit::tokens::{FIELD_AREA_HEIGHT, FIELD_GAP, FIELD_HEIGHT, FIELD_PADDING, GUTTER, HEADER_PADDING, INSET, ROW_GAP, SPLIT_GAP};
use crate::ui_kit::{Ink, Subtitle, Title, Tone, Type, alert, code_box, command_row, divider, muted, page_body, page_head, panel, panel_body, primary_button, rich, side_column, slip, strong};

const MODELS: [(&str, Option<&str>); 4] = [("Claude default", None), ("Opus", Some("opus")), ("Sonnet", Some("sonnet")), ("Haiku", Some("haiku"))];

/// The form's width, as on the canvas.
const FORM_WIDTH: f32 = 400.0;

pub struct Launcher {
    command: &'static CommandSpec,
    values: HashMap<&'static str, String>,
    budget: String,
    model: usize,
    reset_confirmed: bool,
}

impl Default for Launcher {
    fn default() -> Self {
        Self::for_command(&CATALOG[0])
    }
}

impl Launcher {
    fn for_command(command: &'static CommandSpec) -> Self {
        Self { command, values: HashMap::new(), budget: format!("{}.00", command.budget_usd), model: 0, reset_confirmed: false }
    }

    /// Fills the form from a line such as `/outcome followup acme`.
    pub fn prefill(&mut self, line: &str) {
        let Some(command) = spec_for_line(line) else { return };
        *self = Self::for_command(command);
        let arguments = line[command.name.len()..].trim();
        if let (Some(field), false) = (command.fields.first(), arguments.is_empty()) {
            self.values.insert(field.key, arguments.to_owned());
        }
    }

    fn value_of(&self, field: &FieldSpec) -> String {
        self.values.get(field.key).cloned().unwrap_or_else(|| field.default.to_owned())
    }

    fn line(&self) -> String {
        compose_line(self.command, |field| self.value_of(field))
    }

    /// Returns the id of a run the user just started.
    pub fn show(&mut self, ui: &mut Ui, desk: &mut RunDesk) -> Option<String> {
        let subtitle = Subtitle::Text("Every action is the unchanged slash command. Forms only build its arguments.");
        page_head(ui, Title::Slab("All commands"), Some(subtitle), |_| {});
        page_body(ui, |ui| self.body(ui, desk))
    }

    fn body(&mut self, ui: &mut Ui, desk: &mut RunDesk) -> Option<String> {
        let mut started = None;
        side_column("command_form", FORM_WIDTH + GUTTER as f32).show(ui, |ui| {
            slip(ui, |ui| {
                ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| started = self.form(ui, desk));
            });
        });
        let gap = Margin { right: (SPLIT_GAP - GUTTER as f32) as i8, ..Default::default() };
        egui::CentralPanel::default().frame(Frame::new().inner_margin(gap)).show(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| panel(ui, |ui| self.list(ui)));
        });
        started
    }

    /// Each group's heading stands on the hairline of its first row.
    fn list(&mut self, ui: &mut Ui) {
        for group in CommandGroup::ALL {
            Frame::new().inner_margin(Margin::symmetric(0, HEADER_PADDING)).show(ui, |ui| {
                ui.label(rich(ui, group.label(), Type::SECTION, Ink::Text));
            });
            for command in CATALOG.iter().filter(|command| command.group == group) {
                let on = command.name == self.command.name;
                if command_row(ui, command.name, command.summary, command.writes, on).clicked() {
                    *self = Self::for_command(command);
                }
            }
        }
    }

    fn form(&mut self, ui: &mut Ui, desk: &mut RunDesk) -> Option<String> {
        let mut started = None;
        Frame::new().inner_margin(Margin::symmetric(INSET, HEADER_PADDING)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(rich(ui, self.command.name, Type::COMMAND, Ink::Text));
        });
        divider(ui);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            muted(ui, self.command.summary);
            for field in self.command.fields {
                let mut value = self.value_of(field);
                let changed = labelled(ui, field.label, |ui, label| {
                    let edit = if field.multiline {
                        TextEdit::multiline(&mut value).desired_rows(3).min_size(vec2(0.0, FIELD_AREA_HEIGHT))
                    } else {
                        TextEdit::singleline(&mut value).min_size(vec2(0.0, FIELD_HEIGHT)).align(egui::Align2::LEFT_CENTER)
                    };
                    let edit = edit.margin(Margin::symmetric(FIELD_PADDING as i8, 4)).desired_width(f32::INFINITY);
                    ui.add(edit).labelled_by(label).changed()
                });
                if changed {
                    self.values.insert(field.key, value);
                }
            }
            ui.columns_const(|[model, budget]| {
                labelled(model, "Model", |ui, label| {
                    ui.spacing_mut().interact_size.y = FIELD_HEIGHT;
                    let width = ui.available_width();
                    ComboBox::from_id_salt("run_model").width(width).selected_text(MODELS[self.model].0).show_index(ui, &mut self.model, MODELS.len(), |index| MODELS[index].0).labelled_by(label);
                });
                labelled(budget, "Budget cap, USD", |ui, label| {
                    let edit = TextEdit::singleline(&mut self.budget).min_size(vec2(0.0, FIELD_HEIGHT)).align(egui::Align2::LEFT_CENTER);
                    ui.add(edit.margin(Margin::symmetric(FIELD_PADDING as i8, 4)).desired_width(f32::INFINITY)).labelled_by(label);
                });
            });
            let line = self.line();
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = FIELD_GAP;
                ui.label(rich(ui, "Sent to Claude", Type::SMALL, Ink::Muted));
                code_box(ui, |ui| ui.add(egui::Label::new(rich(ui, &line, Type::MONO, Ink::Text)).wrap()));
            });
            muted(ui, "Runs in your repo checkout, with your project settings and permissions.");
            if self.command.destructive {
                ui.label(rich(ui, "This deletes your data", Type::STRONG, Tone::Closed));
                muted(ui, "Files deleted from documents/ cannot be recovered by this app. The command also asks before it deletes anything.");
                ui.checkbox(&mut self.reset_confirmed, "I understand");
            }
            let budget = self.budget.trim().parse::<f64>().ok().filter(|budget| *budget > 0.0);
            let ready = budget.is_some() && (!self.command.destructive || self.reset_confirmed) && !desk.is_busy();
            if budget.is_none() {
                alert(ui, "Enter a budget above zero.");
            }
            if desk.is_busy() {
                strong(ui, "Another run is active. One run at a time keeps your files safe.");
            }
            ui.add_enabled_ui(ready, |ui| {
                if primary_button(ui, "Run").clicked() {
                    let options = StartOptions { model: MODELS[self.model].1.map(str::to_owned), budget_usd: budget.unwrap_or_default() };
                    started = desk.start(&line, &options, chrono::Local::now().naive_local()).ok();
                }
            });
        });
        started
    }
}

/// A form field: its label above the input.
fn labelled<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui, egui::Id) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = FIELD_GAP;
        let label = ui.label(rich(ui, label, Type::NOTE, Ink::Muted));
        add(ui, label.id)
    })
    .inner
}
