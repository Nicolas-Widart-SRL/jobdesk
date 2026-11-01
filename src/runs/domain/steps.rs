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
pub(crate) enum Match {
    /// tool is one of `tools`, and the summary (a file path) contains a needle
    File { tools: &'static [&'static str], any_of: &'static [&'static str] },
    /// tool is "Bash", and some command segment runs a program whose token contains a name.
    /// `args` empty means any; otherwise the token right after the program must be one of them.
    Program { names: &'static [&'static str], args: &'static [&'static str] },
    /// tool is "Agent" or "Task", and the part before ": " contains a needle
    Agent(&'static [&'static str]),
}

/// Words that start a script rather than name it.
const INTERPRETERS: &[&str] = &["python", "python3", "uv", "bun", "node", "bash", "sh", "npx", "deno"];

impl Match {
    pub(crate) fn hits(&self, tool: &str, summary: &str) -> bool {
        match self {
            Match::File { tools, any_of } => tools.contains(&tool) && any_of.iter().any(|needle| summary.contains(needle)),
            Match::Program { names, args } => tool == "Bash" && summary.split(['&', '|', ';', '\n']).any(|segment| runs_program(segment, names, args)),
            Match::Agent(needles) => {
                let agent = summary.split(": ").next().unwrap_or(summary);
                matches!(tool, "Agent" | "Task") && needles.iter().any(|needle| agent.contains(needle))
            }
        }
    }
}

/// The program a command segment runs, past assignments, interpreters, `run` and flags, and the token after it.
fn runs_program(segment: &str, names: &[&str], args: &[&str]) -> bool {
    let mut tokens = segment.split_whitespace().skip_while(|token| is_assignment(token) || is_interpreter(token) || *token == "run" || token.starts_with('-'));
    let Some(program) = tokens.next() else { return false };
    names.iter().any(|name| program.contains(name)) && (args.is_empty() || tokens.next().is_some_and(|arg| args.contains(&arg)))
}

fn is_assignment(token: &str) -> bool {
    token.split_once('=').is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

fn is_interpreter(token: &str) -> bool {
    INTERPRETERS.contains(&token.rsplit('/').next().unwrap_or(token))
}

const APPLY_LABELS: &[&str] = &["Fetch", "Evaluate", "Draft", "Review", "Revise", "Compile", "Finish"];
const APPLY_DOCUMENTS: &[&str] = &["cv/main_", "cover_letters/cover_"];
const APPLY_RULES: &[Rule] = &[
    rule(1, Match::File { tools: &["Read"], any_of: &["04-job-evaluation"] }),
    rule(1, Match::Program { names: &["salary_lookup"], args: &[] }),
    rule(2, Match::File { tools: &["Write"], any_of: APPLY_DOCUMENTS }),
    rule(3, Match::Agent(&["application-reviewer"])),
    Rule { step: 4, on: Match::File { tools: &["Edit"], any_of: APPLY_DOCUMENTS }, after: 3 },
    rule(5, Match::Program { names: &["lualatex", "xelatex", "verify_pdf", "verify_layout"], args: &[] }),
    rule(6, Match::File { tools: &["Write", "Edit"], any_of: &["job_search_tracker.csv", "job_posting.md"] }),
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
    fn rank_store_needs_its_subcommand() {
        let mut rail = StepRail::for_line("/rank").unwrap();
        rail.observe("Bash", "python3 rank_state.py show");
        assert_eq!(rail.current, 0);
        rail.observe("Bash", "python3 rank_state.py apply --x");
        assert_eq!(rail.current, 2);
    }

    #[test]
    fn one_call_takes_the_highest_step() {
        let mut rail = StepRail::for_line("/apply x").unwrap();
        rail.observe("Bash", "python3 salary_lookup.py x && lualatex main.tex");
        assert_eq!(rail.current, 5);
    }
}
