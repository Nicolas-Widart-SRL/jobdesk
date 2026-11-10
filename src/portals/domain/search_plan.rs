use crate::portals::domain::Quotes;
use crate::shared_kernel::file_text::{with_lf, without_bom};
use crate::shared_kernel::placeholders::placeholder_count;
use crate::shared_kernel::visible_text::visible_chars;

/// What `/scrape` searches for, from `.claude/skills/job-scraper/search-queries.md`. Every portal shares it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchPlan {
    pub priorities: Vec<Priority>,
}

/// One `### Priority N: Name` category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Priority {
    pub number: u32,
    pub name: String,
    /// The plain terms the portal CLIs get, one search each.
    pub terms: Vec<Term>,
    /// The Google lines, such as `site:jobs.example.com "Platform Lead"`, for when a portal fails.
    pub google: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub text: String,
    /// It still holds a setup token, such as `[YOUR_KEY_SKILL]`.
    pub placeholder: bool,
}

/// How a portal reads a term, in the words the screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phrase {
    OneWord,
    /// These words together.
    Together,
    /// These words in any order.
    AnyOrder,
}

#[derive(Clone, Copy)]
enum Block {
    /// No `####` heading yet: an older file keeps only `site:` lines here.
    Before,
    Plain,
    Google,
    Other,
}

/// Reads the search file. Only the Query Categories section counts: `/scrape` scopes every portal search to the last
/// 14 days by itself and never reads the Date Filter section.
pub fn parse_search_plan(text: &str) -> SearchPlan {
    let text = with_lf(without_bom(text));
    let mut plan = SearchPlan::default();
    let (mut section, mut in_priority, mut block, mut fenced) = (String::new(), false, Block::Before, false);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            if let Some(priority) = plan.priorities.last_mut().filter(|_| in_priority) {
                add(priority, block, trimmed);
            }
        } else if let Some(heading) = line.strip_prefix("## ") {
            section = heading.trim().to_lowercase();
            in_priority = false;
        } else if section.starts_with("query categories") {
            if let Some(heading) = line.strip_prefix("### ") {
                let priority = priority(heading);
                in_priority = priority.is_some();
                block = Block::Before;
                plan.priorities.extend(priority);
            } else if let Some(heading) = line.strip_prefix("#### ") {
                let heading = heading.trim().to_lowercase();
                block = if heading.starts_with("plain terms") {
                    Block::Plain
                } else if heading.starts_with("websearch") {
                    Block::Google
                } else {
                    Block::Other
                };
            }
        }
    }
    plan
}

/// `Priority 2: Logistics software` reads as 2 and its name.
fn priority(heading: &str) -> Option<Priority> {
    let rest = heading.trim().get(..9).filter(|word| word.eq_ignore_ascii_case("priority "))?;
    let (number, name) = heading.trim()[rest.len()..].split_once(':')?;
    let name: String = visible_chars(name.trim()).collect();
    Some(Priority { number: number.trim().parse().ok()?, name, terms: Vec::new(), google: Vec::new() })
}

fn add(priority: &mut Priority, block: Block, line: &str) {
    let line: String = visible_chars(line).collect();
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    match block {
        Block::Plain => priority.terms.push(Term { text: line.to_owned(), placeholder: placeholder_count(line) > 0 }),
        Block::Google => priority.google.push(line.to_owned()),
        Block::Before if line.starts_with("site:") => priority.google.push(line.to_owned()),
        Block::Before | Block::Other => {}
    }
}

/// The Google lines that name the portal's own site: a label of the `site:` host is the board's word, the folder
/// without `-search`, or starts with it and a hyphen. `linkedin-search` owns `site:linkedin.com/jobs`.
pub fn site_lines_for<'a>(plan: &'a SearchPlan, folder: &str) -> Vec<&'a str> {
    let word = folder.strip_suffix("-search").unwrap_or(folder).to_lowercase();
    let mut lines: Vec<&str> = Vec::new();
    for line in plan.priorities.iter().flat_map(|priority| &priority.google) {
        if !word.is_empty() && names_site(line, &word) && !lines.contains(&line.as_str()) {
            lines.push(line);
        }
    }
    lines
}

fn names_site(line: &str, word: &str) -> bool {
    let Some(target) = line.strip_prefix("site:") else { return false };
    let host = target.split(char::is_whitespace).next().unwrap_or_default();
    let host = host.split('/').next().unwrap_or_default().to_lowercase();
    host.split('.').any(|label| label == word || label.strip_prefix(word).is_some_and(|rest| rest.starts_with('-')))
}

/// The term as the portal gets it, and how the portal reads it. Several words go in quotes only when the portal
/// honours them.
pub fn term_as_sent(term: &str, quotes: Quotes) -> (String, Phrase) {
    let bare = term.trim();
    let bare = bare.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or(bare).trim();
    if bare.split_whitespace().nth(1).is_none() {
        return (bare.to_owned(), Phrase::OneWord);
    }
    match quotes {
        Quotes::Phrase => (format!("\"{bare}\""), Phrase::Together),
        Quotes::AllWords => (format!("\"{bare}\""), Phrase::AnyOrder),
        Quotes::Ignored | Quotes::Unknown => (bare.to_owned(), Phrase::AnyOrder),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped as upstream's template: Plain terms and WebSearch-only blocks under each priority.
    const TEMPLATE: &str = "# Search Queries for Job Scraper\n\n## Installed portal CLIs\n\n```\nnot a term\n```\n\n## Query Categories\n\nQueries are grouped by priority.\n\n### Priority 1: Platform leadership\n\nThese match your strongest direction.\n\n#### Plain terms (portal CLIs)\n\n```\nPlatform Lead\n[YOUR_KEY_SKILL] developer\nRust\n```\n\n#### WebSearch-only (`site:` fallback)\n\n```\nsite:northfield-jobs.example \"Platform Lead\" Namur\nsite:linkedin.com/jobs \"Platform Lead\" Belgium\nsite:jobs.example.com \"Platform Lead\" Namur\n```\n\n### Priority 2: Logistics software\n\n#### Plain terms (portal CLIs)\n\n```\nroute planning\n```\n\n#### WebSearch-only (`site:` fallback)\n\n```\nsite:www.linkedin.com/jobs route planning\nsite:northfield.example route planning\n```\n\n## Location Filter\n\n```\nNamur\n```\n\n## Date Filter\n\nOnly include jobs posted within the last 14 days, or 30 days for a deadline.\n";

    /// Shaped as the older file: each priority holds only `site:` lines.
    const OLD: &str = "## Query Categories\n\n### Priority 1: Platform and backend leadership\n\n```\nsite:jobs.example.com \"Lead Backend Engineer\" Namur\nsite:jobs.example.com \"Platform Lead\" Brussels\n```\n\n### Priority 2: Logistics software\n\n```\nsite:jobs.example.com route planning Rust Belgium\n```\n\n## Date Filter\n\nOnly include jobs posted within the last 7 days.\n";

    fn texts(priority: &Priority) -> Vec<&str> {
        priority.terms.iter().map(|term| term.text.as_str()).collect()
    }

    #[test]
    fn reads_plain_terms_by_priority() {
        let plan = parse_search_plan(TEMPLATE);
        let heads: Vec<(u32, &str)> = plan.priorities.iter().map(|p| (p.number, p.name.as_str())).collect();
        assert_eq!(heads, [(1, "Platform leadership"), (2, "Logistics software")]);
        assert_eq!(texts(&plan.priorities[0]), ["Platform Lead", "[YOUR_KEY_SKILL] developer", "Rust"]);
        assert_eq!(texts(&plan.priorities[1]), ["route planning"]);
        assert_eq!(plan.priorities[0].google.len(), 3);
    }

    #[test]
    fn marks_terms_that_still_hold_a_placeholder() {
        let plan = parse_search_plan(TEMPLATE);
        let marked: Vec<bool> = plan.priorities[0].terms.iter().map(|term| term.placeholder).collect();
        assert_eq!(marked, [false, true, false]);
    }

    #[test]
    fn an_older_file_has_google_lines_only() {
        let plan = parse_search_plan(OLD);
        assert_eq!(plan.priorities.len(), 2);
        assert!(plan.priorities.iter().all(|priority| priority.terms.is_empty()));
        assert_eq!(plan.priorities[0].google, ["site:jobs.example.com \"Lead Backend Engineer\" Namur", "site:jobs.example.com \"Platform Lead\" Brussels"]);
    }

    #[test]
    fn crlf_a_byte_order_mark_and_no_file_read_as_expected() {
        let windows = format!("\u{feff}{}", TEMPLATE.replace('\n', "\r\n"));
        assert_eq!(parse_search_plan(&windows), parse_search_plan(TEMPLATE));
        assert_eq!(parse_search_plan(""), SearchPlan::default());
    }

    #[test]
    fn a_heading_that_is_no_priority_takes_no_lines() {
        let plan = parse_search_plan("## Query Categories\n\n### Priority 1: One\n\n```\nsite:a.example x\n```\n\n### Notes\n\n```\nsite:b.example y\n```\n");
        assert_eq!(plan.priorities.len(), 1);
        assert_eq!(plan.priorities[0].google, ["site:a.example x"]);
    }

    #[test]
    fn site_lines_name_the_portals_own_site() {
        let plan = parse_search_plan(TEMPLATE);
        assert_eq!(site_lines_for(&plan, "linkedin-search"), ["site:linkedin.com/jobs \"Platform Lead\" Belgium", "site:www.linkedin.com/jobs route planning"]);
        assert_eq!(
            site_lines_for(&plan, "northfield-search"),
            ["site:northfield-jobs.example \"Platform Lead\" Namur", "site:northfield.example route planning"]
        );
        assert!(site_lines_for(&plan, "jobs-search").contains(&"site:jobs.example.com \"Platform Lead\" Namur"));
        assert!(site_lines_for(&plan, "jobindex-search").is_empty(), "jobs.example.com names no other board");
        assert!(site_lines_for(&plan, "north-search").is_empty(), "a label must be the word or start with it and a hyphen");
    }

    #[test]
    fn a_site_line_in_two_priorities_shows_once() {
        let plan = parse_search_plan(
            "## Query Categories\n\n### Priority 1: One\n\n```\nsite:northfield.example lead\n```\n\n### Priority 2: Two\n\n```\nsite:northfield.example lead\nsite:northfield.example rust\n```\n",
        );
        assert_eq!(site_lines_for(&plan, "northfield-search"), ["site:northfield.example lead", "site:northfield.example rust"]);
    }

    #[test]
    fn terms_go_in_quotes_only_where_the_portal_honours_them() {
        assert_eq!(term_as_sent("Rust", Quotes::Phrase), ("Rust".into(), Phrase::OneWord));
        assert_eq!(term_as_sent("route planning", Quotes::Phrase), ("\"route planning\"".into(), Phrase::Together));
        assert_eq!(term_as_sent("route planning", Quotes::AllWords), ("\"route planning\"".into(), Phrase::AnyOrder));
        assert_eq!(term_as_sent("route planning", Quotes::Ignored), ("route planning".into(), Phrase::AnyOrder));
        assert_eq!(term_as_sent("route planning", Quotes::Unknown), ("route planning".into(), Phrase::AnyOrder));
        assert_eq!(term_as_sent("\"route planning\"", Quotes::Phrase), ("\"route planning\"".into(), Phrase::Together), "never quoted twice");
    }
}
