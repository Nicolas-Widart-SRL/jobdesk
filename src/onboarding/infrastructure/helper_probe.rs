use std::ffi::OsStr;
use std::path::Path;

use crate::onboarding::domain::{Helper, HelperProbe};

/// What a program printed: `None` when it cannot run, failed or took too long.
type Printed = Option<String>;

/// Asks each of a helper's programs for its version, then has the program beside it look up the files the helper
/// needs. `version` gives the first line a program printed. `beside` gives all a program printed when run, with the
/// given arguments, from the folder of another program or of the file it links to, so both belong to one install. The
/// wiring gives both the tool check's own prober, so they look the same way: on the resolved `PATH`, helper folders
/// first, and never through a developer stub that would open a dialog.
pub struct ProgramProbe {
    version: fn(&str) -> Printed,
    beside: fn(&str, &str, &[&str]) -> Printed,
}

impl ProgramProbe {
    pub fn new(version: fn(&str) -> Printed, beside: fn(&str, &str, &[&str]) -> Printed) -> Self {
        Self { version, beside }
    }

    /// Whether `finder`, beside `program`, finds every one of `files`: each must end a line it printed.
    fn finds(&self, program: &str, finder: &str, files: &[&str]) -> bool {
        let Some(printed) = (self.beside)(program, finder, files) else { return false };
        let found: Vec<_> = printed.lines().filter_map(|line| Path::new(line.trim()).file_name()).collect();
        files.iter().all(|file| found.contains(&OsStr::new(file)))
    }
}

impl HelperProbe for ProgramProbe {
    fn is_here(&self, helper: Helper) -> bool {
        let programs = helper.programs();
        let runs = programs.iter().all(|program| (self.version)(program).is_some_and(|line| helper.accepts(&line)));
        runs && helper.needed_files().is_none_or(|(finder, files)| programs.iter().all(|program| self.finds(program, finder, &files)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A TeX found next to each engine that finds every file it is asked for.
    fn finds_all(_: &str, finder: &str, files: &[&str]) -> Printed {
        assert_eq!(finder, "kpsewhich");
        Some(files.iter().map(|file| format!("/usr/local/texlive/2025/texmf-dist/tex/latex/x/{file}\n")).collect())
    }

    fn finds_none(_: &str, _: &str, _: &[&str]) -> Printed {
        Some(String::new())
    }

    fn here(helper: Helper, version: fn(&str) -> Printed) -> bool {
        ProgramProbe::new(version, finds_all).is_here(helper)
    }

    #[test]
    fn a_helper_is_here_when_each_of_its_programs_runs() {
        let all = |_: &str| Some("Python 3.14.9".to_owned());
        assert!(here(Helper::Python, all));
        assert!(here(Helper::Bun, all));
        assert!(here(Helper::Latex, all));
        let none = |_: &str| None;
        assert!(!here(Helper::Bun, none), "a program that cannot run, or took too long");
        let only_lua = |program: &str| (program == "lualatex").then(|| "LuaHBTeX, Version 1.22.0".to_owned());
        assert!(!here(Helper::Latex, only_lua), "the cover letter needs xelatex too");
    }

    #[test]
    fn a_python_older_than_the_tools_need_is_missing() {
        assert!(!here(Helper::Python, |_| Some("Python 3.9.6".to_owned())), "the Command Line Tools' Python");
        assert!(here(Helper::Python, |_| Some("Python 3.10.12".to_owned())));
    }

    #[test]
    fn a_tex_that_lacks_a_package_the_templates_need_is_missing() {
        let runs = |_: &str| Some("XeTeX 3.141592653-2.6-0.999997 (TeX Live 2025)".to_owned());
        assert!(!ProgramProbe::new(runs, finds_none).is_here(Helper::Latex), "a BasicTeX without moderncv");
        let without_moderncv =
            |_: &str, finder: &str, files: &[&str]| finds_all("", finder, &files.iter().copied().filter(|file| *file != "moderncv.cls").collect::<Vec<_>>());
        assert!(!ProgramProbe::new(runs, without_moderncv).is_here(Helper::Latex));
        let only_beside_lualatex = |program: &str, finder: &str, files: &[&str]| if program == "lualatex" { finds_all(program, finder, files) } else { None };
        assert!(!ProgramProbe::new(runs, only_beside_lualatex).is_here(Helper::Latex), "xelatex's own TeX must find them too");
        let a_lookalike = |_: &str, _: &str, files: &[&str]| Some(files.iter().map(|file| format!("/x/not-{file}\n")).collect());
        assert!(!ProgramProbe::new(runs, a_lookalike).is_here(Helper::Latex), "the whole file name must match");
        assert!(ProgramProbe::new(runs, finds_all).is_here(Helper::Latex));
        assert!(ProgramProbe::new(|_| Some("bun 1.4.4".to_owned()), finds_none).is_here(Helper::Bun), "Bun needs no file");
    }
}
