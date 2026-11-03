use egui::{OpenUrl, Response, Ui};

use crate::settings::application::{ClaudeVersion, Page, about_details, index};
use crate::settings::ui::page_note;
use crate::ui_kit::tokens::DISABLED_OPACITY;
use crate::ui_kit::{Ink, Type, enabled_when, link, rich, setting_row, setting_section, small_button};

/// Where the changelog is read. A fixed `https` address, never read from a file.
const CHANGELOG_URL: &str = "https://github.com/Nicolas-Widart-SRL/jobdesk/blob/main/CHANGELOG.md";
/// Why Show licences waits: the repository has no licence file to show yet.
const NO_LICENCES: &str = "Not available yet.";
const SYSTEM: &str = if cfg!(target_os = "macos") { "macOS" } else { "Windows" };

/// What About shows of this copy.
#[derive(Clone, Copy)]
pub struct AboutView<'a> {
    pub version: &'a str,
    /// Claude Code's version as the tool check knows it.
    pub claude: ClaudeVersion<'a>,
}

/// The two versions with Copy details, the links to read more, then who makes Jobdesk. No logo shows anywhere.
/// Returns true when Open Providers is clicked, so Settings switches to Providers.
pub fn body(ui: &mut Ui, view: AboutView, query: &str) -> bool {
    let &[this_copy, jobdesk, jobdesk_help, claude, claude_help, read_more, sign_in, sign_in_help, changed, changed_help, licences, licences_help, maker] =
        index(Page::About)
    else {
        return false;
    };
    setting_section(ui, this_copy, None, query);
    setting_row(ui, jobdesk, jobdesk_help, query, |ui| {
        if small_button(ui, "Copy details").clicked() {
            ui.ctx().copy_text(about_details(view.version, SYSTEM, std::env::consts::ARCH, view.claude));
        }
        ui.label(rich(ui, view.version, Type::MONO, Ink::Text));
    });
    setting_row(ui, claude, claude_help, query, |ui| match view.claude {
        ClaudeVersion::Found(version) => {
            ui.label(rich(ui, version, Type::MONO, Ink::Text));
        }
        // As Runtime status says it while the first check runs, so a slow probe never reads as a missing program.
        ClaudeVersion::Checking => {
            ui.label(rich(ui, "Checking…", Type::BODY, Ink::Muted));
        }
        ClaudeVersion::NotFound => {
            ui.label(rich(ui, "Not found", Type::BODY, Ink::Muted));
        }
    });
    setting_section(ui, read_more, None, query);
    // An in-app jump: no browser, no run and no setting change.
    let mut providers = false;
    setting_row(ui, sign_in, sign_in_help, query, |ui| providers = link(ui, "Open Providers").clicked());
    setting_row(ui, changed, changed_help, query, |ui| open(ui, "Open the changelog", CHANGELOG_URL));
    setting_row(ui, licences, licences_help, query, |ui| {
        // A link paints no disabled look of its own, so it fades here.
        ui.multiply_opacity(DISABLED_OPACITY);
        enabled_when(ui, false, NO_LICENCES, |ui| link(ui, "Show licences"));
    });
    page_note(ui, maker, query);
    providers
}

/// A link that opens `url` in the browser on click, with the address on hover.
pub(super) fn open(ui: &mut Ui, text: &str, url: &'static str) {
    let response: Response = link(ui, text).on_hover_text(url);
    if response.clicked() {
        ui.ctx().open_url(OpenUrl::new_tab(url));
    }
}
