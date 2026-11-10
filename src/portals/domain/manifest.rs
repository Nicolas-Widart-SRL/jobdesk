use crate::shared_kernel::file_text::{with_lf, without_bom};
use crate::shared_kernel::visible_text::visible_chars;

const NAME_CHARS: usize = 60;
const SENTENCE_CHARS: usize = 200;

/// What a portal's `SKILL.md` says about it, in the words Jobdesk shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Whether `/scrape` searches it. Only a top-level `enabled: false` in the frontmatter turns it off.
    pub enabled: bool,
    /// The board's plain name, such as LinkedIn for `# LinkedIn Search Skill`.
    pub name: String,
    /// The first sentence of the paragraph under the title, which says what it searches.
    pub sentence: Option<String>,
    pub quotes: Quotes,
    /// It carries a "Personal use only" warning, because the board's terms do not allow automated access.
    pub personal_use: bool,
    /// It reads an API key from a `<SERVICE>_API_TOKEN` variable.
    pub needs_key: bool,
}

/// What a portal does with a multi-word term in quotes, from its `Query syntax:` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quotes {
    /// The words must stand together, in that order.
    Phrase,
    /// Every word must show, in any order.
    AllWords,
    /// Quotes change nothing.
    Ignored,
    /// The line is missing or says something else.
    Unknown,
}

/// Reads a `SKILL.md`. `folder` is the portal's folder name, such as `linkedin-search`, for a name when the title has none.
pub fn parse_manifest(text: &str, folder: &str) -> Manifest {
    let text = with_lf(without_bom(text));
    let (frontmatter, body) = split_frontmatter(&text);
    Manifest {
        enabled: frontmatter.is_none_or(enabled),
        name: title(body).map(board_name).filter(|name| !name.is_empty()).unwrap_or_else(|| folder_name(folder)),
        sentence: first_sentence(body),
        quotes: body.lines().find_map(|line| line.trim().strip_prefix("Query syntax:")).map_or(Quotes::Unknown, quotes),
        personal_use: prose_lines(body).any(|line| line.starts_with('#') && line.to_lowercase().contains("personal use only")),
        needs_key: text.contains("MISSING_CREDENTIALS") || names_a_token(&text),
    }
}

/// The frontmatter between a first line `---` and the next line `---`, and the text after it.
fn split_frontmatter(text: &str) -> (Option<&str>, &str) {
    let fence = |line: &str| line.trim_end() == "---";
    let mut lines = text.split_inclusive('\n');
    if !lines.next().is_some_and(fence) {
        return (None, text);
    }
    let start = text.find('\n').map_or(text.len(), |at| at + 1);
    let mut at = start;
    for line in lines {
        if fence(line) {
            return (Some(&text[start..at]), &text[at + line.len()..]);
        }
        at += line.len();
    }
    (None, text)
}

/// The last top-level `enabled` key, as YAML takes it, turns the portal off when its value reads `false` in any case.
/// Space may stand before the colon, and a `#` after a space or a tab opens a comment.
fn enabled(frontmatter: &str) -> bool {
    let Some(value) = frontmatter.lines().filter_map(enabled_value).last() else { return true };
    !value.trim_matches(['"', '\'']).eq_ignore_ascii_case("false")
}

/// The value of a top-level `enabled` line, its comment dropped.
fn enabled_value(line: &str) -> Option<&str> {
    let (key, value) = line.split_once(':')?;
    if key.trim_end() != "enabled" {
        return None;
    }
    let comment = value.char_indices().find(|&(at, c)| c == '#' && (at == 0 || value[..at].ends_with([' ', '\t'])));
    Some(comment.map_or(value, |(at, _)| &value[..at]).trim())
}

/// The first `# ` heading outside a code block.
fn title(body: &str) -> Option<&str> {
    prose_lines(body).find_map(|line| line.strip_prefix("# "))
}

/// The lines of `body` outside fenced code blocks.
fn prose_lines(body: &str) -> impl Iterator<Item = &str> {
    let mut fenced = false;
    body.lines().filter(move |line| {
        let fence = line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~");
        fenced ^= fence;
        !fenced && !fence
    })
}

/// `LinkedIn Search Skill` reads LinkedIn, and `Jobnet-Search Skill` reads Jobnet.
/// A title that is only `Search Skill` reads as no name.
fn board_name(title: &str) -> String {
    let mut name = format!(" {}", plain(title));
    for ends in [&[" skill"][..], &[" search", "-search"]] {
        if let Some(kept) = ends.iter().find_map(|end| without_end(&name, end)) {
            name = kept.trim_end().to_owned();
        }
    }
    cut(name.trim(), NAME_CHARS)
}

/// `text` without `end` at its end, in any case.
fn without_end<'a>(text: &'a str, end: &str) -> Option<&'a str> {
    let at = text.len().checked_sub(end.len())?;
    text.get(at..).filter(|tail| tail.eq_ignore_ascii_case(end)).map(|_| &text[..at])
}

/// `linkedin-search` reads Linkedin.
fn folder_name(folder: &str) -> String {
    let word = folder.strip_suffix("-search").unwrap_or(folder);
    let word: String = visible_chars(word).collect();
    let mut chars = word.chars();
    let name = chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect());
    cut(&name, NAME_CHARS)
}

/// The first sentence of the first plain paragraph between the title and the next heading.
fn first_sentence(body: &str) -> Option<String> {
    let mut lines = prose_lines(body).skip_while(|line| !line.starts_with("# ")).skip(1);
    let mut paragraph = Vec::new();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.starts_with('#') {
            break;
        }
        if line.is_empty() {
            if paragraph.is_empty() {
                continue;
            }
            break;
        }
        if paragraph.is_empty() && !plain_start(line) {
            // A quote, a list, a table or HTML: skip the whole block, but never past a heading.
            match lines.by_ref().map(str::trim).find(|rest| rest.is_empty() || rest.starts_with('#')) {
                Some(rest) if rest.starts_with('#') => break,
                _ => continue,
            }
        }
        paragraph.push(line);
    }
    if paragraph.is_empty() {
        return None;
    }
    let text = plain(&paragraph.join(" "));
    let end = sentence_end(&text).unwrap_or(text.len());
    Some(cut(text[..end].trim(), SENTENCE_CHARS)).filter(|sentence| !sentence.is_empty())
}

/// Whether a line can open a plain paragraph: not a quote, a list item, a table row, HTML or a rule.
fn plain_start(line: &str) -> bool {
    let numbered = line.split_once(['.', ')']).is_some_and(|(number, _)| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
    let marked = ["- ", "* ", "+ ", "---", "***", "___", "==="].iter().any(|mark| line.starts_with(mark));
    !(line.starts_with(['>', '|', '<']) || marked || numbered)
}

/// Just after the first `.`, `!` or `?` that ends a sentence, so `Jobindex.dk` stays whole.
fn sentence_end(text: &str) -> Option<usize> {
    text.char_indices().find_map(|(at, c)| {
        let end = at + c.len_utf8();
        (matches!(c, '.' | '!' | '?') && text[end..].chars().next().is_none_or(char::is_whitespace)).then_some(end)
    })
}

/// The text without inline Markdown, hidden characters or runs of spaces: `[text](url)` reads text, and `**`, `__`,
/// backticks and `*` go.
fn plain(text: &str) -> String {
    let unlinked = without_links(text);
    let marked = unlinked.replace("**", "").replace("__", "").replace(['`', '*'], "");
    let visible: String = visible_chars(&marked).collect();
    visible.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `[text](url)` and `![text](url)` become text.
fn without_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let link = rest[open + 1..].split_once("](").and_then(|(label, after)| {
            let close = after.find(')')?;
            (!label.contains('[')).then_some((label, open + 1 + label.len() + 2 + close + 1))
        });
        match link {
            Some((label, end)) => {
                out.push_str(rest[..open].strip_suffix('!').unwrap_or(&rest[..open]));
                out.push_str(label);
                rest = &rest[end..];
            }
            None => {
                out.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn quotes(syntax: &str) -> Quotes {
    let syntax = syntax.to_lowercase();
    let says = |words: &[&str]| words.iter().any(|word| syntax.contains(word));
    if says(&["exact phrase"]) {
        Quotes::Phrase
    } else if says(&["every quoted word", "require every"]) {
        Quotes::AllWords
    } else if says(&["not honoured", "ignored"]) {
        Quotes::Ignored
    } else {
        Quotes::Unknown
    }
}

/// Whether `text` names a variable such as `NORTHFIELD_API_TOKEN`: a capital letter, then capitals, digits or `_`.
fn names_a_token(text: &str) -> bool {
    text.match_indices("_API_TOKEN")
        .any(|(at, _)| text[..at].bytes().rev().take_while(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_').any(|b| b.is_ascii_uppercase()))
}

/// At most `limit` characters.
fn cut(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped as upstream's `linkedin-search`: a commented `enabled: true`, a warning heading and quotes not honoured.
    const LINKEDIN: &str = "---\nname: linkedin-search\nversion: 1.0.0\ndescription: >\n  Use this skill whenever the user wants to search for jobs.\n  Trigger phrases: find a job, enabled: false.\ncontext: fork\nenabled: true  # set to false to keep this portal installed but have /scrape skip it\nallowed-tools: Bash(bun run .agents/skills/linkedin-search/cli/src/cli.ts *)\n---\n\n# LinkedIn Search Skill\n\nSearch live job listings from LinkedIn's public job board for **any country/region**\n(and remote). No authentication, no API key.\n\n> This is a country-agnostic worked example.\n\n## ⚠️ Personal use only\n\nKeep volume low.\n\n## Commands\n\n### Search job listings\n\n```bash\nbun run .agents/skills/linkedin-search/cli/src/cli.ts search --location \"<place>\" [flags]\n```\n\nQuery syntax: phrase quoting not honoured; OR not verified.\n";

    /// Shaped as upstream's `freehire-search`: a link in the first sentence, and quotes that require every word.
    const FREEHIRE: &str = "---\nname: freehire-search\nenabled: true  # set to false to keep this portal installed\n---\n\n# freehire Search Skill\n\nSearch live job listings from the **[freehire.me](https://freehire.me)** job\naggregator — an open-source IT job board that normalizes postings into one schema. No authentication, no API key.\n\n## Commands\n\nQuery syntax: quotes require every quoted word (any order, not a contiguous phrase); `-word` excludes; uppercase `OR` matches any alternative.\n";

    /// Shaped as upstream's Danish demos: off by a commented line, and an exact phrase in quotes.
    const JOBNET: &str = "---\nname: jobnet-search\ncontext: fork\nenabled: false  # Danish demo portal - ships opt-in; /setup enables it when your market is Denmark, or set true here yourself\n---\n\n# Jobnet-Search Skill\n\nAccess live Danish job listings from the Jobnet.dk public API. No authentication needed.\n\nQuery syntax: phrase quoting not honoured; no OR.\n";

    /// Upstream `jobindex-search`'s query line.
    const JOBINDEX_SYNTAX: &str = "Query syntax: double quotes inside the argument make an exact phrase, word order included (e.g. `--query '\"product owner\"'`); single quotes are ignored; no OR.";

    #[test]
    fn reads_linkedin() {
        let manifest = parse_manifest(LINKEDIN, "linkedin-search");
        assert!(manifest.enabled);
        assert_eq!(manifest.name, "LinkedIn");
        assert_eq!(manifest.sentence.as_deref(), Some("Search live job listings from LinkedIn's public job board for any country/region (and remote)."));
        assert_eq!(manifest.quotes, Quotes::Ignored);
        assert!(manifest.personal_use);
        assert!(!manifest.needs_key);
    }

    #[test]
    fn reads_freehire_without_its_link() {
        let manifest = parse_manifest(FREEHIRE, "freehire-search");
        assert_eq!(manifest.name, "freehire");
        assert_eq!(
            manifest.sentence.as_deref(),
            Some("Search live job listings from the freehire.me job aggregator — an open-source IT job board that normalizes postings into one schema.")
        );
        assert_eq!(manifest.quotes, Quotes::AllWords);
        assert!(!manifest.personal_use);
    }

    #[test]
    fn reads_a_danish_demo_as_off() {
        let manifest = parse_manifest(JOBNET, "jobnet-search");
        assert!(!manifest.enabled, "a comment after `false` keeps it off");
        assert_eq!(manifest.name, "Jobnet");
        assert_eq!(manifest.sentence.as_deref(), Some("Access live Danish job listings from the Jobnet.dk public API."));
        assert_eq!(manifest.quotes, Quotes::Ignored);
        let jobindex =
            JOBNET.replace("# Jobnet-Search Skill", "# Jobindex Search Skill").replace("Query syntax: phrase quoting not honoured; no OR.", JOBINDEX_SYNTAX);
        let jobindex = parse_manifest(&jobindex, "jobindex-search");
        assert_eq!((jobindex.name.as_str(), jobindex.quotes), ("Jobindex", Quotes::Phrase));
    }

    #[test]
    fn only_a_top_level_enabled_line_in_the_frontmatter_counts() {
        assert!(parse_manifest("---\nname: on\n---\nenabled: false in prose\n", "on-search").enabled);
        assert!(parse_manifest("---\nname: on\n---\n\nenabled: false\n", "on-search").enabled);
        assert!(parse_manifest("---\nnested:\n  enabled: false\n---\n", "on-search").enabled);
        assert!(parse_manifest("# On Search Skill\n\nenabled: false\n", "on-search").enabled, "no frontmatter means on");
        assert!(parse_manifest("---\nname: on\n", "on-search").enabled, "an unclosed frontmatter is none");
        for off in ["enabled: false", "enabled: FALSE", "enabled: \"false\"", "enabled: 'False'  # off for now", "enabled:false"] {
            assert!(!parse_manifest(&format!("---\nname: off\n{off}\n---\n"), "off-search").enabled, "{off}");
        }
        assert!(parse_manifest("---\nenabled: true # not false\n---\n", "on-search").enabled);
    }

    /// `/scrape` reads the frontmatter as YAML: a tab may open a comment, space may stand before the colon, and the
    /// last of two keys wins.
    #[test]
    fn enabled_reads_as_yaml_does() {
        let reads = |front: &str| parse_manifest(&format!("---\nname: n\n{front}\n---\n"), "n-search").enabled;
        assert!(!reads("enabled: false\t# off for now"));
        assert!(!reads("enabled : false"));
        assert!(!reads("enabled:\tfalse"));
        assert!(!reads("enabled: true\nenabled: false"));
        assert!(reads("enabled: false\nenabled: true"));
        assert!(reads("enabled: #false"), "a comment alone is no value");
        assert!(reads("enabledx: false"));
    }

    #[test]
    fn crlf_and_a_byte_order_mark_read_the_same() {
        let windows = format!("\u{feff}{}", JOBNET.replace('\n', "\r\n"));
        assert_eq!(parse_manifest(&windows, "jobnet-search"), parse_manifest(JOBNET, "jobnet-search"));
    }

    #[test]
    fn a_missing_title_falls_back_to_the_folder() {
        let manifest = parse_manifest("---\nname: northfield-search\n---\n\nNo title here.\n", "northfield-search");
        assert_eq!(manifest.name, "Northfield");
        assert_eq!(manifest.sentence, None);
        assert_eq!(manifest.quotes, Quotes::Unknown);
        assert_eq!(parse_manifest("# Search Skill\n", "northfield").name, "Northfield", "a title that is only the suffix");
    }

    #[test]
    fn names_and_sentences_drop_hidden_characters_and_are_cut() {
        let text = format!("# North\u{202E}field Search Skill\n\n{}. Then more.\n", "word ".repeat(80));
        let manifest = parse_manifest(&text, "northfield-search");
        assert_eq!(manifest.name, "Northfield");
        assert_eq!(manifest.sentence.unwrap().chars().count(), SENTENCE_CHARS);
        assert_eq!(parse_manifest(&format!("# {} Skill\n", "N".repeat(90)), "n").name.chars().count(), NAME_CHARS);
    }

    #[test]
    fn the_sentence_skips_quotes_and_lists_but_stops_at_a_heading() {
        let text = "# Northfield Search Skill\n\n> A note.\n> More note.\n\n- a list\n\nSearch Northfield's *public* board. Twice.\n";
        assert_eq!(parse_manifest(text, "n").sentence.as_deref(), Some("Search Northfield's public board."));
        assert_eq!(parse_manifest("# Northfield Search Skill\n\n## Commands\n\nRun it.\n", "n").sentence, None);
        assert_eq!(parse_manifest("# Northfield Search Skill\n\n> note\n## Commands\n\nRun it.\n", "n").sentence, None, "a skipped block stops at a heading");
        assert_eq!(parse_manifest("# Northfield Search Skill\n\n- item\n### Usage\n\nRun it.\n", "n").sentence, None);
        assert_eq!(parse_manifest("# Northfield Search Skill\n\nSearch `jobs` and ask!\n", "n").sentence.as_deref(), Some("Search jobs and ask!"));
    }

    #[test]
    fn a_title_inside_a_code_block_is_no_title() {
        let text = "```bash\n# Northfield roles\n```\n\n# Real Search Skill\n\nSearch it.\n";
        let manifest = parse_manifest(text, "n");
        assert_eq!(manifest.name, "Real");
        assert_eq!(manifest.sentence.as_deref(), Some("Search it."));
    }

    #[test]
    fn a_key_is_named_by_its_variable_or_its_error_code() {
        assert!(parse_manifest("Export `NORTHFIELD_API_TOKEN` before you run it.\n", "n").needs_key);
        assert!(parse_manifest("Without it the CLI exits with `MISSING_CREDENTIALS`.\n", "n").needs_key);
        assert!(parse_manifest("Export N0RTH_2_API_TOKEN.\n", "n").needs_key);
        assert!(!parse_manifest("Reads `<SERVICE>_API_TOKEN` in general.\n", "n").needs_key);
        assert!(!parse_manifest("Export 2_API_TOKEN or northfield_API_TOKEN.\n", "n").needs_key);
    }

    #[test]
    fn query_syntax_words_map_to_quotes() {
        assert_eq!(quotes(" double quotes make an exact phrase; single quotes are ignored"), Quotes::Phrase);
        assert_eq!(quotes(" Quotes require every word."), Quotes::AllWords);
        assert_eq!(quotes(" quotes are ignored"), Quotes::Ignored);
        assert_eq!(quotes(" not verified"), Quotes::Unknown);
    }
}
