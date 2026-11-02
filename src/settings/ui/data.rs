use egui::{Align, Label, Layout, Ui, vec2};

use crate::settings::application::{DataSizes, Page, Retention, Settings, clear_question, index, size_detail, size_short};
use crate::settings::ui::{SHOW_FOLDER, SettingsRequest, page_note};
use crate::ui_kit::tokens::{SETTING_CONTROL_GAP, SETTING_HELP_GAP, SETTING_ROW_MIN, SETTING_ROW_PADDING};
use crate::ui_kit::{
    Confirm, Ink, Type, confirm_popover, enabled_when, hit_mark, rich, setting_row, setting_section, small_button, small_danger_button, toggle_group,
};

/// How wide Clear tray decisions' popover is.
const CLEAR_WIDTH: f32 = 340.0;

const NO_DATA: &str = "Jobdesk has no data folder on this computer.";
const NOTHING_KEPT: &str = "Nothing is kept yet.";
const NO_FOLDER: &str = "Choose a folder first.";
const NO_DECISIONS: &str = "No decisions to clear.";
/// Why Clear tray decisions waits while the tray has an issue: its file could not be read, and clearing would overwrite it,
/// or its last save failed.
const UNREAD: &str = "Today's decisions are not available.";
const CLEAR_NOTE: &str = "Today deals every card again, the ones you skipped too. This cannot be undone.";

/// What the page shows of the data folder, as the shell read it.
#[derive(Clone, Copy)]
pub struct DataView<'a> {
    /// The data folder as the user reads it, if there is one.
    pub folder: Option<&'a str>,
    pub sizes: Option<DataSizes>,
    /// How many decisions the open folder's tray holds. None with no folder open, or a tray that could not be read or saved.
    pub decisions: Option<usize>,
    /// Whether a job search folder is open, which decides why Clear tray decisions waits.
    pub workspace_open: bool,
}

/// The data folder and its size, how long run logs stay, and Clear tray decisions, which asks first.
/// `clearing` holds the popover's state. Returns what the user asked of the shell.
pub fn body(ui: &mut Ui, settings: &mut Settings, view: DataView, clearing: &mut Option<bool>, query: &str) -> Option<SettingsRequest> {
    let &[data, data_note, folder, size, logs, keep, keep_help, tray, clear, clear_help, footer] = index(Page::Data) else { return None };
    let mut request = None;
    setting_section(ui, data, Some(data_note), query);
    if folder_row(ui, folder, view.folder, query) {
        request = Some(SettingsRequest::ShowDataFolder);
    }
    let detail = match view.sizes {
        Some(sizes) => format!("Run logs {}, tray decisions {}.", size_detail(sizes.run_logs), size_detail(sizes.decisions)),
        None if view.folder.is_some() => NOTHING_KEPT.to_owned(),
        None => NO_DATA.to_owned(),
    };
    setting_row(ui, size, &detail, query, |ui| {
        if let Some(sizes) = view.sizes {
            ui.label(rich(ui, size_short(sizes.total()), Type::STRONG, Ink::Text));
        }
    });
    setting_section(ui, logs, None, query);
    setting_row(ui, keep, keep_help, query, |ui| {
        let options = Retention::ALL.map(|retention| (retention, retention.label()));
        if let Some(retention) = toggle_group(ui, Some(settings.preferences().retention), &options) {
            settings.set_retention(retention);
        }
    });
    setting_section(ui, tray, None, query);
    let count = view.decisions.unwrap_or(0);
    let why = match view.decisions {
        _ if !view.workspace_open => NO_FOLDER,
        None => UNREAD,
        Some(_) => NO_DECISIONS,
    };
    let enabled = count > 0;
    let mut button = None;
    setting_row(ui, clear, clear_help, query, |ui| {
        button = Some(enabled_when(ui, enabled, why, |ui| small_danger_button(ui, clear)));
    });
    if !enabled {
        *clearing = None;
    }
    if let Some(button) = button {
        if button.clicked() {
            *clearing = Some(true);
        }
        // Keep them is the default and takes the focus, and Esc or a click elsewhere keeps them too.
        let question = clear_question(count);
        let words = Confirm { question: &question, note: CLEAR_NOTE, keep: "Keep them", act: "Clear decisions", esc_note: "Esc keeps them." };
        if confirm_popover(&button, clearing, &words, CLEAR_WIDTH) {
            request = Some(SettingsRequest::ClearDecisions);
        }
    }
    page_note(ui, footer, query);
    request
}

/// The data folder's row: its label over the path in mono, and Show in Finder at the right. The path is not a help
/// line, so the row is laid out here. Returns whether Show in Finder was pressed.
fn folder_row(ui: &mut Ui, label: &str, folder: Option<&str>, query: &str) -> bool {
    let mut show = false;
    let height = SETTING_ROW_MIN - SETTING_ROW_PADDING * 2.0;
    ui.add_space(SETTING_ROW_PADDING);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::right_to_left(Align::Center), |ui| {
        ui.set_min_height(height);
        show = enabled_when(ui, folder.is_some(), NO_DATA, |ui| small_button(ui, SHOW_FOLDER)).clicked();
        ui.add_space(SETTING_CONTROL_GAP);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = SETTING_HELP_GAP;
            hit_mark(ui, label, Type::STRONG, Ink::Text, query);
            match folder {
                Some(folder) => ui.add(Label::new(rich(ui, folder, Type::MONO, Ink::Text)).truncate()),
                None => ui.add(Label::new(rich(ui, NO_DATA, Type::NOTE, Ink::Muted))),
            };
        });
    });
    ui.add_space(SETTING_ROW_PADDING);
    show
}
