use egui::Ui;

use crate::settings::application::{Page, PermissionMode, Settings, index};
use crate::settings::ui::page_note;
use crate::ui_kit::{Tone, setting_radio, setting_row, setting_section, tag};

/// The two ways tool permissions go, the two rules that hold either way, then what allowing costs.
/// Ask me before each tool saves the setting the shell hands to the run desk, which turns its prompts on.
pub fn body(ui: &mut Ui, settings: &mut Settings, query: &str) {
    let &[tools, tools_note, allow, allow_help, ask, ask_help, either, questions, questions_help, rules, rules_help, cost] = index(Page::Permissions) else {
        return;
    };
    setting_section(ui, tools, Some(tools_note), query);
    let current = settings.preferences().permissions;
    for (mode, label, help) in [(PermissionMode::AllowAndNote, allow, allow_help), (PermissionMode::Ask, ask, ask_help)] {
        if setting_radio(ui, current == mode, label, help, query).clicked() {
            settings.set_permissions(mode);
        }
    }
    setting_section(ui, either, None, query);
    setting_row(ui, questions, questions_help, query, |ui| {
        tag(ui, "Always on", Tone::Plain);
    });
    setting_row(ui, rules, rules_help, query, |ui| {
        tag(ui, "This task only", Tone::Plain);
    });
    page_note(ui, cost, query);
}
