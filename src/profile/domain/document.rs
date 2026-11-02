use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::profile::domain::topic::{ProfileFile, SectionRef};
use crate::shared_kernel::markdown;

/// A `##` or `###` of a file and everything under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub level: u8,
    pub heading: String,
    pub parent: Option<String>,
    pub range: Range<usize>,
    pub nth: usize,
}

/// Names one block of one file, so a save can find it again after the file changed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockKey {
    pub file: ProfileFile,
    pub level: u8,
    pub heading: String,
    pub parent: Option<String>,
    pub nth: usize,
}

struct Heading {
    level: u8,
    text: String,
    start: usize,
}

/// The headings at the top level of the document. A heading inside a code fence, a quote, a list or the front matter is not one.
fn headings(text: &str) -> Vec<Heading> {
    let mut found = Vec::new();
    let mut nested = 0usize;
    let mut open: Option<Heading> = None;
    for (event, range) in Parser::new_ext(text, markdown::options() | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS).into_offset_iter() {
        match event {
            Event::Start(Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::FootnoteDefinition(_)) => nested += 1,
            Event::End(TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item | TagEnd::FootnoteDefinition) => nested = nested.saturating_sub(1),
            Event::Start(Tag::Heading { level, .. }) if nested == 0 => {
                let start = text[..range.start].rfind(['\n', '\r']).map_or(0, |at| at + 1);
                open = Some(Heading { level: level as usize as u8, text: String::new(), start });
            }
            Event::End(TagEnd::Heading(_)) => found.extend(open.take()),
            Event::Text(part) | Event::Code(part) => {
                if let Some(heading) = open.as_mut() {
                    heading.text.push_str(&part);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(heading) = open.as_mut() {
                    heading.text.push(' ');
                }
            }
            _ => {}
        }
    }
    found
}

/// Every `##` and `###` block of `text`, in file order.
/// A `##` block runs to the next `#` or `##` and holds its `###` blocks. A `###` block runs to the next `#`, `##` or `###`.
pub fn blocks(text: &str) -> Vec<Block> {
    let found = headings(text);
    let mut result: Vec<Block> = Vec::new();
    let mut section: Option<String> = None;
    for (at, heading) in found.iter().enumerate() {
        let parent = match heading.level {
            1 => {
                section = None;
                continue;
            }
            2 => {
                section = Some(heading.text.clone());
                None
            }
            3 => section.clone(),
            _ => continue,
        };
        let end = found[at + 1..].iter().find(|next| next.level <= heading.level).map_or(text.len(), |next| next.start);
        let nth = result.iter().filter(|earlier| earlier.level == heading.level && earlier.heading == heading.text && earlier.parent == parent).count();
        result.push(Block { level: heading.level, heading: heading.text.clone(), parent, range: heading.start..end, nth });
    }
    result
}

/// The range of the block `key` names, or `None` when the file no longer has it.
pub fn find(text: &str, key: &BlockKey) -> Option<Range<usize>> {
    blocks(text)
        .into_iter()
        .find(|block| block.level == key.level && block.heading == key.heading && block.parent == key.parent && block.nth == key.nth)
        .map(|block| block.range)
}

fn bare(heading: &str) -> &str {
    heading.trim().trim_start_matches(|c: char| !c.is_alphabetic())
}

/// Whether a heading of the file is the one a topic names. Numbers, emoji and a trailing note do not matter.
pub fn heading_matches(found: &str, wanted: &str) -> bool {
    bare(found).to_lowercase().starts_with(&bare(wanted).to_lowercase())
}

/// `CLAUDE.md` holds much besides the profile, so its parent must be the named `##` and not one that only starts like it.
fn parent_matches(file: ProfileFile, found: &str, wanted: &str) -> bool {
    if file == ProfileFile::ClaudeMd { bare(found).to_lowercase() == bare(wanted).to_lowercase() } else { heading_matches(found, wanted) }
}

/// The blocks `section` selects among `blocks`.
pub fn select(blocks: &[Block], section: &SectionRef) -> Vec<Block> {
    blocks
        .iter()
        .filter(|block| match (section.parent, section.heading) {
            (None, None) => block.level == 2,
            (None, Some(heading)) => block.level == 2 && heading_matches(&block.heading, heading),
            (Some(parent), Some(heading)) => {
                block.level == 3
                    && heading_matches(&block.heading, heading)
                    && block.parent.as_deref().is_some_and(|found| parent_matches(section.file, found, parent))
            }
            (Some(_), None) => false,
        })
        .cloned()
        .collect()
}

pub fn section_blocks(text: &str, file: ProfileFile, section: &SectionRef) -> Vec<Block> {
    if section.file != file {
        return Vec::new();
    }
    select(&blocks(text), section)
}

/// The line ending the file uses around `at`, taken from the text before it, or from the whole text when that has no line break.
fn line_ending(text: &str, at: usize) -> &'static str {
    let before = &text[..at];
    let sample = if before.contains(['\n', '\r']) { before } else { text };
    if sample.contains("\r\n") {
        "\r\n"
    } else if sample.contains('\r') && !sample.contains('\n') {
        "\r"
    } else {
        "\n"
    }
}

fn with_ending(draft: &str, ending: &str) -> String {
    let unix = draft.replace("\r\n", "\n").replace('\r', "\n");
    if ending == "\n" { unix } else { unix.replace('\n', ending) }
}

/// Puts `draft` in place of the block at `range`. The bytes outside `range` stay as they are.
/// The blank lines that ended the old block stay too, so the result ends with a newline if and only if the old block did.
pub fn replace(text: &str, range: Range<usize>, draft: &str) -> String {
    let old = &text[range.clone()];
    let tail = &old[old.trim_end().len()..];
    let body = with_ending(draft.trim_end(), line_ending(text, range.start));
    format!("{}{}{}{}", &text[..range.start], body, tail, &text[range.end..])
}

/// Puts `draft` at `at`, after a blank line unless one is already there, and before one unless one follows.
pub fn insert_at(text: &str, at: usize, draft: &str) -> String {
    let ending = line_ending(text, at);
    let (before, after) = text.split_at(at);
    let tail = &before[before.trim_end_matches(['\n', '\r', ' ', '\t']).len()..];
    let breaks = tail.replace("\r\n", "\n").chars().filter(|c| matches!(c, '\n' | '\r')).count();
    let gap = if before.is_empty() { 0 } else { 2usize.saturating_sub(breaks) };
    let closing = if after.is_empty() || after.starts_with(['\n', '\r']) { 1 } else { 2 };
    format!("{before}{}{}{}{after}", ending.repeat(gap), with_ending(draft.trim_end(), ending), ending.repeat(closing))
}

/// The lines of `text`, each with its line ending. A lone `\r` ends a line too.
pub fn lines_with_endings(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < bytes.len() {
        let end = match bytes[at] {
            b'\n' => at + 1,
            b'\r' if bytes.get(at + 1) == Some(&b'\n') => at + 2,
            b'\r' => at + 1,
            _ => {
                at += 1;
                continue;
            }
        };
        lines.push(&text[start..end]);
        start = end;
        at = end;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// An ATX heading is one to six `#`, then a space or the end of the line.
pub fn is_atx(line: &str) -> bool {
    let line = line.trim_start();
    let rest = line.trim_start_matches('#');
    (1..=6).contains(&(line.len() - rest.len())) && (rest.trim_end_matches(['\r', '\n']).is_empty() || rest.starts_with([' ', '\t']))
}

fn is_underline(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty() && (line.chars().all(|c| c == '=') || line.chars().all(|c| c == '-'))
}

/// The length of the heading of a block's text, its line ending included.
/// A setext heading runs from the first line to its underline.
pub fn heading_len(block_text: &str) -> usize {
    let lines = lines_with_endings(block_text);
    let Some(first) = lines.first() else { return 0 };
    if is_atx(first) {
        return first.len();
    }
    match lines.iter().position(|line| is_underline(line)) {
        Some(at) if at > 0 => lines[..=at].iter().map(|line| line.len()).sum(),
        _ => first.len(),
    }
}

/// What a job heading such as `Lead Engineer - Calder Freight (2021 – present)` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleHeading<'a> {
    pub title: &'a str,
    pub company: &'a str,
    pub dates: &'a str,
}

pub fn role_heading(heading: &str) -> Option<RoleHeading<'_>> {
    let heading = heading.trim();
    if !heading.ends_with(')') {
        return None;
    }
    let mut depth = 0usize;
    let mut open = None;
    for (at, c) in heading.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    open = Some(at);
                    break;
                }
            }
            _ => {}
        }
    }
    let open = open?;
    let dates = heading[open + 1..heading.len() - 1].trim();
    let name = heading[..open].trim_end();
    let (dash, width) = [" - ", " – ", " — "].iter().filter_map(|dash| name.find(dash).map(|at| (at, dash.len()))).min()?;
    Some(RoleHeading { title: name[..dash].trim(), company: name[dash + width..].trim(), dates })
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREE: &str = "# Top\n\n## A\nalpha\n\n## B\nbeta\n\n### B1\none\n\n### B2\ntwo\n\n## C\ngamma\n";

    fn key(text: &str, heading: &str, level: u8) -> BlockKey {
        let block = blocks(text).into_iter().find(|block| block.heading == heading && block.level == level).unwrap();
        BlockKey { file: ProfileFile::Candidate, level, heading: block.heading, parent: block.parent, nth: block.nth }
    }

    #[test]
    fn a_heading_in_a_fence_is_not_a_block() {
        let text = "## Real\ntext\n\n```md\n## Fake\nmore\n```\n\n## Next\nx\n";
        let found: Vec<String> = blocks(text).into_iter().map(|block| block.heading).collect();
        assert_eq!(found, ["Real", "Next"]);
        assert!(blocks(text)[0].range.end == text.find("## Next").unwrap());
    }

    #[test]
    fn a_heading_in_a_quote_or_list_is_not_a_block() {
        let text = "## Real\n> ## Quoted\n\n- ## Listed\n\n## Next\n";
        assert_eq!(blocks(text).len(), 2);
    }

    #[test]
    fn block_ranges_nest_and_count_repeats() {
        let found = blocks(THREE);
        let b = &found[1];
        assert_eq!((b.level, b.heading.as_str(), b.parent.clone()), (2, "B", None));
        assert_eq!(&THREE[b.range.clone()], "## B\nbeta\n\n### B1\none\n\n### B2\ntwo\n\n");
        assert_eq!(&THREE[found[2].range.clone()], "### B1\none\n\n");
        assert_eq!(found[2].parent.as_deref(), Some("B"));
        assert_eq!(&THREE[found[4].range.clone()], "## C\ngamma\n");
        let twice = blocks("## X\na\n## X\nb\n");
        assert_eq!((twice[0].nth, twice[1].nth), (0, 1));
    }

    #[test]
    fn a_level_four_heading_stays_inside_its_block() {
        let text = "## A\n### B\n#### D\nx\n### E\n";
        let found = blocks(text);
        assert_eq!(found.len(), 3);
        assert_eq!(&text[found[1].range.clone()], "### B\n#### D\nx\n");
    }

    #[test]
    fn replace_keeps_every_byte_outside_the_block() {
        for text in [THREE.to_owned(), THREE.replace('\n', "\r\n")] {
            let range = find(&text, &key(&text, "B", 2)).unwrap();
            let result = replace(&text, range.clone(), "## B\nnew\nlines\n");
            assert_eq!(&result[..range.start], &text[..range.start]);
            assert_eq!(&result[result.len() - (text.len() - range.end)..], &text[range.end..]);
            assert!(result.contains(if text.contains('\r') { "new\r\nlines\r\n\r\n## C" } else { "new\nlines\n\n## C" }));
            assert!(!result.contains("\r\r"));
        }
    }

    #[test]
    fn replace_ends_with_a_newline_only_when_the_block_did() {
        let with = "## A\nx\n";
        assert_eq!(replace(with, 0..with.len(), "## A\ny"), "## A\ny\n");
        let without = "## A\nx";
        assert_eq!(replace(without, 0..without.len(), "## A\ny\n\n"), "## A\ny");
    }

    #[test]
    fn insert_adds_a_blank_line_only_when_missing() {
        assert_eq!(insert_at("## A\nx\n", 7, "## N\ny"), "## A\nx\n\n## N\ny\n");
        assert_eq!(insert_at("## A\nx\n\n", 8, "## N\ny\n"), "## A\nx\n\n## N\ny\n");
        assert_eq!(insert_at("## A\nx\n\n## C\n", 8, "## N\ny"), "## A\nx\n\n## N\ny\n\n## C\n");
        assert_eq!(insert_at("## A\r\nx\r\n", 9, "## N\ny"), "## A\r\nx\r\n\r\n## N\r\ny\r\n");
        assert_eq!(insert_at("", 0, "## N"), "## N\n");
    }

    #[test]
    fn find_locates_blocks_by_their_key() {
        let text = "## X\na\n## X\nb\n";
        let second = BlockKey { file: ProfileFile::Candidate, level: 2, heading: "X".into(), parent: None, nth: 1 };
        assert_eq!(find(text, &second), Some(7..text.len()));
        let gone = BlockKey { nth: 2, ..second };
        assert_eq!(find(text, &gone), None);
    }

    #[test]
    fn a_section_names_its_parent() {
        let text = "## Scoring Dimensions\n### 1. Technical Skills Match (0-100)\nx\n### 2. Other\ny\n## Elsewhere\n### Technical Skills Match\nz\n";
        let section = SectionRef { file: ProfileFile::Evaluation, parent: Some("Scoring Dimensions"), heading: Some("Technical Skills Match") };
        let found = section_blocks(text, ProfileFile::Evaluation, &section);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].heading, "1. Technical Skills Match (0-100)");
        let elsewhere = SectionRef { parent: Some("Elsewhere"), ..section };
        assert_eq!(section_blocks(text, ProfileFile::Evaluation, &elsewhere)[0].heading, "Technical Skills Match");
        let other = SectionRef { parent: Some("Nothing"), ..section };
        assert!(section_blocks(text, ProfileFile::Evaluation, &other).is_empty());
        assert!(section_blocks(text, ProfileFile::Candidate, &section).is_empty());
    }

    #[test]
    fn headings_match_without_numbers_emoji_or_notes() {
        assert!(heading_matches("💼 Professional Experience", "Professional Experience"));
        assert!(heading_matches("4. Location & Logistics (Pass/Fail + Notes)", "Location & Logistics"));
        assert!(heading_matches("Profile Statement / Elevator Pitch (Best Practice)", "Profile Statement / Elevator Pitch"));
        assert!(heading_matches("education", "Education"));
        assert!(!heading_matches("Skills", "Education"));
    }

    #[test]
    fn a_role_heading_has_title_company_and_dates() {
        let role = role_heading("Lead Engineer - Calder Freight (2021 – present)").unwrap();
        assert_eq!((role.title, role.company, role.dates), ("Lead Engineer", "Calder Freight", "2021 – present"));
        let template = role_heading("[JOB_TITLE] - [COMPANY] ([START] - [END])").unwrap();
        assert_eq!((template.title, template.company, template.dates), ("[JOB_TITLE]", "[COMPANY]", "[START] - [END]"));
        let nested = role_heading("Dev — Acme (Pty) Ltd (2019 (spring) – 2020)").unwrap();
        assert_eq!((nested.company, nested.dates), ("Acme (Pty) Ltd", "2019 (spring) – 2020"));
        assert_eq!(role_heading("Languages"), None);
        assert_eq!(role_heading("Languages (spoken)"), None);
        assert_eq!(role_heading("Dev - Acme 2020)"), None);
    }

    #[test]
    fn the_heading_line_ends_before_the_body() {
        assert_eq!(heading_len("## A\nbody\n"), 5);
        assert_eq!(heading_len("A\n---\nbody\n"), 6);
        assert_eq!(heading_len("## A"), 4);
        assert_eq!(heading_len("Line one\nLine two\n---\nbody\n"), 22);
        assert_eq!(heading_len("#tag\n---\nbody\n"), 9);
        assert_eq!(heading_len("## A\rbody\r"), 5);
    }

    #[test]
    fn front_matter_is_not_a_block() {
        let text = "---\nframework_version: 1.1.1\n---\n\n# Behavioral Profile\n\n## How\nx\n";
        let found = blocks(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].heading, "How");
        assert_eq!(found[0].range.start, text.find("## How").unwrap());
        assert!(blocks("---\nframework_version: 1.1.1\n---\n\n# Title\n").is_empty());
    }

    #[test]
    fn a_lone_carriage_return_ends_a_line() {
        let text = "## A\rx\r## B\ry\r";
        let found = blocks(text);
        assert_eq!((found[0].range.clone(), found[1].range.clone()), (0..7, 7..text.len()));
        assert_eq!(replace(text, found[0].range.clone(), "## A\nz"), "## A\rz\r## B\ry\r");
        assert_eq!(insert_at("## A\r", 5, "## N"), "## A\r\r## N\r");
    }

    #[test]
    fn a_claude_md_parent_must_match_whole() {
        let text = "## Candidate Profile Rules\n### Identity\nno\n## Candidate Profile\n### Identity\nyes\n";
        let section = SectionRef { file: ProfileFile::ClaudeMd, parent: Some("Candidate Profile"), heading: Some("Identity") };
        let found = section_blocks(text, ProfileFile::ClaudeMd, &section);
        assert_eq!(found.len(), 1);
        assert_eq!(&text[found[0].range.clone()], "### Identity\nyes\n");
        let numbered = "## 🧑 2. Candidate profile\n### Identity\nyes\n";
        assert_eq!(section_blocks(numbered, ProfileFile::ClaudeMd, &section).len(), 1);
    }
}
