/// A readable progress rail for the long commands.
///
/// Display only: it is inferred from the tools a run uses and never steers the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRail {
    pub labels: &'static [&'static str],
    pub current: usize,
    rules: &'static [Rule],
}

#[derive(Debug, PartialEq, Eq)]
struct Rule {
    step: usize,
    on: Match,
    after: usize,
}

const fn rule(step: usize, on: Match) -> Rule {
    Rule { step, on, after: 0 }
}

/// What a tool call must do to move the rail: it is matched on the action, not on any text in the summary.
#[derive(Debug, PartialEq, Eq)]
pub enum Match {
    /// tool is one of `tools`, and the summary (a file path) contains a needle
    File { tools: &'static [&'static str], any_of: &'static [&'static str] },
    /// tool is "Bash", and some command segment runs a program whose file name contains a name, directly or behind a runner.
    /// `args` empty means any; otherwise the first non-flag token after the program must be one of them.
    Program { names: &'static [&'static str], args: &'static [&'static str] },
    /// tool is "Agent" or "Task", and the part before ": " contains a needle
    Agent(&'static [&'static str]),
}

/// Shell keywords and wrappers that come before the command word.
const WRAPPERS: &[&str] = &["do", "then", "else", "time", "env", "timeout", "xargs", "exec"];

impl Match {
    pub(crate) fn hits(&self, tool: &str, summary: &str) -> bool {
        match self {
            Match::File { tools, any_of } => {
                let path = summary.replace('\\', "/");
                tools.contains(&tool) && any_of.iter().any(|needle| path.contains(needle))
            }
            Match::Program { names, args } => tool == "Bash" && summary.split(['&', '|', ';', '\n']).any(|segment| runs_program(segment, names, args)),
            Match::Agent(needles) => {
                let agent = summary.split_once(": ").map_or(summary, |(agent, _)| agent);
                matches!(tool, "Agent" | "Task") && needles.iter().any(|needle| agent.contains(needle))
            }
        }
    }
}

/// Whether a command segment runs one of `names`, as its command word or behind a runner such as `uv run`
/// or `python3`, and whether the first argument after the program is one of `args`.
/// Known limit: a flag with a value between the program and its subcommand is read as the subcommand.
fn runs_program(segment: &str, names: &[&str], args: &[&str]) -> bool {
    let tokens: Vec<&str> = segment.split_whitespace().map(|token| token.trim_matches(['"', '\''])).collect();
    let mut at = 0;
    let mut wrapped = false;
    while let Some(&token) = tokens.get(at) {
        let wrapper = WRAPPERS.contains(&token);
        // After a wrapper come its own flags and a duration, as in `timeout 300 lualatex`.
        let wrapper_arg = wrapped && (token.starts_with('-') || token.starts_with(|c: char| c.is_ascii_digit()));
        if !(is_assignment(token) || wrapper || wrapper_arg) {
            break;
        }
        wrapped |= wrapper;
        at += 1;
    }
    let Some(&word) = tokens.get(at) else { return false };
    let named = |token: &&str| {
        let file = basename(token);
        names.iter().any(|name| file.contains(name))
    };
    let program = if named(&word) {
        at
    } else if is_interpreter(word) {
        match script_after(&tokens, at + 1) {
            Some(script) if named(&tokens[script]) => script,
            _ => return false,
        }
    } else {
        return false;
    };
    args.is_empty() || tokens[program + 1..].iter().find(|token| !token.starts_with('-') && **token != "\\").is_some_and(|token| args.contains(token))
}

/// The script a runner starts: the first token past its flags, `run` and a nested interpreter.
/// `-c` and `-m` run no script file, so they give none.
fn script_after(tokens: &[&str], from: usize) -> Option<usize> {
    let mut at = from;
    while let Some(&token) = tokens.get(at) {
        match token {
            "-c" | "-m" => return None,
            "-W" | "-X" | "--with" | "--python" | "--from" => at += 1,
            "run" => {}
            _ if token.starts_with('-') || is_interpreter(token) => {}
            _ => return Some(at),
        }
        at += 1;
    }
    None
}

fn is_assignment(token: &str) -> bool {
    token.split_once('=').is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// The file name of a path in either style, without a Windows `.exe`.
fn basename(token: &str) -> &str {
    let file = token.rsplit(['/', '\\']).next().unwrap_or(token);
    match file.len().checked_sub(4).and_then(|cut| file.split_at_checked(cut)) {
        Some((stem, ext)) if ext.eq_ignore_ascii_case(".exe") => stem,
        _ => file,
    }
}

/// A runner starts a script rather than naming it.
fn is_interpreter(token: &str) -> bool {
    let file = basename(token).to_ascii_lowercase();
    matches!(file.as_str(),"python" | "python3" | "py" | "node" | "bun" | "deno" | "bash" | "sh" | "uv" | "uvx" | "npx" | "bunx" | "pnpm") || file.starts_with("python3.")
}

const APPLY_LABELS: &[&str] = &["Fetch", "Evaluate", "Draft", "Review", "Revise", "Compile", "Finish"];
const APPLY_DOCUMENTS: &[&str] = &["cv/main_", "cover_letters/cover_"];
const APPLY_RULES: &[Rule] = &[
    rule(1, Match::File { tools: &["Read"], any_of: &["04-job-evaluation"] }),
    rule(1, Match::Program { names: &["salary_lookup"], args: &[] }),
    rule(2, Match::File { tools: &["Write"], any_of: APPLY_DOCUMENTS }),
    rule(3, Match::Agent(&["application-reviewer"])),
    Rule { step: 4, on: Match::File { tools: &["Edit", "MultiEdit"], any_of: APPLY_DOCUMENTS }, after: 3 },
    rule(5, Match::Program { names: &["lualatex", "xelatex", "verify_pdf", "verify_layout"], args: &[] }),
    rule(6, Match::File { tools: &["Write", "Edit", "MultiEdit"], any_of: &["job_search_tracker.csv", "job_posting.md"] }),
];

const RANK_LABELS: &[&str] = &["Load", "Score", "Store"];
const RANK_RULES: &[Rule] = &[rule(1, Match::Agent(&["posting-scorer"])), rule(2, Match::Program { names: &["rank_state.py"], args: &["apply", "sweep"] })];

const SCRAPE_LABELS: &[&str] = &["Prepare", "Search", "Store"];
const SCRAPE_RULES: &[Rule] = &[rule(1, Match::Program { names: &["cli.ts"], args: &[] }), rule(2, Match::Program { names: &["scrape_state.py"], args: &["store"] })];

impl StepRail {
    pub fn for_line(line: &str) -> Option<Self> {
        let (labels, rules) = match line.split_whitespace().next()? {
            "/apply" => (APPLY_LABELS, APPLY_RULES),
            "/rank" => (RANK_LABELS, RANK_RULES),
            "/scrape" => (SCRAPE_LABELS, SCRAPE_RULES),
            _ => return None,
        };
        Some(Self { labels, current: 0, rules })
    }

    /// Whether the run has reached the last step.
    pub fn is_complete(&self) -> bool {
        self.current + 1 == self.labels.len()
    }

    /// Every rule is judged against the step the call began at, so one call never chains rules.
    pub fn observe(&mut self, tool: &str, summary: &str) {
        let from = self.current;
        let reached = self.rules.iter().filter(|rule| rule.step > from && from >= rule.after && rule.on.hits(tool, summary)).map(|rule| rule.step).max();
        if let Some(step) = reached {
            self.current = step;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_moves_forward_only() {
        let mut rail = StepRail::for_line("/apply https://x.example").unwrap();
        rail.observe("Read", ".claude/skills/a/04-job-evaluation.md");
        assert_eq!(rail.current, 1);
        rail.observe("Write", "cv/main_acme_lead.tex");
        rail.observe("Agent", "application-reviewer: critique");
        rail.observe("Edit", "cv/main_acme_lead.tex");
        assert_eq!(rail.current, 4);
        rail.observe("Read", "04-job-evaluation.md");
        assert_eq!(rail.current, 4);
    }

    #[test]
    fn edit_counts_as_revise_only_after_review() {
        let mut rail = StepRail::for_line("/apply x").unwrap();
        rail.observe("Edit", "cv/main_acme.tex");
        assert_eq!(rail.current, 0);
    }

    #[test]
    fn short_commands_have_no_rail() {
        assert_eq!(StepRail::for_line("/outcome acme"), None);
    }

    #[test]
    fn listing_files_does_not_move_the_rail() {
        let mut rail = StepRail::for_line("/apply x").unwrap();
        rail.observe("Read", "04-job-evaluation.md");
        assert_eq!(rail.current, 1);
        rail.observe("Bash", "ls salary_data.json salary_lookup.py job_search_tracker.csv");
        assert_eq!(rail.current, 1);
    }

    #[test]
    fn reading_the_tracker_or_posting_is_not_finish() {
        let mut rail = StepRail::for_line("/apply x").unwrap();
        rail.current = 5;
        rail.observe("Read", "/w/job_search_tracker.csv");
        rail.observe("Read", "/w/documents/applications/x/job_posting.md");
        assert_eq!(rail.current, 5);
    }

    #[test]
    fn scripts_move_the_rail() {
        let at = |tool: &str, summary: &str| {
            let mut rail = StepRail::for_line("/apply x").unwrap();
            rail.observe(tool, summary);
            rail.current
        };
        assert_eq!(at("Bash", "python3 salary_lookup.py \"Acme\" --json"), 1);
        assert_eq!(at("Bash", "cd cv && lualatex -interaction=nonstopmode main.tex"), 5);
        assert_eq!(at("Bash", "uv run python tools/verify_layout.py cv/x.pdf"), 5);
        assert_eq!(at("Edit", "/w/job_search_tracker.csv"), 6);
    }

    #[test]
    fn scripts_hide_behind_flags_and_wrappers() {
        let at = |summary: &str| {
            let mut rail = StepRail::for_line("/apply x").unwrap();
            rail.observe("Bash", summary);
            rail.current
        };
        assert_eq!(at("uv run --with pypdf python tools/verify_pdf.py x.pdf"), 5);
        assert_eq!(at("python3 -X utf8 tools/verify_layout.py"), 5);
        assert_eq!(at("timeout 300 lualatex main.tex"), 5);
        assert_eq!(at("for f in a.tex; do lualatex \"$f\"; done"), 5);
        assert_eq!(at("C:\\Python312\\python.exe tools\\verify_pdf.py cv\\x.pdf"), 5);
        assert_eq!(at("cat .claude/x/salary_lookup.py"), 0);
    }

    #[test]
    fn complete_only_at_the_last_step() {
        let mut rail = StepRail::for_line("/rank").unwrap();
        assert!(!rail.is_complete());
        rail.observe("Bash", "python3 rank_state.py apply");
        assert!(rail.is_complete());
    }

    #[test]
    fn windows_paths_match_file_needles() {
        let mut rail = StepRail::for_line("/apply x").unwrap();
        rail.observe("Write", "C:\\w\\cv\\main_acme.tex");
        assert_eq!(rail.current, 2);
        rail.observe("Agent", "application-reviewer: critique");
        rail.observe("Edit", "C:\\w\\cv\\main_acme.tex");
        assert_eq!(rail.current, 4);
    }

    #[test]
    fn only_the_script_that_runs_is_tested() {
        let at = |summary: &str| {
            let mut rail = StepRail::for_line("/apply x").unwrap();
            rail.observe("Bash", summary);
            rail.current
        };
        assert_eq!(at("bash -c 'cat tools/salary_lookup.py'"), 0);
        assert_eq!(at("python3 -m py_compile tools/verify_layout.py"), 0);
        assert_eq!(at("python3 tools/posting_guard.py --in documents/verify_pdf_notes.txt"), 0);
        assert_eq!(at("uv run --python 3.12 python tools/verify_layout.py cv/x.pdf"), 5);
        assert_eq!(at("PYTHONUTF8=1 \"C:\\Python312\\PYTHON.EXE\" \"tools\\verify_pdf.py\""), 5);
    }

    #[test]
    fn multi_edit_finishes_like_edit() {
        let mut rail = StepRail::for_line("/apply x").unwrap();
        rail.current = 5;
        rail.observe("MultiEdit", "/w/job_search_tracker.csv");
        assert_eq!(rail.current, 6);
    }

    #[test]
    fn rank_store_needs_its_subcommand() {
        let mut rail = StepRail::for_line("/rank").unwrap();
        rail.observe("Bash", "python3 rank_state.py show");
        assert_eq!(rail.current, 0);
        rail.observe("Bash", "python3 rank_state.py apply --x");
        assert_eq!(rail.current, 2);
        let mut rail = StepRail::for_line("/rank").unwrap();
        rail.observe("Bash", "python3 tools/rank_state.py --x apply");
        assert_eq!(rail.current, 2);
    }

    #[test]
    fn one_call_takes_the_highest_step() {
        let mut rail = StepRail::for_line("/apply x").unwrap();
        rail.observe("Bash", "python3 salary_lookup.py x && lualatex main.tex");
        assert_eq!(rail.current, 5);
    }
}
