use std::collections::HashMap;

use egui::{ComboBox, Frame, Key, Margin, Modifiers, ScrollArea, TextEdit, Ui, vec2};

use crate::runs::application::{
    CAP_READING, CATALOG, CapCheck, CommandGroup, CommandSpec, FieldSpec, RunDefaults, RunDesk, StartOptions, cap_line, cap_reason, compose_line,
    spec_for_line, start_values,
};
use crate::shared_kernel::ModelChoice;
use crate::ui_kit::tokens::{FIELD_AREA_HEIGHT, FIELD_GAP, FIELD_HEIGHT, FIELD_PADDING, HEADER_PADDING, INSET, ROW_GAP};
use crate::ui_kit::{
    Ink, Subtitle, Title, Tone, Type, alert, code_box, command_chord, command_row, divider, enabled_when, group_title, hand, link, muted, page_body, page_head,
    panel, panel_body, primary_shortcut_button, rich, slip, split, strong,
};

/// The keys that start the run.
const RUN_KEYS: [&str; 2] = command_chord("Enter");

/// The form's width, as on the canvas.
const FORM_WIDTH: f32 = 400.0;

const NO_BUDGET: &str = "Enter a budget above zero.";
const BUSY: &str = "Another run is active. One run at a time keeps your files safe.";
const TICK_FIRST: &str = "Tick \"I understand\" first.";

/// What a new task starts with, and whether the monthly cap lets one start.
pub struct LaunchRules<'a> {
    pub defaults: &'a RunDefaults,
    pub cap: &'a CapCheck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherRequest {
    /// The id of the run the user just started.
    Started(String),
    /// Open Settings on Run defaults, where the cap is.
    OpenRunDefaults,
}

/// The model and the budget hold only what the user changed. Untouched, they show what the run defaults give the command.
pub struct Launcher {
    command: &'static CommandSpec,
    values: HashMap<&'static str, String>,
    budget: Option<String>,
    model: Option<ModelChoice>,
    reset_confirmed: bool,
}

impl Default for Launcher {
    fn default() -> Self {
        Self::for_command(&CATALOG[0])
    }
}

impl Launcher {
    fn for_command(command: &'static CommandSpec) -> Self {
        Self { command, values: HashMap::new(), budget: None, model: None, reset_confirmed: false }
    }

    /// Fills the form from a line such as `/outcome followup acme`. A field with a flag, such as `/setup --section goals`,
    /// takes only the value after its flag, because the line puts the flag back.
    pub fn prefill(&mut self, line: &str) {
        let Some(command) = spec_for_line(line) else { return };
        *self = Self::for_command(command);
        let arguments = line[command.name.len()..].trim();
        if let (Some(field), false) = (command.fields.first(), arguments.is_empty()) {
            let after_flag = field.flag.and_then(|flag| arguments.strip_prefix(flag)).filter(|rest| rest.starts_with(char::is_whitespace));
            self.values.insert(field.key, after_flag.map_or(arguments, str::trim).to_owned());
        }
    }

    fn value_of(&self, field: &FieldSpec) -> String {
        self.values.get(field.key).cloned().unwrap_or_else(|| field.default.to_owned())
    }

    fn line(&self) -> String {
        compose_line(self.command, |field| self.value_of(field))
    }

    /// Returns the run the user just started, or the way to the cap.
    pub fn show(&mut self, ui: &mut Ui, desk: &mut RunDesk, rules: &LaunchRules) -> Option<LauncherRequest> {
        // Taken before any field runs, so a multiline field never reads the keys as a new line, even while Run waits.
        let run_keys = ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter));
        let subtitle = Subtitle::Text("Pick a command. Nothing runs until you press Run.");
        page_head(ui, Title::Slab("New task"), Some(subtitle), |_| {});
        page_body(ui, |ui| self.body(ui, desk, rules, run_keys))
    }

    fn body(&mut self, ui: &mut Ui, desk: &mut RunDesk, rules: &LaunchRules, run_keys: bool) -> Option<LauncherRequest> {
        split(ui, FORM_WIDTH, |main, side| {
            ScrollArea::vertical().auto_shrink([false, false]).show(main, |ui| panel(ui, |ui| self.list(ui)));
            slip(side, |ui| ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| self.form(ui, desk, rules, run_keys)).inner)
        })
    }

    /// Each group's heading stands on the hairline of its first row.
    fn list(&mut self, ui: &mut Ui) {
        for group in CommandGroup::ALL {
            group_title(ui, group.label());
            for command in CATALOG.iter().filter(|command| command.group == group) {
                let on = command.name == self.command.name;
                if command_row(ui, command.name, command.summary, command.writes, on).clicked() {
                    *self = Self::for_command(command);
                }
            }
        }
    }

    /// `run_keys` is Cmd or Ctrl Enter, which starts the run only when the button could.
    /// A reached cap holds back only a new task: a running one, and replies to a live session, go on elsewhere.
    fn form(&mut self, ui: &mut Ui, desk: &mut RunDesk, rules: &LaunchRules, run_keys: bool) -> Option<LauncherRequest> {
        let mut asked = None;
        let start = start_values(rules.defaults, self.command);
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
            let mut model = self.model.unwrap_or(start.model);
            let mut budget_text = self.budget.clone().unwrap_or_else(|| start.budget.field_text());
            ui.columns_const(|[model_column, budget_column]| {
                labelled(model_column, "Model", |ui, label| {
                    ui.spacing_mut().interact_size.y = FIELD_HEIGHT;
                    let width = ui.available_width();
                    let mut at = ModelChoice::ALL.iter().position(|&choice| choice == model).unwrap_or(0);
                    let combo = ComboBox::from_id_salt("run_model").width(width).selected_text(model.label()).show_index(
                        ui,
                        &mut at,
                        ModelChoice::ALL.len(),
                        |index| ModelChoice::ALL[index].label(),
                    );
                    if hand(combo).labelled_by(label).changed() {
                        model = ModelChoice::ALL[at];
                        self.model = Some(model);
                    }
                });
                labelled(budget_column, "Spend at most, USD", |ui, label| {
                    let edit = TextEdit::singleline(&mut budget_text).min_size(vec2(0.0, FIELD_HEIGHT)).align(egui::Align2::LEFT_CENTER);
                    if ui.add(edit.margin(Margin::symmetric(FIELD_PADDING as i8, 4)).desired_width(f32::INFINITY)).labelled_by(label).changed() {
                        self.budget = Some(budget_text.clone());
                    }
                });
            });
            let line = self.line();
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = FIELD_GAP;
                ui.label(rich(ui, "Claude will run", Type::SMALL, Ink::Muted));
                code_box(ui, |ui| ui.add(egui::Label::new(rich(ui, &line, Type::MONO, Ink::Text)).wrap()));
            });
            muted(ui, "Runs in your job search folder.");
            if self.command.destructive {
                ui.label(rich(ui, "This deletes your data", Type::STRONG, Tone::Closed));
                muted(ui, "Files deleted from documents/ cannot be recovered by this app. The command also asks before it deletes anything.");
                hand(ui.checkbox(&mut self.reset_confirmed, "I understand"));
            }
            let budget = budget_text.trim().parse::<f64>().ok().filter(|budget| *budget > 0.0);
            let busy = desk.is_busy();
            let reached = match rules.cap {
                CapCheck::Reached(reached) => Some(reached),
                CapCheck::Open | CapCheck::Reading => None,
            };
            let ready = budget.is_some() && !busy && *rules.cap == CapCheck::Open && (!self.command.destructive || self.reset_confirmed);
            if budget.is_none() {
                alert(ui, NO_BUDGET);
            }
            if busy {
                strong(ui, BUSY);
            }
            if let Some(reached) = reached {
                strong(ui, cap_line(reached));
            }
            let why = match rules.cap {
                _ if budget.is_none() => NO_BUDGET.to_owned(),
                _ if busy => BUSY.to_owned(),
                CapCheck::Reached(reached) => cap_reason(reached),
                CapCheck::Reading => CAP_READING.to_owned(),
                CapCheck::Open => TICK_FIRST.to_owned(),
            };
            let mut run = |ui: &mut Ui| {
                let mut started = None;
                enabled_when(ui, ready, &why, |ui| {
                    let run = primary_shortcut_button(ui, "Run", &RUN_KEYS);
                    if run.clicked() || (ready && run_keys) {
                        let options = StartOptions { model: model.alias().map(str::to_owned), budget_usd: budget.unwrap_or_default() };
                        started = desk.start(&line, &options, chrono::Local::now().naive_local()).ok();
                    }
                    run
                });
                started.map(LauncherRequest::Started)
            };
            // The way to the cap stands beside Run only while the cap holds it back, so the usual form keeps its look.
            asked = if reached.is_some() {
                ui.horizontal(|ui| {
                    let started = run(ui);
                    if link(ui, "Change the cap in Settings").clicked() { Some(LauncherRequest::OpenRunDefaults) } else { started }
                })
                .inner
            } else {
                run(ui)
            };
        });
        asked
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A flag in the line is the field's own, so it is not typed into the field a second time.
    #[test]
    fn a_flagged_line_fills_only_the_value() {
        let mut launcher = Launcher::default();
        launcher.prefill("/setup --section experience");
        assert_eq!(launcher.line(), "/setup --section experience");
        launcher.prefill("/outcome followup acme");
        assert_eq!(launcher.line(), "/outcome followup acme");
        launcher.prefill("/setup");
        assert_eq!(launcher.line(), "/setup");
    }
}
