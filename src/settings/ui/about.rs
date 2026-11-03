use egui::{OpenUrl, Response, Ui};

use crate::settings::application::{
    AboutAction, AboutButton, AboutUpdate, BarAction, CHECK_NOW, CHECKING_WORD, ClaudeVersion, Mark, Page, about_details, index,
};
use crate::settings::ui::{SettingsRequest, page_note};
use crate::ui_kit::tokens::DISABLED_OPACITY;
use crate::ui_kit::{
    Ink, Tone, Type, enabled_when, link, rich, setting_check, setting_row, setting_row_lines, setting_section, small_button, small_primary_button, tag,
};

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
    /// The updates: the switch, the latest version and the step to take.
    pub update: &'a AboutUpdate,
    /// Why Restart to update waits, if it does.
    pub restart_wait: Option<&'a str>,
}

/// The two versions with Copy details, the updates, the links to read more, then who makes Jobdesk. No logo shows
/// anywhere. Returns what the user asked of the updates. The switch is saved here, and the shell follows it.
pub fn body(ui: &mut Ui, view: AboutView, auto: &mut bool, query: &str) -> Option<SettingsRequest> {
    let &[
        this_copy,
        jobdesk,
        jobdesk_help,
        claude,
        claude_help,
        updates,
        check_auto,
        _,
        _,
        latest,
        read_more,
        changed,
        changed_help,
        licences,
        licences_help,
        maker,
    ] = index(Page::About)
    else {
        return None;
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
    setting_section(ui, updates, None, query);
    let update = view.update;
    setting_check(ui, auto, check_auto, update.auto_help, query);
    let help: Vec<(&str, Ink)> = update.help.iter().map(|line| (line.text.as_str(), if line.failed { Tone::Closed.into() } else { Ink::Muted })).collect();
    let mut request = None;
    setting_row_lines(ui, latest, &help, query, |ui| request = latest_controls(ui, update, view.restart_wait));
    setting_section(ui, read_more, None, query);
    setting_row(ui, changed, changed_help, query, |ui| open(ui, "Open the changelog", CHANGELOG_URL));
    setting_row(ui, licences, licences_help, query, |ui| {
        // A link paints no disabled look of its own, so it fades here.
        ui.multiply_opacity(DISABLED_OPACITY);
        enabled_when(ui, false, NO_LICENCES, |ui| link(ui, "Show licences"));
    });
    page_note(ui, maker, query);
    request
}

/// Latest version's controls, laid out from the right: Check now or the step to take, then what leads to it.
fn latest_controls(ui: &mut Ui, update: &AboutUpdate, restart_wait: Option<&str>) -> Option<SettingsRequest> {
    let mut request = None;
    // Check now waits only while a check runs, which the first help line says.
    if let Some(enabled) = update.check_now
        && enabled_when(ui, enabled, update.help.first().map_or("", |line| line.text.as_str()), |ui| small_button(ui, CHECK_NOW)).clicked()
    {
        request = Some(SettingsRequest::CheckForUpdates);
    }
    for button in update.buttons.iter().rev() {
        if about_button(ui, button, restart_wait).clicked() {
            request = Some(match button.action {
                AboutAction::WhatsNew => SettingsRequest::OpenReleaseNotes,
                AboutAction::ReleasePage => SettingsRequest::OpenReleasePage,
                AboutAction::Bar(BarAction::Download | BarAction::Retry) => SettingsRequest::DownloadUpdate,
                AboutAction::Bar(BarAction::Cancel) => SettingsRequest::CancelUpdate,
                AboutAction::Bar(BarAction::Restart) => SettingsRequest::RestartToUpdate,
            });
        }
    }
    if update.checking {
        ui.label(rich(ui, CHECKING_WORD, Type::BODY, Ink::Muted));
    }
    if let Some((word, mark)) = update.tag {
        tag(ui, word, if mark == Mark::Failed { Tone::Closed } else { Tone::Hired });
    }
    request
}

/// One of About's update steps: What's new and the release page as links, Cancel as a plain button, the step to take
/// as the primary one. Restart to update waits while a task is open.
fn about_button(ui: &mut Ui, button: &AboutButton, restart_wait: Option<&str>) -> Response {
    let label = button.label.as_str();
    match button.action {
        AboutAction::WhatsNew | AboutAction::ReleasePage => link(ui, label),
        AboutAction::Bar(BarAction::Restart) => {
            enabled_when(ui, restart_wait.is_none(), restart_wait.unwrap_or_default(), |ui| small_primary_button(ui, label))
        }
        _ if button.primary => small_primary_button(ui, label),
        _ => small_button(ui, label),
    }
}

/// A link that opens `url` in the browser on click, with the address on hover.
pub(super) fn open(ui: &mut Ui, text: &str, url: &'static str) {
    let response: Response = link(ui, text).on_hover_text(url);
    if response.clicked() {
        ui.ctx().open_url(OpenUrl::new_tab(url));
    }
}
