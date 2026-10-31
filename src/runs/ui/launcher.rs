use std::collections::HashMap;

use egui::{ComboBox, Panel, RichText, ScrollArea, TextEdit, Ui};

use crate::runs::application::{CATALOG, CommandGroup, CommandSpec, FieldSpec, RunDesk, StartOptions, compose_line, spec_for_line};
use crate::ui_kit::{Palette, faint_mono, muted, panel, panel_body, panel_header, primary_button, semibold, strong};

const MODELS: [(&str, Option<&str>); 4] = [("Claude default", None), ("Opus", Some("opus")), ("Sonnet", Some("sonnet")), ("Haiku", Some("haiku"))];

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
        let mut started = None;
        Panel::right("command_form").exact_size(420.0).resizable(false).frame(egui::Frame::new().inner_margin(egui::Margin { left: 12, ..Default::default() })).show(ui, |ui| {
            panel(ui, |ui| {
                ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| started = self.form(ui, desk));
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.list(ui));
        });
        started
    }

    fn list(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        for group in CommandGroup::ALL {
            panel(ui, |ui| {
                panel_header(ui, group.label(), None);
                for command in CATALOG.iter().filter(|command| command.group == group) {
                    let on = std::ptr::eq(command, self.command);
                    let label = format!("{:<14}{}", command.name, command.summary);
                    let text = RichText::new(label).monospace().color(if on { p.text } else { p.muted });
                    let button = egui::Button::new(text)
                        .fill(if on { p.raised } else { egui::Color32::TRANSPARENT })
                        .stroke(egui::Stroke::NONE)
                        .min_size(egui::vec2(ui.available_width(), 30.0))
                        .wrap_mode(egui::TextWrapMode::Truncate);
                    if ui.add(button).clicked() {
                        *self = Self::for_command(command);
                    }
                }
            });
            ui.add_space(8.0);
        }
    }

    fn form(&mut self, ui: &mut Ui, desk: &mut RunDesk) -> Option<String> {
        let mut started = None;
        let p = Palette::of(ui);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(self.command.name).monospace().size(15.0));
            muted(ui, self.command.summary);
            faint_mono(ui, format!("Changes: {}", self.command.writes));
            ui.add_space(6.0);
            for field in self.command.fields {
                let label = muted(ui, field.label);
                let mut value = self.value_of(field);
                let edit = if field.multiline { TextEdit::multiline(&mut value).desired_rows(5) } else { TextEdit::singleline(&mut value) };
                if ui.add(edit.desired_width(f32::INFINITY)).labelled_by(label.id).changed() {
                    self.values.insert(field.key, value);
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let label = muted(ui, "Model");
                    ComboBox::from_id_salt("run_model").selected_text(MODELS[self.model].0).show_index(ui, &mut self.model, MODELS.len(), |index| MODELS[index].0).labelled_by(label.id);
                });
                ui.vertical(|ui| {
                    let label = muted(ui, "Budget cap, USD");
                    ui.add(TextEdit::singleline(&mut self.budget).desired_width(100.0)).labelled_by(label.id);
                });
            });
            ui.add_space(6.0);
            muted(ui, "Sent to Claude");
            let line = self.line();
            ui.label(RichText::new(&line).monospace());
            ui.add_space(6.0);
            if self.command.destructive {
                ui.label(RichText::new("This deletes your data").color(p.closed).family(semibold()));
                muted(ui, "Files deleted from documents/ cannot be recovered by this app. The command also asks before it deletes anything.");
                ui.checkbox(&mut self.reset_confirmed, "I understand");
            }
            let budget = self.budget.trim().parse::<f64>().ok().filter(|budget| *budget > 0.0);
            let ready = budget.is_some() && (!self.command.destructive || self.reset_confirmed) && !desk.is_busy();
            if budget.is_none() {
                ui.label(RichText::new("Enter a budget above zero.").color(p.closed));
            }
            if desk.is_busy() {
                strong(ui, "Another run is active. One run at a time keeps your files safe.");
            }
            ui.add_enabled_ui(ready, |ui| {
                if primary_button(ui, "Run").clicked() {
                    let options = StartOptions { model: MODELS[self.model].1.map(str::to_owned), budget_usd: budget.unwrap_or_default() };
                    started = desk.start(&line, &options).ok();
                }
            });
        });
        started
    }
}
