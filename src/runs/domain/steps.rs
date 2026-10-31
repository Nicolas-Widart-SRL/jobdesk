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
    tool: Option<&'static str>,
    any_of: &'static [&'static str],
    after: usize,
}

const fn rule(step: usize, tool: Option<&'static str>, any_of: &'static [&'static str]) -> Rule {
    Rule { step, tool, any_of, after: 0 }
}

const APPLY_LABELS: &[&str] = &["Fetch", "Evaluate", "Draft", "Review", "Revise", "Compile", "Finish"];
const APPLY_RULES: &[Rule] = &[
    rule(1, None, &["04-job-evaluation", "salary_lookup"]),
    rule(2, Some("Write"), &["cv/main_", "cover_letters/cover_"]),
    rule(3, None, &["application-reviewer"]),
    Rule { step: 4, tool: Some("Edit"), any_of: &["cv/main_", "cover_letters/cover_"], after: 3 },
    rule(5, None, &["lualatex", "xelatex", "verify_pdf", "verify_layout"]),
    rule(6, None, &["job_search_tracker.csv", "job_posting.md"]),
];

const RANK_LABELS: &[&str] = &["Load", "Score", "Store"];
const RANK_RULES: &[Rule] = &[rule(1, None, &["posting-scorer"]), rule(2, None, &["rank_state.py apply", "rank_state.py sweep"])];

const SCRAPE_LABELS: &[&str] = &["Prepare", "Search", "Store"];
const SCRAPE_RULES: &[Rule] = &[rule(1, None, &["cli.ts"]), rule(2, None, &["scrape_state.py store"])];

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

    pub fn observe(&mut self, tool: &str, summary: &str) {
        for rule in self.rules {
            let tool_matches = rule.tool.is_none_or(|expected| expected == tool);
            let text_matches = rule.any_of.iter().any(|needle| summary.contains(needle));
            if tool_matches && text_matches && self.current >= rule.after && rule.step > self.current {
                self.current = rule.step;
            }
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
}
