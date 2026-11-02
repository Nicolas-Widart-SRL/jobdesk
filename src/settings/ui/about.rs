use egui::{OpenUrl, Response, Ui};

use crate::settings::application::{Page, about_details, index};
use crate::settings::ui::page_note;
use crate::ui_kit::tokens::DISABLED_OPACITY;
use crate::ui_kit::{Ink, Type, enabled_when, link, rich, setting_row, setting_section, small_button};

/// Where the review of how Jobdesk signs in, and the changelog, are read. Fixed `https` addresses, never read from a file.
const REVIEW_URL: &str = "https://github.com/Nicolas-Widart-SRL/jobdesk/blob/main/docs/auth-review.md";
const CHANGELOG_URL: &str = "https://github.com/Nicolas-Widart-SRL/jobdesk/blob/main/CHANGELOG.md";
/// Why Show licences waits: the repository has no licence file to show yet.
const NO_LICENCES: &str = "Not available yet.";
const SYSTEM: &str = if cfg!(target_os = "macos") { "macOS" } else { "Windows" };

/// What About shows of this copy.
#[derive(Clone, Copy)]
pub struct AboutView<'a> {
    pub version: &'a str,
    /// Claude Code's version, once the tool check found it.
    pub claude: Option<&'a str>,
}

/// The two versions with Copy details, the links to read more, then who makes Jobdesk. No logo shows anywhere.
pub fn body(ui: &mut Ui, view: AboutView, query: &str) {
    let &[this_copy, jobdesk, jobdesk_help, claude, claude_help, read_more, signs_in, signs_in_help, changed, changed_help, licences, licences_help, maker] =
        index(Page::About)
    else {
        return;
    };
    setting_section(ui, this_copy, None, query);
    setting_row(ui, jobdesk, jobdesk_help, query, |ui| {
        if small_button(ui, "Copy details").clicked() {
            ui.ctx().copy_text(about_details(view.version, SYSTEM, std::env::consts::ARCH, view.claude));
        }
        ui.label(rich(ui, view.version, Type::MONO, Ink::Text));
    });
    setting_row(ui, claude, claude_help, query, |ui| match view.claude {
        Some(version) => {
            ui.label(rich(ui, version, Type::MONO, Ink::Text));
        }
        None => {
            ui.label(rich(ui, "Not found", Type::BODY, Ink::Muted));
        }
    });
    setting_section(ui, read_more, None, query);
    setting_row(ui, signs_in, signs_in_help, query, |ui| open(ui, "Open the review", REVIEW_URL));
    setting_row(ui, changed, changed_help, query, |ui| open(ui, "Open the changelog", CHANGELOG_URL));
    setting_row(ui, licences, licences_help, query, |ui| {
        // A link paints no disabled look of its own, so it fades here.
        ui.multiply_opacity(DISABLED_OPACITY);
        enabled_when(ui, false, NO_LICENCES, |ui| link(ui, "Show licences"));
    });
    page_note(ui, maker, query);
}

/// A link that opens `url` in the browser on click, with the address on hover.
fn open(ui: &mut Ui, text: &str, url: &'static str) {
    let response: Response = link(ui, text).on_hover_text(url);
    if response.clicked() {
        ui.ctx().open_url(OpenUrl::new_tab(url));
    }
}
