use egui::{Align, ComboBox, Frame, Key, Label, Layout, Margin, Modifiers, Popup, Rect, ScrollArea, ThemePreference, Ui, UiBuilder, pos2, vec2};

use crate::settings::application::{Group, IssueKind, Page, Preferences, RuntimeWord, SearchHits, Settings, StartScreen, TextSize, ThemeChoice, index, search};
use crate::ui_kit::tokens::{BODY_PADDING, FIELD_HEIGHT, GUTTER, HEADER_PADDING, INSET, PICK_PADDING, PICK_ROW_HEIGHT, RUN_SIDE, SETTING_ROW_PADDING};
use crate::ui_kit::{
    Ink, Tone, Type, back_pressed, copyable, divider, enabled_when, hand, pick_row, rail_title, rich, set_reduced_motion, setting_check, setting_row, setting_section, settings_page, search_field, shortcut_button, slip,
    small_button, toggle_group,
};

/// How wide Open on's combo box is.
const OPEN_ON_WIDTH: f32 = 180.0;

const SHOW_FOLDER: &str = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in Explorer" };

/// Why Change folder waits: a new folder would leave the running task behind.
const FOLDER_BUSY: &str = "A task is running. Change the folder once it ends.";

/// What stays between frames: the page open and the search.
#[derive(Default)]
pub struct SettingsScreen {
    page: Page,
    query: String,
}

impl SettingsScreen {
    /// A page not built yet cannot open, so General stays.
    pub fn open_on(&mut self, page: Page) {
        if page.built() {
            self.page = page;
        }
    }

    /// The page open, which Settings reopens on.
    pub fn page(&self) -> Page {
        self.page
    }
}

pub struct SettingsView<'a> {
    pub settings: &'a mut Settings,
    /// The job search folder as the user reads it, if one is chosen.
    pub folder: Option<&'a str>,
    /// Whether the folder is open and read, so Runtime status draws its sections and a search can match them.
    pub folder_read: bool,
    pub runtime: RuntimeWord,
    /// Whether a task is running, so the folder cannot change under it.
    pub task_live: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRequest {
    Back,
    ChangeFolder,
    ShowFolder,
}

/// The Settings screen: Back to app and the rail at the left, the open page on paper beside them.
/// `other_page` draws the pages another feature owns, such as Runtime status, with the search query.
pub fn show(ui: &mut Ui, screen: &mut SettingsScreen, view: SettingsView, other_page: &mut dyn FnMut(&mut Ui, Page, &str)) -> Option<SettingsRequest> {
    let mut request = None;
    // Esc clears a search before anything else takes it, the field included.
    let ctx = ui.ctx().clone();
    let clears = !screen.query.is_empty() && !Popup::is_any_open(&ctx) && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    if clears {
        screen.query.clear();
    } else if back_pressed(&ctx) {
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
        Page::General => {
            let (_, asked) = settings_page(&mut page_ui, page.title(), page.lead(), |_| {}, |ui| general(ui, view.settings, view.folder, view.task_live, &query));
            request = asked.or(request);
        }
        Page::Appearance => {
            settings_page(&mut page_ui, page.title(), page.lead(), |_| {}, |ui| appearance(ui, view.settings, &query));
        }
        _ => other_page(&mut page_ui, page, &query),
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
        let hits = search(&screen.query, view.folder_read);
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
                let row = |ui: &mut Ui| pick_row(ui, on, |ui| rail_row(ui, page.title(), &word, ink));
                let picked = if page.built() { row(ui) } else { enabled_when(ui, false, "Not available yet.", row) };
                if picked.clicked() && page.built() {
                    screen.page = page;
                }
            }
        });
    }
}

/// The one word at a row's right, saying the page's state.
fn state_word(page: Page, view: &SettingsView) -> (String, Ink) {
    match page {
        _ if !page.built() => ("Later".to_owned(), Ink::Muted),
        Page::Appearance => (view.settings.preferences().theme.label().to_owned(), Ink::Muted),
        Page::Runtime => match view.runtime {
            RuntimeWord::None => (String::new(), Ink::Muted),
            RuntimeWord::Missing(n) => (format!("{n} missing"), Tone::Closed.into()),
            RuntimeWord::ToFill(n) => (format!("{n} to fill"), Tone::Interview.into()),
        },
        _ => (String::new(), Ink::Muted),
    }
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
fn general(ui: &mut Ui, settings: &mut Settings, folder: Option<&str>, task_live: bool, query: &str) -> Option<SettingsRequest> {
    let &[section, label, help, opens, open_on, open_help] = index(Page::General) else { return None };
    let mut request = None;
    setting_section(ui, section, None, query);
    setting_row(ui, label, help, query, |ui| match folder {
        Some(_) => {
            if enabled_when(ui, !task_live, FOLDER_BUSY, |ui| small_button(ui, "Change folder")).clicked() {
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
        let combo = ComboBox::from_id_salt("settings_open_on").width(OPEN_ON_WIDTH).selected_text(current.label()).show_index(ui, &mut at, StartScreen::ALL.len(), |index| StartScreen::ALL[index].label());
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

    struct State {
        screen: SettingsScreen,
        settings: Settings,
        asked: Option<SettingsRequest>,
    }

    fn setup(ctx: &egui::Context) {
        crate::ui_kit::install_fonts(ctx);
        crate::ui_kit::install_theme(ctx);
    }

    fn harness(state: Rc<RefCell<State>>) -> Harness<'static> {
        Harness::builder().with_size([1280.0, 820.0]).with_context_setup(setup).build_ui(move |ui| {
            let state = &mut *state.borrow_mut();
            let view = SettingsView { settings: &mut state.settings, folder: Some("~/Documents/ai-job-search"), folder_read: true, runtime: RuntimeWord::Missing(1), task_live: false };
            let asked = show(ui, &mut state.screen, view, &mut |ui, page, _| {
                ui.label(format!("{} body", page.title()));
            });
            if asked.is_some() {
                state.asked = asked;
            }
        })
    }

    fn state() -> Rc<RefCell<State>> {
        Rc::new(RefCell::new(State { screen: SettingsScreen::default(), settings: Settings::unsaved(), asked: None }))
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
        Rc::new(RefCell::new(State { screen: SettingsScreen::default(), settings, asked: None }))
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
    fn the_rail_lists_every_page_in_its_group_and_fades_the_later_ones() {
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
            assert_eq!(rows[0].accesskit_node().is_disabled(), !page.built(), "{}", page.title());
        }
        assert_eq!(harness.query_all_by_label("Later").count(), 9);
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
        harness.get_by_label("3 matches");
        harness.get_by_label("1 match");
        harness.get_by_label("4 settings match. Esc clears the search.");
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
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).with_context_setup(setup).build_ui(move |ui| {
            let view = SettingsView { settings: &mut settings, folder: None, folder_read: false, runtime: RuntimeWord::None, task_live: false };
            show(ui, &mut screen, view, &mut |_, _, _| {});
        });
        harness.run();
        harness.get_by_label("No folder chosen yet.");
        harness.get_by_label("Choose folder");
        assert!(harness.get_by_label(SHOW_FOLDER).accesskit_node().is_disabled());
    }

    #[test]
    fn every_indexed_line_shows_on_its_page() {
        for page in [Page::General, Page::Appearance] {
            let state = state();
            state.borrow_mut().screen.open_on(page);
            let mut harness = harness(state);
            harness.run();
            // A checkbox's help is its description, as its words are part of it.
            for &line in index(page) {
                let described = harness.query_all_by(|node| node.description().as_deref() == Some(line)).next().is_some();
                assert!(described || harness.query_all_by_label(line).next().is_some(), "{}: {line}", page.title());
            }
        }
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
}
