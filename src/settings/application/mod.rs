mod settings;

pub use settings::{IssueKind, RuntimeWord, SearchHits, Settings, search};

pub use crate::settings::domain::{Group, Page, Preferences, PreferencesStore, StartScreen, TextSize, ThemeChoice, drawn_index, index};
