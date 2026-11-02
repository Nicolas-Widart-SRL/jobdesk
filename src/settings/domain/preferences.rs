use crate::settings::domain::run_defaults::{PermissionMode, Retention, RunDefaults, SpendCap};
use crate::shared_kernel::LoadError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "System",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
        }
    }
}

/// The zoom of the whole window, in whole percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSize(u16);

impl TextSize {
    pub const STEPS: [u16; 5] = [90, 100, 110, 125, 150];
    pub const DEFAULT: u16 = 100;
    const LEAST: u16 = 50;
    const MOST: u16 = 300;

    pub fn percent(self) -> u16 {
        self.0
    }

    pub fn from_percent(percent: i64) -> Self {
        Self(percent.clamp(i64::from(Self::LEAST), i64::from(Self::MOST)) as u16)
    }

    pub fn factor(self) -> f32 {
        f32::from(self.0) / 100.0
    }

    /// Rounds to a whole percent, because a zoom shortcut can land between the steps.
    pub fn from_factor(factor: f32) -> Self {
        if factor.is_nan() {
            return Self::default();
        }
        Self((factor * 100.0).round().clamp(f32::from(Self::LEAST), f32::from(Self::MOST)) as u16)
    }

    pub fn label(self) -> String {
        format!("{}%", self.0)
    }
}

impl Default for TextSize {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

/// The main tabs the app can open on. Setup is left out, because it can leave the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartScreen {
    #[default]
    Today,
    Jobs,
    Applications,
    Insights,
    Tasks,
}

impl StartScreen {
    pub const ALL: [StartScreen; 5] = [StartScreen::Today, StartScreen::Jobs, StartScreen::Applications, StartScreen::Insights, StartScreen::Tasks];

    pub fn label(self) -> &'static str {
        match self {
            StartScreen::Today => "Today",
            StartScreen::Jobs => "Jobs",
            StartScreen::Applications => "Applications",
            StartScreen::Insights => "Insights",
            StartScreen::Tasks => "Tasks",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Preferences {
    pub theme: ThemeChoice,
    pub text_size: TextSize,
    pub reduce_motion: bool,
    pub open_on: StartScreen,
    pub run_defaults: RunDefaults,
    pub cap: SpendCap,
    pub permissions: PermissionMode,
    pub retention: Retention,
}

pub trait PreferencesStore {
    /// `None` when nothing was saved yet.
    fn load(&self) -> Result<Option<Preferences>, LoadError>;
    fn save(&self, preferences: &Preferences) -> Result<(), LoadError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zoom_factor_rounds_to_a_whole_percent() {
        assert_eq!(TextSize::from_factor(1.1), TextSize(110));
        assert_eq!(TextSize::from_factor(1.2549), TextSize(125));
        assert_eq!(TextSize::from_factor(1.0), TextSize::default());
        assert_eq!(TextSize(125).factor(), 1.25);
    }

    #[test]
    fn a_zoom_factor_stays_in_range() {
        assert_eq!(TextSize::from_factor(9.0), TextSize(300));
        assert_eq!(TextSize::from_factor(0.1), TextSize(50));
        assert_eq!(TextSize::from_factor(f32::NAN), TextSize::default());
        assert_eq!(TextSize::from_percent(999), TextSize(300));
        assert_eq!(TextSize::from_percent(-5), TextSize(50));
    }

    #[test]
    fn labels_show_the_percent() {
        assert_eq!(TextSize(110).label(), "110%");
        assert!(TextSize::STEPS.contains(&TextSize::DEFAULT));
    }

    #[test]
    fn the_defaults_are_what_the_app_did_before() {
        let preferences = Preferences::default();
        assert_eq!(
            (preferences.theme, preferences.text_size, preferences.reduce_motion, preferences.open_on),
            (ThemeChoice::System, TextSize(100), false, StartScreen::Today)
        );
        assert_eq!(preferences.run_defaults, RunDefaults::default());
        assert_eq!(preferences.cap.active(), None);
        assert_eq!(preferences.permissions, PermissionMode::AllowAndNote);
        assert_eq!(preferences.retention, Retention::Forever, "nothing is deleted until the user says so");
    }
}
