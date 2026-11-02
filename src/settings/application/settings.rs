use crate::settings::domain::{Page, Preferences, PreferencesStore, StartScreen, TextSize, ThemeChoice, drawn_index};
use crate::shared_kernel::LoadError;

/// What went wrong with the settings file, which the rail says in its own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    /// It could not be read at launch, so the defaults are in use until the next change replaces it.
    Read,
    /// The last change could not be saved, so it lasts until Jobdesk closes.
    Save,
}

/// The user's preferences. Each change is saved at once, and a failure to read or save is kept to show.
pub struct Settings {
    store: Option<Box<dyn PreferencesStore>>,
    preferences: Preferences,
    issue: Option<(IssueKind, LoadError)>,
}

impl Settings {
    /// A file that cannot be read gives the defaults, and the next change replaces it.
    /// With no file yet, `first` is used and saved at once: what the window already restored, such as its theme and zoom.
    pub fn open(store: Box<dyn PreferencesStore>, first: Preferences) -> Self {
        let (preferences, issue) = match store.load() {
            Ok(Some(found)) => (found, None),
            Ok(None) => (first, store.save(&first).err().map(|error| (IssueKind::Save, error))),
            Err(error) => (Preferences::default(), Some((IssueKind::Read, error))),
        };
        Self { store: Some(store), preferences, issue }
    }

    /// The defaults, kept nowhere. For a window that has no data folder.
    pub fn unsaved() -> Self {
        Self { store: None, preferences: Preferences::default(), issue: None }
    }

    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    pub fn issue(&self) -> Option<(IssueKind, &LoadError)> {
        self.issue.as_ref().map(|(kind, error)| (*kind, error))
    }

    pub fn set_theme(&mut self, theme: ThemeChoice) {
        self.change(|preferences| &mut preferences.theme, theme);
    }

    pub fn set_text_size(&mut self, text_size: TextSize) {
        self.change(|preferences| &mut preferences.text_size, text_size);
    }

    pub fn set_reduce_motion(&mut self, reduce_motion: bool) {
        self.change(|preferences| &mut preferences.reduce_motion, reduce_motion);
    }

    pub fn set_open_on(&mut self, open_on: StartScreen) {
        self.change(|preferences| &mut preferences.open_on, open_on);
    }

    /// Saves only a changed value. The new value stays in use when the save fails.
    fn change<T: PartialEq>(&mut self, field: impl Fn(&mut Preferences) -> &mut T, value: T) {
        let slot = field(&mut self.preferences);
        if *slot == value {
            return;
        }
        *slot = value;
        let Some(store) = &self.store else { return };
        self.issue = store.save(&self.preferences).err().map(|error| (IssueKind::Save, error));
    }
}

/// The pages a search matches, each with how many of its lines match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHits {
    pub pages: Vec<(Page, usize)>,
    pub total: usize,
}

/// `None` for a blank query. Case does not matter, and every line the page draws counts once.
/// `folder_read` says whether the folder is open and read, which decides what Runtime status draws.
pub fn search(query: &str, folder_read: bool) -> Option<SearchHits> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let pages: Vec<(Page, usize)> = Page::ALL
        .iter()
        .map(|&page| (page, drawn_index(page, folder_read).iter().filter(|line| line.to_lowercase().contains(&needle)).count()))
        .filter(|&(_, count)| count > 0)
        .collect();
    let total = pages.iter().map(|&(_, count)| count).sum();
    Some(SearchHits { pages, total })
}

/// The one word the rail shows beside Runtime status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeWord {
    None,
    Missing(usize),
    ToFill(usize),
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;

    #[derive(Default)]
    struct Shared {
        saved: RefCell<Vec<Preferences>>,
        load: RefCell<Option<Result<Option<Preferences>, LoadError>>>,
        fail_save: Cell<bool>,
    }

    struct Fake(Rc<Shared>);

    impl PreferencesStore for Fake {
        fn load(&self) -> Result<Option<Preferences>, LoadError> {
            self.0.load.borrow_mut().take().unwrap_or(Ok(None))
        }

        fn save(&self, preferences: &Preferences) -> Result<(), LoadError> {
            if self.0.fail_save.get() {
                return Err(LoadError::new("settings.json", "disk full"));
            }
            self.0.saved.borrow_mut().push(*preferences);
            Ok(())
        }
    }

    /// Opened over a file that holds the defaults, so the first save is a change's.
    fn settings(shared: &Rc<Shared>) -> Settings {
        if shared.load.borrow().is_none() {
            *shared.load.borrow_mut() = Some(Ok(Some(Preferences::default())));
        }
        Settings::open(Box::new(Fake(shared.clone())), Preferences::default())
    }

    #[test]
    fn nothing_saved_takes_and_saves_what_the_window_restored() {
        let shared = Rc::new(Shared::default());
        let restored = Preferences { theme: ThemeChoice::Dark, text_size: TextSize::from_percent(125), ..Preferences::default() };
        let settings = Settings::open(Box::new(Fake(shared.clone())), restored);
        assert_eq!(*settings.preferences(), restored);
        assert!(settings.issue().is_none());
        assert_eq!(*shared.saved.borrow(), vec![restored]);
    }

    #[test]
    fn a_seed_that_cannot_be_saved_is_a_save_issue() {
        let shared = Rc::new(Shared::default());
        shared.fail_save.set(true);
        let settings = Settings::open(Box::new(Fake(shared.clone())), Preferences::default());
        assert_eq!(settings.issue().map(|(kind, _)| kind), Some(IssueKind::Save));
    }

    #[test]
    fn saved_preferences_come_back() {
        let shared = Rc::new(Shared::default());
        let stored = Preferences { theme: ThemeChoice::Dark, text_size: TextSize::from_percent(125), reduce_motion: true, open_on: StartScreen::Jobs };
        *shared.load.borrow_mut() = Some(Ok(Some(stored)));
        assert_eq!(*settings(&shared).preferences(), stored);
    }

    #[test]
    fn the_same_value_twice_saves_once() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        settings.set_theme(ThemeChoice::Dark);
        settings.set_theme(ThemeChoice::Dark);
        settings.set_text_size(TextSize::default());
        assert_eq!(shared.saved.borrow().len(), 1);
        assert_eq!(shared.saved.borrow()[0].theme, ThemeChoice::Dark);
    }

    #[test]
    fn every_setter_saves_its_value() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        settings.set_text_size(TextSize::from_percent(150));
        settings.set_reduce_motion(true);
        settings.set_open_on(StartScreen::Tasks);
        let saved = shared.saved.borrow();
        assert_eq!(saved.len(), 3);
        let last = saved[2];
        assert_eq!((last.text_size.percent(), last.reduce_motion, last.open_on), (150, true, StartScreen::Tasks));
    }

    #[test]
    fn a_read_error_gives_the_defaults_and_a_read_issue_until_a_change_saves() {
        let shared = Rc::new(Shared::default());
        *shared.load.borrow_mut() = Some(Err(LoadError::new("settings.json", "bad")));
        let restored = Preferences { theme: ThemeChoice::Dark, ..Preferences::default() };
        let mut settings = Settings::open(Box::new(Fake(shared.clone())), restored);
        assert_eq!(*settings.preferences(), Preferences::default(), "an unreadable file is not a missing one");
        assert!(shared.saved.borrow().is_empty(), "the file is left as it is until a change");
        assert_eq!(settings.issue().map(|(kind, issue)| (kind, issue.source.as_str())), Some((IssueKind::Read, "settings.json")));
        settings.set_theme(ThemeChoice::Light);
        assert!(settings.issue().is_none(), "the change replaced the file");
    }

    #[test]
    fn a_save_error_keeps_the_value_and_records_a_save_issue() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        shared.fail_save.set(true);
        settings.set_reduce_motion(true);
        assert!(settings.preferences().reduce_motion);
        assert_eq!(settings.issue().map(|(kind, _)| kind), Some(IssueKind::Save));
        shared.fail_save.set(false);
        settings.set_open_on(StartScreen::Insights);
        assert!(settings.issue().is_none());
    }

    #[test]
    fn unsaved_settings_change_without_a_store() {
        let mut settings = Settings::unsaved();
        settings.set_theme(ThemeChoice::Light);
        assert_eq!(settings.preferences().theme, ThemeChoice::Light);
        assert!(settings.issue().is_none());
    }

    #[test]
    fn a_search_counts_lines_per_page() {
        let hits = search("FOLDER", true).unwrap();
        assert_eq!(hits.pages, vec![(Page::General, 3), (Page::Runtime, 1)]);
        assert_eq!(hits.total, 4);
    }

    #[test]
    fn without_a_folder_runtime_status_matches_only_what_it_draws() {
        assert_eq!(search("sync", true).unwrap().pages, vec![(Page::Runtime, 1)]);
        assert!(search("sync", false).unwrap().pages.is_empty());
        assert_eq!(search("environment", false).unwrap().pages, vec![(Page::Runtime, 1)]);
    }

    #[test]
    fn a_blank_search_is_none_and_a_miss_has_no_pages() {
        assert_eq!(search("  ", true), None);
        assert_eq!(search("", true), None);
        let miss = search("zzz", true).unwrap();
        assert!(miss.pages.is_empty());
        assert_eq!(miss.total, 0);
    }

    #[test]
    fn a_search_trims_the_query() {
        assert_eq!(search("  motion ", true).unwrap().pages, vec![(Page::Appearance, 2)]);
    }
}
