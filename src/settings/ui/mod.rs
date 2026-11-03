mod about;
mod data;
mod defaults;
mod jobs_view;
mod permissions;
mod providers;
mod queue;
mod terms;
mod usage;

use std::cell::Cell;

use chrono::{NaiveDate, NaiveDateTime};
use egui::{Align, ComboBox, Frame, Key, Label, Layout, Margin, Modifiers, Popup, Rect, ScrollArea, ThemePreference, Ui, UiBuilder, pos2, vec2};

use crate::settings::application::{
    CapCheck, ClaudeVersion, DataSizes, Drawn, Group, IssueKind, Page, Period, Preferences, Providers, RuntimeWord, SearchHits, Settings, StartScreen,
    TermsState, TermsWord, TextSize, ThemeChoice, UsageState, WordTone, cap_word, hidden_word, index, rail_word, search, size_short, terms_drawn, terms_word,
};
use crate::settings::ui::about::AboutView;
use crate::settings::ui::data::DataView;
pub use crate::settings::ui::providers::LastTask;
use crate::settings::ui::providers::{ProvidersScreen, ProvidersView};
use crate::ui_kit::tokens::{
    BODY_PADDING, DOC_MEASURE, FIELD_HEIGHT, GUTTER, HEADER_PADDING, INSET, PICK_PADDING, PICK_ROW_HEIGHT, RUN_SIDE, SETTING_ROW_PADDING, SETTING_SECTION_GAP,
};
use crate::ui_kit::{
    Ink, Tone, Type, back_pressed, copyable, divider, enabled_when, hand, hit_mark, pick_row, rail_title, rich, search_field, set_reduced_motion,
    setting_check, setting_row, setting_section, settings_page, settings_page_with_foot, shortcut_button, slip, small_button, toggle_group,
};

/// How wide Open on's combo box is.
const OPEN_ON_WIDTH: f32 = 180.0;

const SHOW_FOLDER: &str = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in Explorer" };

/// What stays between frames: the page open, the search, the period Usage statistics counts, Run defaults' amounts
/// as typed, whether Clear tray decisions is asking, Providers' way picked and key as typed, whether Your terms shows
/// its command, the skip reason and the score as typed.
#[derive(Default)]
pub struct SettingsScreen {
    page: Page,
    query: String,
    period: Period,
    edits: defaults::Edits,
    /// Clear tray decisions' popover: open, and whether Keep them still takes the focus.
    clearing: Option<bool>,
    providers: ProvidersScreen,
    terms_command: bool,
    reason: String,
    score: Option<String>,
}

impl SettingsScreen {
    pub fn open_on(&mut self, page: Page) {
        self.page = page;
    }

    /// The page open, which Settings reopens on.
    pub fn page(&self) -> Page {
        self.page
    }

    /// Settings is closed, by whatever way: a popover that was asking closes with it, so reopening never finds it open.
    /// A key typed and not saved is forgotten.
    pub fn closed(&mut self) {
        self.clearing = None;
        self.providers.reset();
        self.terms_command = false;
    }
}

pub struct SettingsView<'a> {
    pub settings: &'a mut Settings,
    /// The job search folder as the user reads it, if one is chosen.
    pub folder: Option<&'a str>,
    /// Whether the folder is open and read, so Runtime status draws its sections and a search can match them.
    pub folder_read: bool,
    pub runtime: RuntimeWord,
    /// Why Change folder waits, if it does, as the shell words it.
    pub folder_wait: Option<&'a str>,
    pub usage: UsageState<'a>,
    /// The app's day, which names the month in Usage statistics before its logs are read.
    pub today: NaiveDate,
    /// Whether a new task may start this month, as the launcher checks it.
    pub cap: CapCheck,
    /// The app's data folder as the user reads it, if there is one.
    pub data: Option<&'a str>,
    /// What the data folder holds, once measured.
    pub sizes: Option<DataSizes>,
    /// How many decisions the open folder's tray holds. None with no folder open, or a tray that could not be read or saved.
    pub decisions: Option<usize>,
    /// This copy's version, and Claude Code's as the tool check knows it.
    pub version: &'a str,
    pub claude: ClaudeVersion<'a>,
    /// How tasks sign in, the saved key and Test connection.
    pub providers: &'a mut Providers,
    /// The newest task that said how it signed in, if any.
    pub last_task: Option<LastTask>,
    /// A task's process is up, so Test connection waits.
    pub task_running: bool,
    /// One of those tasks is working, not only waiting for a reply.
    pub task_working: bool,
    /// The app's clock, for the ages Providers shows.
    pub now: NaiveDateTime,
    /// Your terms as the open folder gives them.
    pub terms: TermsState<'a>,
    /// How many jobs the Jobs view hides, and how many there are in all. None until the saved jobs are read.
    pub jobs: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRequest {
    Back,
    ChangeFolder,
    ShowFolder,
    /// Show the app's own data folder, never the job search folder.
    ShowDataFolder,
    /// Forget the open folder's tray decisions. Asked only after Clear decisions in the popover.
    ClearDecisions,
    /// Close Settings and show Applications.
    OpenApplications,
    /// Open the launcher, as New task… does on Tasks.
    NewTask,
    /// Close Settings and show the newest task that said how it signed in.
    OpenLastTask,
    /// Close Settings and show Profile, where the profile your terms come from is edited.
    OpenProfile,
}

/// The Settings screen: Back to app and the rail at the left, the open page on paper beside them.
/// `other_page` draws the pages another feature owns, such as Runtime status, with the search query.
pub fn show(ui: &mut Ui, screen: &mut SettingsScreen, view: SettingsView, other_page: &mut dyn FnMut(&mut Ui, Page, &str)) -> Option<SettingsRequest> {
    let mut request = None;
    // The rail may pick another page before the page draws, so the page open as the frame began is kept.
    let opened_on = screen.page;
    // Esc clears a search before anything else takes it, the field included. While Clear tray decisions asks, Esc
    // belongs to its popover, which egui's memory of open popups does not hold, and keeps the decisions.
    let ctx = ui.ctx().clone();
    let asking = screen.clearing.is_some() || screen.providers.asking();
    let clears = !asking && !screen.query.is_empty() && !Popup::is_any_open(&ctx) && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    if clears {
        screen.query.clear();
    } else if !asking && back_pressed(&ctx) {
        request = Some(SettingsRequest::Back);
    }
    let area = ui.available_rect_before_wrap();
    let rail_rect = Rect::from_min_max(area.min, pos2((area.left() + RUN_SIDE).min(area.right()), area.bottom()));
    let page_rect = Rect::from_min_max(pos2((rail_rect.right() + f32::from(GUTTER)).min(area.right()), area.top()), area.max);
    let mut rail_ui = ui.new_child(UiBuilder::new().id_salt("settings_rail").max_rect(rail_rect).layout(Layout::top_down(Align::Min)));
    rail_ui.set_width(rail_rect.width());
    if shortcut_button(&mut rail_ui, "Back to app", &["Esc"]).clicked() {
        request = Some(SettingsRequest::Back);
    }
    rail_ui.add_space(f32::from(GUTTER));
    rail(&mut rail_ui, screen, &view);
    let mut page_ui = ui.new_child(UiBuilder::new().id_salt("settings_page").max_rect(page_rect).layout(Layout::top_down(Align::Min)));
    page_ui.set_width(page_rect.width());
    let query = screen.query.trim().to_owned();
    let page = screen.page;
    match page {
        Page::Providers => {
            let last_task = view.last_task.as_ref();
            let providers_view =
                ProvidersView { last_task, claude: view.claude, task_running: view.task_running, task_working: view.task_working, now: view.now };
            let (state, settings, providers) = (&mut screen.providers, view.settings, view.providers);
            let (_, asked) =
                settings_page(&mut page_ui, page.heading(), page.lead(), |_| {}, |ui| providers::body(ui, state, settings, providers, providers_view, &query));
            request = asked.or(request);
        }
        Page::General => {
            let (_, asked) =
                settings_page(&mut page_ui, page.title(), page.lead(), |_| {}, |ui| general(ui, view.settings, view.folder, view.folder_wait, &query));
            request = asked.or(request);
        }
        Page::Appearance => {
            settings_page(&mut page_ui, page.title(), page.lead(), |_| {}, |ui| appearance(ui, view.settings, &query));
        }
        Page::Usage => {
            let lead = if matches!(view.usage, UsageState::Reading) { usage::READING } else { page.lead() };
            // The toggles run first, so the body counts the period picked this frame.
            let period = Cell::new(screen.period);
            let cap = view.settings.preferences().cap;
            let (_, asked) = settings_page(
                &mut page_ui,
                page.title(),
                lead,
                |ui| usage::period_toggles(ui, &period, view.today),
                |ui| usage::body(ui, view.usage, period.get(), view.today, &cap, &query),
            );
            screen.period = period.get();
            request = asked.or(request);
        }
        Page::Defaults => {
            let edits = &mut screen.edits;
            settings_page(&mut page_ui, page.heading(), page.lead(), |_| {}, |ui| defaults::body(ui, view.settings, edits, view.usage, &query));
        }
        Page::Permissions => {
            settings_page(&mut page_ui, page.heading(), page.lead(), |_| {}, |ui| permissions::body(ui, view.settings, &query));
        }
        Page::Data => {
            let data = DataView { folder: view.data, sizes: view.sizes, decisions: view.decisions, workspace_open: view.folder.is_some() };
            let clearing = &mut screen.clearing;
            let (_, asked) = settings_page(&mut page_ui, page.heading(), page.lead(), |_| {}, |ui| data::body(ui, view.settings, data, clearing, &query));
            request = asked.or(request);
        }
        Page::About => {
            let about = AboutView { version: view.version, claude: view.claude };
            let (_, providers) = settings_page(&mut page_ui, page.heading(), page.lead(), |_| {}, |ui| about::body(ui, about, &query));
            if providers {
                screen.page = Page::Providers;
            }
        }
        Page::Terms => {
            let lead = if matches!(view.terms, TermsState::Reading) { terms::READING } else { page.lead() };
            let show_command = &mut screen.terms_command;
            let (asked, _) = settings_page_with_foot(
                &mut page_ui,
                page.heading(),
                lead,
                |ui| terms::actions(ui, view.terms),
                |ui| terms::body(ui, view.terms, show_command, &query),
                terms::foot,
            );
            request = asked.or(request);
        }
        Page::Queue => {
            let reason = &mut screen.reason;
            settings_page(&mut page_ui, page.heading(), page.lead(), |_| {}, |ui| queue::body(ui, view.settings, reason, &query));
        }
        Page::JobsView => {
            let score = &mut screen.score;
            settings_page(&mut page_ui, page.heading(), page.lead(), |_| {}, |ui| jobs_view::body(ui, view.settings, view.terms, view.jobs, score, &query));
        }
        Page::Runtime => other_page(&mut page_ui, page, &query),
    }
    // The popover asks about the page it sits on, so another page or leaving Settings closes it.
    if screen.page != Page::Data || request == Some(SettingsRequest::Back) {
        screen.clearing = None;
    }
    // Another page or leaving Settings closes the command Your terms shows.
    if screen.page != Page::Terms || request == Some(SettingsRequest::Back) {
        screen.terms_command = false;
    }
    // Leaving Providers forgets a key typed and not saved, and picks the way in use when it opens again.
    if opened_on == Page::Providers && (screen.page != Page::Providers || request == Some(SettingsRequest::Back)) {
        screen.providers.reset();
    }
    ui.advance_cursor_after_rect(area);
    request
}

/// The preferences the window holds before any settings file: the theme and zoom eframe restored from the last session,
/// with the zoom snapped to a whole percent as `follow_zoom` does. The rest are the defaults.
pub fn restored(ctx: &egui::Context) -> Preferences {
    let theme = match ctx.options(|options| options.theme_preference) {
        ThemePreference::System => ThemeChoice::System,
        ThemePreference::Light => ThemeChoice::Light,
        ThemePreference::Dark => ThemeChoice::Dark,
    };
    Preferences { theme, text_size: TextSize::from_factor(ctx.zoom_factor()), ..Preferences::default() }
}

/// Applies the preferences to the window. Called when one changes, and once at start, never every frame.
pub fn apply(ctx: &egui::Context, preferences: &Preferences) {
    ctx.set_theme(match preferences.theme {
        ThemeChoice::System => ThemePreference::System,
        ThemeChoice::Light => ThemePreference::Light,
        ThemeChoice::Dark => ThemePreference::Dark,
    });
    ctx.set_zoom_factor(preferences.text_size.factor());
    set_reduced_motion(ctx, preferences.reduce_motion);
}

fn seen_zoom_id() -> egui::Id {
    egui::Id::NULL.with("settings_seen_zoom")
}

/// Saves the text size when the zoom moved since the last frame, as Cmd + and Cmd − move it.
/// A new zoom shows only on the frame after it is set, so the size `apply` set comes back as the saved one and saves nothing.
pub fn follow_zoom(ctx: &egui::Context, settings: &mut Settings) {
    let zoom = ctx.zoom_factor();
    let seen: Option<f32> = ctx.data_mut(|data| {
        let seen = data.get_temp(seen_zoom_id());
        data.insert_temp(seen_zoom_id(), zoom);
        seen
    });
    if seen.is_some_and(|seen| seen != zoom) {
        settings.set_text_size(TextSize::from_factor(zoom));
    }
}

/// The rail: its title, the search, the pages in their groups, and a foot that says how changes are kept.
fn rail(ui: &mut Ui, screen: &mut SettingsScreen, view: &SettingsView) {
    slip(ui, |ui| {
        Frame::new().inner_margin(Margin { left: INSET, right: INSET, top: BODY_PADDING, bottom: HEADER_PADDING }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = BODY_PADDING.into();
            ui.label(rich(ui, "Settings", Type::SLIP_TITLE, Ink::Text));
            search_field(ui, &mut screen.query, "Search settings");
        });
        let hits = search(&screen.query, Drawn { folder_read: view.folder_read, terms: terms_drawn(view.terms) });
        if let Some(found) = &hits
            && let Some(&(first, _)) = found.pages.first()
            && !found.pages.iter().any(|&(page, _)| page == screen.page)
        {
            screen.page = first;
        }
        let rest = ui.available_rect_before_wrap();
        let mut foot = ui.new_child(UiBuilder::new().id_salt("settings_rail_foot").max_rect(rest).layout(Layout::bottom_up(Align::Min)));
        rail_foot(&mut foot, view.settings, hits.as_ref());
        let rows_rect = Rect::from_min_max(rest.min, pos2(rest.right(), foot.min_rect().top()));
        let mut rows = ui.new_child(UiBuilder::new().id_salt("settings_rail_rows").max_rect(rows_rect).layout(Layout::top_down(Align::Min)));
        ScrollArea::vertical().id_salt("settings_rail_scroll").auto_shrink([false, false]).show(&mut rows, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            rail_rows(ui, screen, view, hits.as_ref());
        });
        ui.advance_cursor_after_rect(rest);
    });
}

/// The pages under their group titles. A search keeps only the pages it matched, each with its count.
fn rail_rows(ui: &mut Ui, screen: &mut SettingsScreen, view: &SettingsView, hits: Option<&SearchHits>) {
    if hits.is_some_and(|found| found.pages.is_empty()) {
        rail_title(ui, "No setting matches");
        return;
    }
    // Rows sit in so their words line up with the group titles.
    let side = INSET - PICK_PADDING.0;
    for group in Group::ALL {
        let pages: Vec<(Page, Option<usize>)> = match hits {
            Some(found) => found.pages.iter().filter(|(page, _)| page.group() == group).map(|&(page, count)| (page, Some(count))).collect(),
            None => Page::ALL.iter().filter(|page| page.group() == group).map(|&page| (page, None)).collect(),
        };
        if pages.is_empty() {
            continue;
        }
        rail_title(ui, group.title());
        Frame::new().inner_margin(Margin::symmetric(side, 0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (page, count) in pages {
                let (word, ink) = match count {
                    Some(1) => ("1 match".to_owned(), Ink::Muted),
                    Some(count) => (format!("{count} matches"), Ink::Muted),
                    None => state_word(page, view),
                };
                let on = page == screen.page;
                if pick_row(ui, on, |ui| rail_row(ui, page.title(), &word, ink)).clicked() {
                    screen.page = page;
                }
            }
        });
    }
}

/// The one word at a row's right, saying the page's state.
fn state_word(page: Page, view: &SettingsView) -> (String, Ink) {
    match page {
        Page::Appearance => (view.settings.preferences().theme.label().to_owned(), Ink::Muted),
        Page::Usage => match view.usage {
            UsageState::Read { report, today, .. } => (rail_word(report, today), Ink::Muted),
            UsageState::Reading | UsageState::NoFolder => (String::new(), Ink::Muted),
        },
        Page::Runtime => match view.runtime {
            RuntimeWord::None => (String::new(), Ink::Muted),
            RuntimeWord::Missing(n) => (format!("{n} missing"), Tone::Closed.into()),
            RuntimeWord::ToFill(n) => (format!("{n} to fill"), Tone::Interview.into()),
        },
        Page::Defaults => match (view.cap, view.settings.preferences().cap.active()) {
            (CapCheck::Reached(_), _) => ("Cap reached".to_owned(), Tone::Closed.into()),
            (_, Some(amount)) => (cap_word(amount), Ink::Muted),
            (_, None) => (String::new(), Ink::Muted),
        },
        Page::Providers => {
            let (word, tone) = view.providers.rail_word(view.settings.preferences());
            (word, if tone == WordTone::Closed { Tone::Closed.into() } else { Ink::Muted })
        }
        Page::Permissions => (view.settings.preferences().permissions.label().to_owned(), Ink::Muted),
        Page::Data => (view.sizes.map(|sizes| size_short(sizes.total())).unwrap_or_default(), Ink::Muted),
        Page::About => (view.version.to_owned(), Ink::Muted),
        Page::Terms => match terms_word(view.terms) {
            word @ TermsWord::NotStated(_) => (word.text(), Tone::Interview.into()),
            word @ TermsWord::Hidden => (word.text(), Tone::Closed.into()),
            word => (word.text(), Ink::Muted),
        },
        Page::JobsView => (hidden_word(&view.settings.preferences().jobs_view, view.jobs), Ink::Muted),
        Page::General | Page::Queue => (String::new(), Ink::Muted),
    }
}

/// A line at the foot of a page, under its last row, held to the measure.
fn page_note(ui: &mut Ui, text: &str, query: &str) {
    ui.add_space(SETTING_SECTION_GAP.0);
    ui.scope(|ui| {
        ui.set_max_width(DOC_MEASURE);
        hit_mark(ui, text, Type::NOTE, Ink::Muted, query);
    });
}

fn rail_row(ui: &mut Ui, title: &str, word: &str, ink: Ink) {
    let height = PICK_ROW_HEIGHT - f32::from(PICK_PADDING.1) * 2.0;
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_height(height);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if !word.is_empty() {
                ui.add(Label::new(rich(ui, word, Type::NOTE, ink)));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(Label::new(rich(ui, title, Type::BODY, Ink::Text)).truncate());
            });
        });
    });
}

/// What the rail's foot says when the settings file could not be read at launch.
const UNREAD_FOOT: &str = "Settings could not be read, so the defaults are in use. Your next change replaces the file.";
/// What the rail's foot says when a change could not be saved.
const UNSAVED_FOOT: &str = "Settings could not be saved. Changes last until Jobdesk closes.";

/// The foot of the rail, laid out from the bottom up: a file that failed, what a search matched, or how changes are kept.
fn rail_foot(ui: &mut Ui, settings: &Settings, hits: Option<&SearchHits>) {
    let issue = settings.issue().map(|(kind, _)| kind);
    let (text, ink): (String, Ink) = match hits {
        _ if issue == Some(IssueKind::Read) => (UNREAD_FOOT.to_owned(), Tone::Closed.into()),
        _ if issue == Some(IssueKind::Save) => (UNSAVED_FOOT.to_owned(), Tone::Closed.into()),
        Some(found) if found.total == 1 => ("1 setting matches. Esc clears the search.".to_owned(), Ink::Muted),
        Some(found) if found.total > 1 => (format!("{} settings match. Esc clears the search.", found.total), Ink::Muted),
        Some(_) => ("Esc clears the search.".to_owned(), Ink::Muted),
        None => ("Changes apply at once and stay on this computer. Your job search folder is never changed here.".to_owned(), Ink::Muted),
    };
    Frame::new().inner_margin(Margin::symmetric(INSET, BODY_PADDING)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add(Label::new(rich(ui, text, Type::NOTE, ink)).wrap());
    });
    divider(ui);
}

/// The folder and the screen Jobdesk opens on. Returns what the user asked of the folder.
fn general(ui: &mut Ui, settings: &mut Settings, folder: Option<&str>, folder_wait: Option<&str>, query: &str) -> Option<SettingsRequest> {
    let &[section, label, help, opens, open_on, open_help] = index(Page::General) else { return None };
    let mut request = None;
    setting_section(ui, section, None, query);
    setting_row(ui, label, help, query, |ui| match folder {
        Some(_) => {
            if enabled_when(ui, folder_wait.is_none(), folder_wait.unwrap_or_default(), |ui| small_button(ui, "Change folder")).clicked() {
                request = Some(SettingsRequest::ChangeFolder);
            }
            if small_button(ui, SHOW_FOLDER).clicked() {
                request = Some(SettingsRequest::ShowFolder);
            }
        }
        None => {
            if small_button(ui, "Choose folder").clicked() {
                request = Some(SettingsRequest::ChangeFolder);
            }
            enabled_when(ui, false, "Choose a folder first.", |ui| small_button(ui, SHOW_FOLDER));
        }
    });
    // The folder reads under the row's help, so it takes back the row's bottom padding.
    ui.add_space(-SETTING_ROW_PADDING);
    match folder {
        Some(folder) => {
            copyable(ui, |ui| ui.add(Label::new(rich(ui, folder, Type::MONO, Ink::Text)).truncate()));
        }
        None => {
            ui.add(Label::new(rich(ui, "No folder chosen yet.", Type::NOTE, Ink::Muted)));
        }
    }
    ui.add_space(SETTING_ROW_PADDING);
    setting_section(ui, opens, None, query);
    setting_row(ui, open_on, open_help, query, |ui| {
        ui.spacing_mut().interact_size.y = FIELD_HEIGHT;
        let current = settings.preferences().open_on;
        let mut at = StartScreen::ALL.iter().position(|&screen| screen == current).unwrap_or(0);
        let combo = ComboBox::from_id_salt("settings_open_on").width(OPEN_ON_WIDTH).selected_text(current.label()).show_index(
            ui,
            &mut at,
            StartScreen::ALL.len(),
            |index| StartScreen::ALL[index].label(),
        );
        // Its label is the row's own text, so the box is named here.
        ui.ctx().accesskit_node_builder(combo.id, |node| node.set_label(open_on));
        if hand(combo).changed() {
            settings.set_open_on(StartScreen::ALL[at]);
        }
    });
    request
}

/// Theme, text size and motion. Each change is applied to the window at once.
fn appearance(ui: &mut Ui, settings: &mut Settings, query: &str) {
    let &[look, theme, theme_help, size, size_help, motion, reduce, reduce_help] = index(Page::Appearance) else { return };
    let ctx = ui.ctx().clone();
    setting_section(ui, look, None, query);
    setting_row(ui, theme, theme_help, query, |ui| {
        let options = ThemeChoice::ALL.map(|choice| (choice, choice.label()));
        if let Some(choice) = toggle_group(ui, Some(settings.preferences().theme), &options) {
            settings.set_theme(choice);
            apply(&ctx, settings.preferences());
        }
    });
    setting_row(ui, size, size_help, query, |ui| {
        let shown = TextSize::from_factor(ctx.zoom_factor());
        let labels = TextSize::STEPS.map(|step| TextSize::from_percent(step.into()).label());
        let options: Vec<(u16, &str)> = TextSize::STEPS.iter().zip(&labels).map(|(&step, label)| (step, label.as_str())).collect();
        let current = TextSize::STEPS.into_iter().find(|&step| TextSize::from_percent(step.into()) == shown);
        if let Some(step) = toggle_group(ui, current, &options) {
            settings.set_text_size(TextSize::from_percent(step.into()));
            apply(&ctx, settings.preferences());
        }
    });
    setting_section(ui, motion, None, query);
    let mut on = settings.preferences().reduce_motion;
    if setting_check(ui, &mut on, reduce, reduce_help, query).changed() {
        settings.set_reduce_motion(on);
        apply(&ctx, settings.preferences());
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;
    use crate::runs::application::{ModelCost, TaskCost, TaskOutcome};
    use crate::settings::application::{Commute, CommuteTier, Pipeline, TermPart, UsageReport, YourTerms, sent_on};

    struct State {
        screen: SettingsScreen,
        settings: Settings,
        asked: Option<SettingsRequest>,
        report: UsageReport,
        pipeline: Pipeline,
        reading: bool,
        decisions: Option<usize>,
        cap: CapCheck,
        no_folder: bool,
        providers: Providers,
        last_task: Option<LastTask>,
        task_running: bool,
        terms: YourTerms,
        jobs: Option<(usize, usize)>,
    }

    /// A key kept in memory, and a Test connection that never answers.
    fn memory_providers(key: Option<&str>) -> Providers {
        crate::settings::open_providers(crate::settings::memory_keychain(key), crate::settings::idle_tester())
    }

    fn now() -> NaiveDateTime {
        today().and_hms_opt(11, 30, 0).unwrap()
    }

    /// The fictional data folder's sizes: 212 MB in all.
    const SIZES: DataSizes = DataSizes { run_logs: 211_800_000, decisions: 18_000 };

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn task(on: NaiveDate, command: Option<&str>, model: &str, cost_usd: f64) -> TaskCost {
        let model = ModelCost { model: model.into(), tokens_in: 100_000, tokens_out: 10_000, cache_read: 80_000, cost_usd };
        TaskCost { started_on: on, command: command.map(str::to_owned), outcome: TaskOutcome::Finished, cost_usd, models: vec![model] }
    }

    /// Fictional tasks: $41.20 in October, and $9.00 in September.
    fn report() -> UsageReport {
        UsageReport::new(vec![
            task(day(2026, 10, 3), Some("/apply"), "claude-opus-4-5", 30.0),
            task(day(2026, 10, 13), Some("/rank"), "claude-sonnet-4-5", 11.2),
            task(day(2026, 9, 4), None, "claude-haiku-4-5", 9.0),
        ])
    }

    fn setup(ctx: &egui::Context) {
        crate::ui_kit::install_fonts(ctx);
        crate::ui_kit::install_theme(ctx);
    }

    fn harness(state: Rc<RefCell<State>>) -> Harness<'static> {
        harness_at(state, 1280.0)
    }

    fn harness_at(state: Rc<RefCell<State>>, width: f32) -> Harness<'static> {
        Harness::builder().with_size([width, 820.0]).with_context_setup(setup).build_ui(move |ui| {
            let state = &mut *state.borrow_mut();
            let usage = if state.reading { UsageState::Reading } else { UsageState::Read { report: &state.report, pipeline: &state.pipeline, today: today() } };
            let view = SettingsView {
                settings: &mut state.settings,
                folder: (!state.no_folder).then_some("~/Documents/ai-job-search"),
                folder_read: true,
                runtime: RuntimeWord::Missing(1),
                folder_wait: None,
                usage,
                today: today(),
                cap: state.cap,
                data: Some("~/Library/Application Support/Jobdesk"),
                sizes: Some(SIZES),
                decisions: state.decisions,
                version: "0.1.0",
                claude: ClaudeVersion::Found("2.1.4"),
                providers: &mut state.providers,
                last_task: state.last_task.clone(),
                task_running: state.task_running,
                task_working: state.task_running,
                now: now(),
                terms: if state.no_folder { TermsState::NoFolder } else { TermsState::Read(&state.terms) },
                jobs: state.jobs,
            };
            let asked = show(ui, &mut state.screen, view, &mut |ui, page, _| {
                ui.label(format!("{} body", page.title()));
            });
            if asked.is_some() {
                state.asked = asked;
            }
        })
    }

    /// Fictional terms: the floor given, and two commute tiers from Namur.
    fn terms(floor: Option<u32>) -> YourTerms {
        let tier = |name: &str, places: &[&str], note: &str| CommuteTier {
            name: name.to_owned(),
            places: places.iter().map(|place| place.to_string()).collect(),
            note: note.to_owned(),
        };
        let tiers = vec![tier("ideal", &["Namur", "Charleroi"], "Under 45 minutes."), tier("too far", &["Over 2 hours"], "")];
        YourTerms { floor: TermPart::Read(floor), commute: TermPart::Read(Commute { origin: Some("from Namur (by car)".to_owned()), tiers }) }
    }

    fn state() -> Rc<RefCell<State>> {
        state_with(Settings::unsaved())
    }

    /// A settings file that cannot be read, or one that reads but cannot be saved.
    struct Broken {
        read: bool,
    }

    impl crate::settings::application::PreferencesStore for Broken {
        fn load(&self) -> Result<Option<Preferences>, crate::shared_kernel::LoadError> {
            if self.read { Err(crate::shared_kernel::LoadError::new("settings.json", "bad")) } else { Ok(Some(Preferences::default())) }
        }

        fn save(&self, _: &Preferences) -> Result<(), crate::shared_kernel::LoadError> {
            Err(crate::shared_kernel::LoadError::new("settings.json", "disk full"))
        }
    }

    fn state_with(settings: Settings) -> Rc<RefCell<State>> {
        let pipeline = sent_on(&[day(2026, 10, 7), day(2026, 10, 16), day(2026, 8, 20)]);
        Rc::new(RefCell::new(State {
            screen: SettingsScreen::default(),
            settings,
            asked: None,
            report: report(),
            pipeline,
            reading: false,
            decisions: Some(214),
            cap: CapCheck::Open,
            no_folder: false,
            providers: memory_providers(None),
            last_task: None,
            task_running: false,
            terms: terms(Some(700)),
            jobs: Some((0, 14)),
        }))
    }

    #[test]
    fn a_file_that_could_not_be_read_says_the_defaults_are_in_use() {
        let mut harness = harness(state_with(Settings::open(Box::new(Broken { read: true }), Preferences::default())));
        harness.run();
        harness.get_by_label(UNREAD_FOOT);
        assert!(harness.query_by_label(UNSAVED_FOOT).is_none());
    }

    #[test]
    fn a_change_that_could_not_be_saved_says_it_lasts_until_jobdesk_closes() {
        let state = state_with(Settings::open(Box::new(Broken { read: false }), Preferences::default()));
        state.borrow_mut().settings.set_reduce_motion(true);
        let mut harness = harness(state);
        harness.run();
        harness.get_by_label(UNSAVED_FOOT);
        assert!(harness.query_by_label(UNREAD_FOOT).is_none());
    }

    #[test]
    fn choosing_in_open_on_saves_the_choice() {
        let folder = tempfile::tempdir().unwrap();
        let state = state_with(crate::settings::open_settings(folder.path(), Preferences::default()));
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by(|node| node.role() == egui::accesskit::Role::ComboBox && node.label().as_deref() == Some("Open on")).click();
        harness.run();
        harness.get_by_label("Jobs").click();
        harness.run();
        assert_eq!(state.borrow().settings.preferences().open_on, StartScreen::Jobs);
        let reopened = crate::settings::open_settings(folder.path(), Preferences::default());
        assert_eq!(reopened.preferences().open_on, StartScreen::Jobs, "the choice is in the file");
    }

    #[test]
    fn restored_takes_the_theme_and_snaps_the_zoom() {
        let ctx = egui::Context::default();
        ctx.options_mut(|options| {
            options.theme_preference = ThemePreference::Dark;
            options.zoom_factor = 1.2549;
        });
        let preferences = restored(&ctx);
        assert_eq!((preferences.theme, preferences.text_size.percent()), (ThemeChoice::Dark, 125));
        assert_eq!((preferences.reduce_motion, preferences.open_on), (false, StartScreen::Today));
    }

    #[test]
    fn the_rail_lists_every_page_in_its_group_and_every_one_opens() {
        let mut harness = harness(state());
        harness.run();
        // Appearance's word is the theme, System by default, which is also a group's title.
        for group in Group::ALL {
            assert!(harness.query_all_by_label(group.title()).next().is_some(), "{}", group.title());
        }
        assert_eq!(harness.query_all_by_label("System").count(), 2);
        for page in Page::ALL {
            let rows: Vec<_> = harness.query_all_by_label(page.title()).collect();
            assert!(!rows.is_empty(), "{}", page.title());
            assert!(!rows[0].accesskit_node().is_disabled(), "{}", page.title());
        }
        assert_eq!(harness.query_all_by_label("Later").count(), 0, "every page is built");
        harness.get_by_label("1 missing");
    }

    #[test]
    fn dark_sets_the_preference_and_the_window() {
        let state = state();
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("Appearance").click();
        harness.run();
        harness.get_by_label("Dark").click();
        harness.run();
        assert_eq!(state.borrow().settings.preferences().theme, ThemeChoice::Dark);
        assert_eq!(harness.ctx.theme(), egui::Theme::Dark);
    }

    #[test]
    fn a_text_size_zooms_the_window() {
        let state = state();
        state.borrow_mut().screen.open_on(Page::Appearance);
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("125%").click();
        harness.run();
        assert_eq!(harness.ctx.zoom_factor(), 1.25);
        assert_eq!(state.borrow().settings.preferences().text_size.percent(), 125);
    }

    #[test]
    fn a_search_keeps_the_matching_pages_with_their_counts() {
        let state = state();
        state.borrow_mut().screen.open_on(Page::Appearance);
        state.borrow_mut().screen.query = "folder".to_owned();
        let mut harness = harness(state.clone());
        harness.run();
        // Data and privacy matches four times, General three, Permissions twice, Run defaults and Runtime status once.
        assert_eq!(harness.query_all_by_label("4 matches").count(), 1);
        assert_eq!(harness.query_all_by_label("3 matches").count(), 1);
        assert_eq!(harness.query_all_by_label("2 matches").count(), 1);
        assert_eq!(harness.query_all_by_label("1 match").count(), 2);
        harness.get_by_label("11 settings match. Esc clears the search.");
        assert!(harness.query_by_label("Appearance").is_none());
        assert!(harness.query_by_label("Usage statistics").is_none());
        assert_eq!(state.borrow().screen.page, Page::General, "the open page had no hit");
    }

    #[test]
    fn no_match_keeps_the_page() {
        let state = state();
        state.borrow_mut().screen.open_on(Page::Appearance);
        state.borrow_mut().screen.query = "zzz".to_owned();
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("No setting matches");
        assert_eq!(state.borrow().screen.page, Page::Appearance);
    }

    #[test]
    fn esc_clears_the_search_first_then_goes_back() {
        let state = state();
        state.borrow_mut().screen.query = "folder".to_owned();
        let mut harness = harness(state.clone());
        harness.run();
        harness.key_press(Key::Escape);
        harness.run();
        assert_eq!(state.borrow().screen.query, "");
        assert_eq!(state.borrow().asked, None);
        harness.key_press(Key::Escape);
        harness.run();
        assert_eq!(state.borrow().asked, Some(SettingsRequest::Back));
    }

    #[test]
    fn the_folder_buttons_ask_the_shell() {
        let state = state();
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("~/Documents/ai-job-search");
        harness.get_by_label(SHOW_FOLDER).click();
        harness.run();
        assert_eq!(state.borrow().asked, Some(SettingsRequest::ShowFolder));
        harness.get_by_label("Change folder").click();
        harness.run();
        assert_eq!(state.borrow().asked, Some(SettingsRequest::ChangeFolder));
    }

    #[test]
    fn with_no_folder_show_waits_for_one() {
        let mut settings = Settings::unsaved();
        let mut screen = SettingsScreen::default();
        let mut providers = memory_providers(None);
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(setup).build_ui(move |ui| {
            let view = SettingsView {
                settings: &mut settings,
                folder: None,
                folder_read: false,
                runtime: RuntimeWord::None,
                folder_wait: None,
                usage: UsageState::NoFolder,
                today: today(),
                cap: CapCheck::Open,
                data: None,
                sizes: None,
                decisions: None,
                version: "0.1.0",
                claude: ClaudeVersion::NotFound,
                providers: &mut providers,
                last_task: None,
                task_running: false,
                task_working: false,
                now: now(),
                terms: TermsState::NoFolder,
                jobs: None,
            };
            show(ui, &mut screen, view, &mut |_, _, _| {});
        });
        harness.run();
        harness.get_by_label("No folder chosen yet.");
        harness.get_by_label("Choose folder");
        assert!(harness.get_by_label(SHOW_FOLDER).accesskit_node().is_disabled());
    }

    #[test]
    fn every_indexed_line_shows_on_its_page() {
        for page in [
            Page::General,
            Page::Appearance,
            Page::Usage,
            Page::Terms,
            Page::Queue,
            Page::JobsView,
            Page::Providers,
            Page::Defaults,
            Page::Permissions,
            Page::Data,
            Page::About,
        ] {
            let state = state();
            state.borrow_mut().screen.open_on(page);
            // Your terms draws only one of each part's helps. The other states are checked one by one below.
            let drawn = Drawn { folder_read: true, terms: terms_drawn(TermsState::Read(&state.borrow().terms)) };
            let mut harness = harness(state);
            harness.run();
            // A checkbox's help is its description, as its words are part of it, and the cap's checkbox is named by its sentence.
            for line in crate::settings::application::drawn_index(page, drawn) {
                let described = harness.query_all_by(|node| node.description().as_deref() == Some(line)).next().is_some();
                let in_sentence = harness
                    .query_all_by(|node| node.role() == egui::accesskit::Role::CheckBox && node.label().is_some_and(|label| label.contains(line)))
                    .next()
                    .is_some();
                assert!(described || in_sentence || harness.query_all_by_label(line).next().is_some(), "{}: {line}", page.title());
            }
        }
    }

    fn on(state: &Rc<RefCell<State>>, page: Page) {
        state.borrow_mut().screen.open_on(page);
    }

    fn disabled_because(harness: &mut Harness<'static>, label: &str, why: &str) {
        assert!(harness.get_by_label(label).accesskit_node().is_disabled(), "{label}");
        harness.get_by_label(label).hover();
        harness.run_steps(8);
        assert!(harness.query_by_label(why).is_some(), "{label}: {why}");
    }

    #[test]
    fn your_terms_show_the_profile_values_read_only() {
        let state = state();
        on(&state, Page::Terms);
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("Freelance only");
        harness.get_by_label("EUR 700 a day");
        harness.get_by_label("From Namur (by car).");
        harness.get_by_label("Ideal");
        harness.get_by_label("Namur, Charleroi");
        harness.get_by_label("Under 45 minutes.");
        harness.get_by_label("Too far");
        harness.get_by_label("Over 2 hours");
        harness.get_by_label("From profile");
        assert!(harness.query_by_role(egui::accesskit::Role::TextInput).is_some(), "only the search is a field");
        assert_eq!(harness.query_all_by_role(egui::accesskit::Role::TextInput).count(), 1);

        assert!(harness.query_by_label("/setup --section goals").is_none());
        harness.get_by_label("Show the command").click();
        harness.run();
        harness.get_by_label("/setup --section goals");
        harness.get_by_label("Hide the command").click();
        harness.run();
        assert!(harness.query_by_label("/setup --section goals").is_none());

        harness.get_by_label("Edit in Profile").click();
        harness.run();
        assert_eq!(state.borrow().asked, Some(SettingsRequest::OpenProfile));
    }

    #[test]
    fn a_floor_not_stated_says_what_the_check_does_instead() {
        let state = state();
        state.borrow_mut().terms = terms(None);
        on(&state, Page::Terms);
        let mut harness = harness(state);
        harness.run();
        harness.get_by_label("Not stated");
        harness.get_by_label("Your profile does not state one, so every job's day-rate check reads Unclear instead of guessing.");
        harness.get_by_label("1 not stated");
        assert!(harness.query_by_label("EUR 700 a day").is_none());
    }

    #[test]
    fn a_profile_that_cannot_be_read_hides_your_terms_and_waits_to_update() {
        let state = state();
        let failed = crate::shared_kernel::LoadError { source: "01-candidate-profile.md".to_owned(), reason: "not UTF-8\u{202e} text".to_owned() };
        state.borrow_mut().terms.floor = TermPart::Failed(failed);
        on(&state, Page::Terms);
        let mut harness = harness(state);
        harness.run();
        harness.get_by_label("Your terms could not be read.");
        harness.get_by_label("01-candidate-profile.md: not UTF-8 text");
        harness.get_by_label("Jobs still reads every job you saved. Its day-rate check reads Unclear until the file is fixed.");
        harness.get_by_label("Your terms are hidden, not gone");
        harness.get_by_label("From Namur (by car). This part comes from another file, so it still shows.");
        harness.get_by_label("Namur, Charleroi");
        assert!(harness.query_by_label("Freelance only").is_none(), "the rate's part is hidden");
        harness.get_by_label("Hidden");
        assert!(!harness.get_by_label("Edit in Profile").accesskit_node().is_disabled());
        disabled_because(&mut harness, "Update with Claude…", crate::settings::application::PROFILE_UNREADABLE);
    }

    #[test]
    fn profile_text_shows_as_plain_text() {
        let state = state();
        let tier =
            CommuteTier { name: "**ideal**".to_owned(), places: vec!["Na\u{200b}mur".to_owned(), "[x](https://x.example)".to_owned()], note: String::new() };
        state.borrow_mut().terms.commute = TermPart::Read(Commute { origin: Some("from Na\u{202e}mur".to_owned()), tiers: vec![tier] });
        on(&state, Page::Terms);
        let mut harness = harness(state);
        harness.run();
        harness.get_by_label("**ideal**");
        harness.get_by_label("Namur, [x](https://x.example)");
        harness.get_by_label("From Namur.");
        assert_eq!(harness.query_all_by_role(egui::accesskit::Role::Link).count(), 1, "nothing from the profile is a link");
        harness.get_by_role_and_label(egui::accesskit::Role::Link, "Show the command");
    }

    #[test]
    fn with_no_folder_your_terms_wait_for_one() {
        let state = state();
        state.borrow_mut().no_folder = true;
        on(&state, Page::Terms);
        let mut harness = harness(state);
        harness.run();
        harness.get_by_label("Your terms show here once you choose your job search folder.");
        disabled_because(&mut harness, "Edit in Profile", "Choose a folder first.");
        disabled_because(&mut harness, "Update with Claude…", "Choose a folder first.");
    }

    #[test]
    fn the_queue_settings_save() {
        let state = state();
        on(&state, Page::Queue);
        let mut harness = harness(state.clone());
        harness.run();
        assert_eq!(harness.get_by_label("14 days").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        harness.get_by_label("30 days").click();
        harness.run();
        harness.get_by_label("Until Monday").click();
        harness.run();
        let queue = state.borrow().settings.preferences().queue.clone();
        assert_eq!((queue.coming_up, queue.later), (crate::settings::application::ComingUp::Month, crate::settings::application::LaterLasts::UntilMonday));
    }

    fn reasons(state: &Rc<RefCell<State>>) -> Vec<String> {
        state.borrow().settings.preferences().queue.skip_reasons.list().to_vec()
    }

    fn reason_field<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
        harness.get_by(|node| node.role() == egui::accesskit::Role::TextInput && node.label().as_deref() == Some("New skip reason"))
    }

    #[test]
    fn a_skip_reason_is_added_by_button_or_enter_and_removed() {
        let state = state();
        on(&state, Page::Queue);
        let mut harness = harness(state.clone());
        harness.run();
        disabled_because(&mut harness, "Add reason", "Type a reason first.");

        reason_field(&harness).focus();
        harness.run();
        reason_field(&harness).type_text("rate");
        harness.run();
        disabled_because(&mut harness, "Add reason", "That reason is already in the list.");

        reason_field(&harness).focus();
        harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        reason_field(&harness).type_text("Culture");
        harness.run();
        harness.get_by_label("Add reason").click();
        harness.run();
        assert_eq!(reasons(&state), ["Rate", "Scope", "Domain", "Location", "Culture"]);
        assert_eq!(reason_field(&harness).value().as_deref(), Some(""), "the field empties");

        reason_field(&harness).focus();
        harness.run();
        reason_field(&harness).type_text("Agency, not the client");
        harness.key_press(Key::Enter);
        harness.run();
        assert_eq!(reasons(&state).last().map(String::as_str), Some("Agency, not the client"));

        harness.get_by_label("Remove Scope").click();
        harness.run();
        assert_eq!(reasons(&state), ["Rate", "Domain", "Location", "Culture", "Agency, not the client"]);
    }

    #[test]
    fn a_full_list_says_so_and_waits_for_a_removal() {
        let state = state();
        for n in 0..4 {
            state.borrow_mut().settings.add_skip_reason(&format!("Extra {n}")).unwrap();
        }
        on(&state, Page::Queue);
        let mut harness = harness(state.clone());
        harness.run();
        let full = "The list holds 8 reasons. Remove one first.";
        assert!(harness.query_by_label(full).is_some(), "the page says the list is full, with no hover");
        assert!(reason_field(&harness).accesskit_node().is_disabled(), "the field waits too");
        assert!(harness.get_by_label("Add reason").accesskit_node().is_disabled());
        harness.get_by_label("Add reason").hover();
        harness.run_steps(8);
        assert_eq!(harness.query_all_by_label(full).count(), 2, "the hover says it too");

        harness.get_by_label("Remove Extra 0").click();
        harness.run();
        assert!(harness.query_by_label(full).is_none());
        assert!(!reason_field(&harness).accesskit_node().is_disabled());
    }

    #[test]
    fn tab_goes_from_the_reason_field_to_add_reason() {
        let state = state();
        on(&state, Page::Queue);
        let mut harness = harness(state);
        harness.run();
        reason_field(&harness).focus();
        harness.run();
        reason_field(&harness).type_text("Culture");
        harness.run();
        harness.key_press(Key::Tab);
        harness.run();
        assert!(harness.get_by_label("Add reason").accesskit_node().is_focused());
    }

    /// Your terms, with each part stated, not stated or failed, draws every line its search matches, and none it does not.
    #[test]
    fn a_search_matches_only_what_the_terms_page_draws() {
        let failed = |file: &str| crate::shared_kernel::LoadError::new(file, "denied");
        let floors =
            [("floor", TermPart::Read(Some(700))), ("no floor", TermPart::Read(None)), ("profile failed", TermPart::Failed(failed("01-candidate-profile.md")))];
        let commutes = [
            ("tiers", terms(None).commute),
            ("no tiers", TermPart::Read(Commute::default())),
            ("evaluation failed", TermPart::Failed(failed("04-job-evaluation.md"))),
        ];
        for (floor_case, floor) in &floors {
            for (commute_case, commute) in &commutes {
                let case = format!("{floor_case}, {commute_case}");
                let state = state();
                state.borrow_mut().terms = YourTerms { floor: floor.clone(), commute: commute.clone() };
                on(&state, Page::Terms);
                let mut harness = harness(state.clone());
                harness.run();
                let drawn = crate::settings::application::drawn_index(
                    Page::Terms,
                    Drawn { folder_read: true, terms: terms_drawn(TermsState::Read(&state.borrow().terms)) },
                );
                for &line in index(Page::Terms) {
                    let shown = harness.query_all_by_label(line).next().is_some();
                    assert_eq!(shown, drawn.contains(&line), "{case}: {line}");
                }
            }
        }
    }

    #[test]
    fn commute_tiers_not_stated_say_what_the_check_still_does() {
        let state = state();
        state.borrow_mut().terms.commute = TermPart::Read(Commute::default());
        on(&state, Page::Terms);
        let mut harness = harness(state);
        harness.run();
        harness.get_by_label(
            "Your profile does not state them, so the commute check reads Unclear for a job that is not fully remote, unless the location check ruled it out.",
        );
    }

    #[test]
    fn a_reason_field_takes_forty_characters() {
        let state = state();
        on(&state, Page::Queue);
        let mut harness = harness(state);
        harness.run();
        reason_field(&harness).focus();
        harness.run();
        reason_field(&harness).type_text(&"x".repeat(45));
        harness.run();
        assert_eq!(reason_field(&harness).value().map(|text| text.chars().count()), Some(40));
    }

    #[test]
    fn the_jobs_view_filters_save_and_the_count_follows() {
        let state = state();
        on(&state, Page::JobsView);
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("No filter is on, so Jobs shows all 14 jobs.");
        let floor = harness.get_by_label("Day rate under your floor");
        assert_eq!(floor.accesskit_node().description().as_deref(), Some("EUR 700 a day, from Your terms."));
        floor.click();
        harness.run();
        harness.get_by_label("Too far to commute").click();
        harness.run();
        harness.get_by_label("Language does not match").click();
        harness.run();
        harness.get_by_label("Hide jobs the safety check flagged").click();
        harness.run();
        harness.get_by_label("Hide jobs scored under 60").click();
        harness.run();
        let view = state.borrow().settings.preferences().jobs_view;
        assert!(view.below_floor && view.too_far && view.language && view.flagged && view.under_score);

        state.borrow_mut().jobs = Some((3, 14));
        harness.run();
        harness.get_by_label("Jobs hides 3 of 14 jobs with these filters. Hidden jobs stay in your folder, and Jobs shows how many it hides.");
        harness.get_by_label("3 hidden");
    }

    #[test]
    fn a_typed_score_saves_once_it_reads_from_0_to_100() {
        let state = state();
        on(&state, Page::JobsView);
        let mut harness = harness(state.clone());
        harness.run();
        fn field<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
            harness.get_by(|node| node.role() == egui::accesskit::Role::TextInput && node.label().as_deref() == Some("Hide jobs scored under"))
        }
        field(&harness).focus();
        harness.run();
        harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        field(&harness).type_text("75");
        harness.run();
        assert_eq!(state.borrow().settings.preferences().jobs_view.score, 75);
        harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        field(&harness).type_text("150");
        harness.run();
        assert_eq!(state.borrow().settings.preferences().jobs_view.score, 75, "past 100 saves nothing");
        assert_eq!(field(&harness).value().as_deref(), Some("150"), "and stays as typed while the field has the focus");
    }

    #[test]
    fn a_search_finds_the_queue_and_jobs_view() {
        let state = state();
        state.borrow_mut().screen.query = "Skip reasons".to_owned();
        let mut harness = harness(state.clone());
        harness.run();
        assert_eq!(state.borrow().screen.page, Page::Queue);
        state.borrow_mut().screen.query = "hide jobs scored under".to_owned();
        harness.run();
        assert_eq!(state.borrow().screen.page, Page::JobsView);
    }

    #[test]
    fn the_new_pages_say_their_state_in_the_rail() {
        let state = state();
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("From profile");
        assert!(harness.query_by_label_contains(" hidden").is_none(), "no filter, no word");
        harness.get_by_label("Claude sign-in");
        harness.get_by_label("Allow and note");
        harness.get_by_label("212 MB");
        harness.get_by_label("0.1.0");
        assert!(harness.query_by_label_contains("Cap ").is_none(), "no cap, no word");

        state.borrow_mut().settings.set_cap_on(true);
        state.borrow_mut().settings.set_cap_amount(crate::settings::application::Dollars::parse("60"));
        state.borrow_mut().settings.set_permissions(crate::settings::application::PermissionMode::Ask);
        harness.run();
        harness.get_by_label("Cap $60");
        harness.get_by_label("Ask first");

        let reached = state.borrow().settings.preferences().cap.reached(60.4, today()).unwrap();
        state.borrow_mut().cap = CapCheck::Reached(reached);
        harness.run();
        harness.get_by_label("Cap reached");
    }

    #[test]
    fn the_about_page_shows_its_own_heading() {
        let state = state();
        on(&state, Page::About);
        let mut harness = harness(state);
        harness.run();
        harness.get_by_label("About Jobdesk");
        harness.get_by_label("2.1.4");
        assert!(harness.get_by_label("Show licences").accesskit_node().is_disabled(), "there is no licence file to show yet");
        assert!(!harness.get_by_label("Open the changelog").accesskit_node().is_disabled());
    }

    #[test]
    fn how_tasks_sign_in_opens_providers() {
        let state = state();
        on(&state, Page::About);
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("How tasks sign in");
        harness.get_by_label("Open Providers").click();
        harness.run();
        assert_eq!(state.borrow().screen.page, Page::Providers);
        assert_eq!(state.borrow().asked, None, "a jump inside Settings asks the shell for nothing");
        harness.get_by_label(Page::Providers.lead());
    }

    #[test]
    fn a_search_finds_how_tasks_sign_in() {
        let about = |query: &str| search(query, Drawn::folder(true)).is_some_and(|hits| hits.pages.iter().any(|&(page, _)| page == Page::About));
        assert!(about("How tasks sign in"));
        assert!(about("optional API key"));
    }

    #[test]
    fn a_typed_budget_saves_once_it_reads_and_empty_saves_none() {
        let state = state();
        on(&state, Page::Defaults);
        let mut harness = harness(state.clone());
        harness.run();
        fn field<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
            harness.get_by(|node| node.role() == egui::accesskit::Role::TextInput && node.label().as_deref() == Some("Spend at most, USD"))
        }
        field(&harness).focus();
        harness.run();
        field(&harness).type_text("4.5");
        harness.run();
        let budget = state.borrow().settings.preferences().run_defaults.budget;
        assert_eq!(budget, crate::settings::application::Dollars::parse("4.50"));
        assert_eq!(field(&harness).value().as_deref(), Some("4.5"), "the field keeps what was typed");

        harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        harness.key_press(Key::Backspace);
        harness.run();
        assert_eq!(state.borrow().settings.preferences().run_defaults.budget, None, "empty starts each command at its suggestion");
    }

    #[test]
    fn ask_me_before_each_tool_saves_ask() {
        let state = state();
        on(&state, Page::Permissions);
        let mut harness = harness(state.clone());
        harness.run();
        let ask = harness.get_by_label("Ask me before each tool");
        assert_eq!(ask.accesskit_node().role(), egui::accesskit::Role::RadioButton);
        ask.click();
        harness.run();
        assert_eq!(state.borrow().settings.preferences().permissions, crate::settings::application::PermissionMode::Ask);
        harness.get_by_label("Allow and note in the activity").click();
        harness.run();
        assert_eq!(state.borrow().settings.preferences().permissions, crate::settings::application::PermissionMode::AllowAndNote);
    }

    fn clear_button<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
        harness.get_by_role_and_label(egui::accesskit::Role::Button, "Clear tray decisions")
    }

    #[test]
    fn clear_tray_decisions_waits_with_nothing_to_clear() {
        let state = state();
        on(&state, Page::Data);
        state.borrow_mut().decisions = Some(0);
        let mut harness = harness(state.clone());
        harness.run();
        // Each reason shows on hover over the disabled button.
        let waits_because = |harness: &mut Harness<'static>, why: &str| {
            assert!(clear_button(harness).accesskit_node().is_disabled(), "{why}");
            clear_button(harness).hover();
            harness.run_steps(8);
            assert!(harness.query_by_label(why).is_some(), "{why}");
        };
        waits_because(&mut harness, "No decisions to clear.");

        state.borrow_mut().decisions = None;
        harness.run();
        waits_because(&mut harness, "Today's decisions are not available.");
        assert!(harness.query_by_label("No decisions to clear.").is_none(), "a tray that could not be read is not an empty one");
        clear_button(&harness).click();
        harness.run();
        assert!(harness.query_all_by_label_contains("tray decisions?").next().is_none(), "nothing asks");

        state.borrow_mut().no_folder = true;
        harness.run();
        waits_because(&mut harness, "Choose a folder first.");
    }

    /// A command's budget saves only an amount. Text that is not one stays in the field while it has the focus, and
    /// the saved budget stays as it was.
    #[test]
    fn a_command_budget_that_does_not_read_keeps_the_saved_one() {
        let mut settings = Settings::unsaved();
        settings.add_command(crate::runs::application::CATALOG.iter().find(|spec| spec.name == "/rank").unwrap());
        let saved = settings.preferences().run_defaults.commands[0].budget;
        let state = state_with(settings);
        on(&state, Page::Defaults);
        let mut harness = harness(state.clone());
        harness.run();
        fn field<'a>(harness: &'a Harness<'static>) -> egui_kittest::Node<'a> {
            harness.get_by(|node| node.role() == egui::accesskit::Role::TextInput && node.label().as_deref() == Some("Spend at most for /rank"))
        }
        field(&harness).focus();
        harness.run();
        harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        field(&harness).type_text("abc");
        harness.run();
        assert_eq!(field(&harness).value().as_deref(), Some("abc"), "the typed text stays while the field has the focus");
        assert_eq!(state.borrow().settings.preferences().run_defaults.commands[0].budget, saved, "the saved budget stays");

        harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        field(&harness).type_text("2.5");
        harness.run();
        assert_eq!(state.borrow().settings.preferences().run_defaults.commands[0].budget, crate::settings::application::Dollars::parse("2.50").unwrap());
    }

    #[test]
    fn about_says_checking_while_the_tool_check_runs() {
        let mut settings = Settings::unsaved();
        let mut screen = SettingsScreen::default();
        let mut providers = memory_providers(None);
        screen.open_on(Page::About);
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(setup).build_ui(move |ui| {
            let view = SettingsView {
                settings: &mut settings,
                folder: None,
                folder_read: false,
                runtime: RuntimeWord::None,
                folder_wait: None,
                usage: UsageState::NoFolder,
                today: today(),
                cap: CapCheck::Open,
                data: None,
                sizes: None,
                decisions: None,
                version: "0.1.0",
                claude: ClaudeVersion::Checking,
                providers: &mut providers,
                last_task: None,
                task_running: false,
                task_working: false,
                now: now(),
                terms: TermsState::NoFolder,
                jobs: None,
            };
            show(ui, &mut screen, view, &mut |_, _, _| {});
        });
        harness.run();
        harness.get_by_label("Checking…");
        assert!(harness.query_by_label("Not found").is_none(), "a check still running never says Claude Code is missing");
    }

    #[test]
    fn clear_tray_decisions_asks_first_and_esc_keeps_them() {
        let state = state();
        on(&state, Page::Data);
        let mut harness = harness(state.clone());
        harness.run();
        clear_button(&harness).click();
        harness.run();
        let asking = |harness: &Harness<'static>| harness.query_all_by_label("Clear 214 tray decisions?").next().is_some();
        assert!(asking(&harness));
        assert!(harness.get_by_label("Keep them").accesskit_node().is_focused(), "Keep them is the default");
        harness.key_press(Key::Escape);
        harness.run();
        assert!(!asking(&harness));
        assert_eq!(state.borrow().asked, None, "Esc keeps them, and Settings stays");

        clear_button(&harness).click();
        harness.run();
        harness.get_by_label("Clear decisions").click();
        harness.run();
        assert_eq!(state.borrow().asked, Some(SettingsRequest::ClearDecisions));
    }

    #[test]
    fn show_in_finder_asks_for_the_data_folder() {
        let state = state();
        on(&state, Page::Data);
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("~/Library/Application Support/Jobdesk");
        harness.get_by_label("Run logs 211.8 MB, tray decisions 18 KB.");
        harness.get_by_label(SHOW_FOLDER).click();
        harness.run();
        assert_eq!(state.borrow().asked, Some(SettingsRequest::ShowDataFolder));
    }

    #[test]
    fn a_command_row_adds_and_removes() {
        let state = state();
        on(&state, Page::Defaults);
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("Add a command").click();
        harness.run();
        harness.get_by_label("/rank").click();
        harness.run();
        let commands = |state: &Rc<RefCell<State>>| -> Vec<String> {
            state.borrow().settings.preferences().run_defaults.commands.iter().map(|row| row.command.clone()).collect()
        };
        assert_eq!(commands(&state), ["/rank"]);
        harness.get_by_label("Model for /rank");
        harness.get_by_label("Remove /rank").click();
        harness.run();
        assert!(commands(&state).is_empty());
    }

    #[test]
    fn runtime_status_is_drawn_by_its_owner() {
        let state = state();
        let mut harness = harness(state);
        harness.run();
        harness.get_by_label("Runtime status").click();
        harness.run();
        harness.get_by_label("Runtime status body");
    }

    #[test]
    fn a_moved_zoom_saves_the_text_size() {
        let ctx = egui::Context::default();
        let mut settings = Settings::unsaved();
        follow_zoom(&ctx, &mut settings);
        assert_eq!(settings.preferences().text_size, TextSize::default());
        ctx.options_mut(|options| options.zoom_factor = 1.1);
        follow_zoom(&ctx, &mut settings);
        assert_eq!(settings.preferences().text_size.percent(), 110);
    }

    fn on_usage(state: &Rc<RefCell<State>>) {
        state.borrow_mut().screen.open_on(Page::Usage);
    }

    #[test]
    fn the_rail_says_this_months_spend_or_no_tasks_yet() {
        let mut spent = harness(state());
        spent.run();
        spent.get_by_label("$41.20 in Oct");

        let state = state();
        state.borrow_mut().report = UsageReport::default();
        let mut none = harness(state);
        none.run();
        none.get_by_label("No tasks yet");
    }

    #[test]
    fn while_the_logs_are_read_the_rail_says_nothing_and_the_lead_says_so() {
        let state = state();
        state.borrow_mut().reading = true;
        on_usage(&state);
        let mut harness = harness(state);
        harness.run();
        assert!(harness.query_by_label_contains(" in Oct").is_none());
        harness.get_by_label(usage::READING);
    }

    #[test]
    fn choosing_a_period_changes_the_spent_figure() {
        let state = state();
        on_usage(&state);
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("$41.20");
        assert_eq!(harness.get_by_label("October").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        harness.get_by_label("All time").click();
        harness.run();
        harness.get_by_label("$50.20");
        assert_eq!(state.borrow().screen.period, Period::All);
        assert!(harness.query_by_label("$41.20").is_none());
    }

    /// October's $41.20 fills a bar against a $60 cap, and only the month's figure has one.
    #[test]
    fn the_months_spend_measures_against_the_cap() {
        const CAP_NOTE: &str = "of your $60.00 monthly cap";
        let state = state();
        on_usage(&state);
        let mut harness = harness(state.clone());
        harness.run();
        assert!(harness.query_by_label(CAP_NOTE).is_none(), "no cap, no bar");
        harness.get_by_label("in October");

        state.borrow_mut().settings.set_cap_on(true);
        state.borrow_mut().settings.set_cap_amount(crate::settings::application::Dollars::parse("60"));
        harness.run();
        harness.get_by_label(CAP_NOTE);
        assert!(harness.query_by_label("in October").is_none());
        for period in ["90 days", "This year", "All time"] {
            harness.get_by_label(period).click();
            harness.run();
            assert!(harness.query_by_label(CAP_NOTE).is_none(), "{period}");
        }
    }

    #[test]
    fn per_application_sent_shows_only_when_something_was_sent() {
        let state = state();
        on_usage(&state);
        let mut sent = harness(state.clone());
        sent.run();
        sent.get_by_label("Per application sent");
        sent.get_by_label("$20.60");
        sent.get_by_label("You marked 2 applications sent in October. Every task's cost counts, not only drafting.");

        state.borrow_mut().pipeline = Pipeline::default();
        let mut none = harness(state);
        none.run();
        assert!(none.query_by_label("Per application sent").is_none());
        assert!(none.query_by_label("Open Applications").is_none());
    }

    #[test]
    fn open_applications_asks_the_shell() {
        let state = state();
        on_usage(&state);
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("Open Applications").click();
        harness.run();
        assert_eq!(state.borrow().asked, Some(SettingsRequest::OpenApplications));
    }

    #[test]
    fn with_no_tasks_new_task_asks_for_the_launcher() {
        let state = state();
        state.borrow_mut().report = UsageReport::default();
        on_usage(&state);
        let mut harness = harness(state.clone());
        harness.run();
        harness.get_by_label("Usage counts the tasks Jobdesk runs, from its own logs. Once your first task ends, its cost, tokens and model show here.");
        assert!(!harness.get_by_label("Find new jobs…").accesskit_node().is_disabled());
        harness.get_by_label("New task…").click();
        harness.run();
        assert_eq!(state.borrow().asked, Some(SettingsRequest::NewTask));
    }

    #[test]
    fn with_no_folder_usage_waits_for_one() {
        let mut settings = Settings::unsaved();
        let mut screen = SettingsScreen::default();
        let mut providers = memory_providers(None);
        screen.open_on(Page::Usage);
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(setup).build_ui(move |ui| {
            let view = SettingsView {
                settings: &mut settings,
                folder: None,
                folder_read: false,
                runtime: RuntimeWord::None,
                folder_wait: None,
                usage: UsageState::NoFolder,
                today: today(),
                cap: CapCheck::Open,
                data: None,
                sizes: None,
                decisions: None,
                version: "0.1.0",
                claude: ClaudeVersion::NotFound,
                providers: &mut providers,
                last_task: None,
                task_running: false,
                task_working: false,
                now: now(),
                terms: TermsState::NoFolder,
                jobs: None,
            };
            show(ui, &mut screen, view, &mut |_, _, _| {});
        });
        harness.run();
        // The rail's word is blank, so the only "No tasks yet" is the page's.
        assert_eq!(harness.query_all_by_label("No tasks yet").count(), 1);
        assert!(harness.get_by_label("Find new jobs…").accesskit_node().is_disabled());
        assert!(harness.get_by_label("New task…").accesskit_node().is_disabled());
    }

    #[test]
    fn a_narrow_page_puts_by_model_under_by_command() {
        let state = state();
        on_usage(&state);
        let mut harness = harness(state.clone());
        harness.run();
        let (command, model) = (harness.get_by_label("By command").rect(), harness.get_by_label("By model").rect());
        assert!((command.top() - model.top()).abs() < 1.0, "side by side at 1280");

        let mut harness = harness_at(state, 1040.0);
        harness.run();
        let last_command = harness.get_by_label("/rank").rect();
        let model = harness.get_by_label("By model").rect();
        assert!(model.top() > last_command.bottom(), "By model sits under the By command table");
        assert!((model.left() - harness.get_by_label("By command").rect().left()).abs() < 1.0);
    }
}
