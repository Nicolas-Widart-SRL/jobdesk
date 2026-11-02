use crate::settings::domain::{Page, Preferences, PreferencesStore, StartScreen, TextSize, ThemeChoice, index};
use crate::shared_kernel::LoadError;

/// The user's preferences. Each change is saved at once, and a failure to read or save is kept to show.
pub struct Settings {
    store: Option<Box<dyn PreferencesStore>>,
    preferences: Preferences,
    issue: Option<LoadError>,
}

impl Settings {
    /// A file that cannot be read gives the defaults, and the next change replaces it.
    pub fn open(store: Box<dyn PreferencesStore>) -> Self {
        let (preferences, issue) = match store.load() {
            Ok(found) => (found.unwrap_or_default(), None),
            Err(error) => (Preferences::default(), Some(error)),
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

    pub fn issue(&self) -> Option<&LoadError> {
        self.issue.as_ref()
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
        self.issue = store.save(&self.preferences).err();
    }
}

/// The pages a search matches, each with how many of its lines match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHits {
    pub pages: Vec<(Page, usize)>,
    pub total: usize,
}

/// `None` for a blank query. Case does not matter, and every index line counts once.
pub fn search(query: &str) -> Option<SearchHits> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let pages: Vec<(Page, usize)> = Page::ALL
        .iter()
        .map(|&page| (page, index(page).iter().filter(|line| line.to_lowercase().contains(&needle)).count()))
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

    fn settings(shared: &Rc<Shared>) -> Settings {
        Settings::open(Box::new(Fake(shared.clone())))
    }

    #[test]
    fn nothing_saved_gives_the_defaults() {
        let shared = Rc::new(Shared::default());
        let settings = settings(&shared);
        assert_eq!(*settings.preferences(), Preferences::default());
        assert!(settings.issue().is_none());
        assert!(shared.saved.borrow().is_empty());
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
    fn a_load_error_gives_the_defaults_and_an_issue() {
        let shared = Rc::new(Shared::default());
        *shared.load.borrow_mut() = Some(Err(LoadError::new("settings.json", "bad")));
        let settings = settings(&shared);
        assert_eq!(*settings.preferences(), Preferences::default());
        assert_eq!(settings.issue().map(|issue| issue.source.as_str()), Some("settings.json"));
    }

    #[test]
    fn a_save_error_keeps_the_value_and_records_an_issue() {
        let shared = Rc::new(Shared::default());
        shared.fail_save.set(true);
        let mut settings = settings(&shared);
        settings.set_reduce_motion(true);
        assert!(settings.preferences().reduce_motion);
        assert!(settings.issue().is_some());
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
        let hits = search("FOLDER").unwrap();
        assert_eq!(hits.pages, vec![(Page::General, 3), (Page::Runtime, 1)]);
        assert_eq!(hits.total, 4);
    }

    #[test]
    fn a_blank_search_is_none_and_a_miss_has_no_pages() {
        assert_eq!(search("  "), None);
        assert_eq!(search(""), None);
        let miss = search("zzz").unwrap();
        assert!(miss.pages.is_empty());
        assert_eq!(miss.total, 0);
    }

    #[test]
    fn a_search_trims_the_query() {
        assert_eq!(search("  motion ").unwrap().pages, vec![(Page::Appearance, 2)]);
    }
}
