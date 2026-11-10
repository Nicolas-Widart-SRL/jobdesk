use crate::onboarding::domain::{Helper, HelperProbe};

/// Asks each of a helper's programs for its version. `version` gives the first line a program printed, or `None` when
/// it cannot run. The wiring gives it the tool check's own prober, so both look the same way: on the resolved `PATH`,
/// helper folders first, and never through a developer stub that would open a dialog.
pub struct ProgramProbe {
    version: fn(&str) -> Option<String>,
}

impl ProgramProbe {
    pub fn new(version: fn(&str) -> Option<String>) -> Self {
        Self { version }
    }
}

impl HelperProbe for ProgramProbe {
    fn is_here(&self, helper: Helper) -> bool {
        helper.programs().iter().all(|program| (self.version)(program).is_some_and(|line| helper.accepts(&line)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn here(helper: Helper, version: fn(&str) -> Option<String>) -> bool {
        ProgramProbe::new(version).is_here(helper)
    }

    #[test]
    fn a_helper_is_here_when_each_of_its_programs_runs() {
        let all = |_: &str| Some("Python 3.14.9".to_owned());
        assert!(here(Helper::Python, all));
        assert!(here(Helper::Bun, all));
        assert!(here(Helper::Latex, all));
        let none = |_: &str| None;
        assert!(!here(Helper::Bun, none));
        let only_lua = |program: &str| (program == "lualatex").then(|| "LuaHBTeX, Version 1.22.0".to_owned());
        assert!(!here(Helper::Latex, only_lua), "the cover letter needs xelatex too");
    }

    #[test]
    fn a_python_older_than_the_tools_need_is_missing() {
        assert!(!here(Helper::Python, |_| Some("Python 3.9.6".to_owned())), "the Command Line Tools' Python");
        assert!(here(Helper::Python, |_| Some("Python 3.10.12".to_owned())));
    }
}
