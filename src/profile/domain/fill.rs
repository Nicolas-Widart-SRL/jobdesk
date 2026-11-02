use std::ops::Range;

use crate::profile::domain::document::{BlockKey, find, lines_with_endings};
use crate::shared_kernel::placeholders::{placeholder_count, placeholder_spans};

/// How much of a topic the user has written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    Filled,
    ToFill(usize),
    NotStarted,
    /// A file of the topic could not be read, so its state is unknown.
    Hidden,
}

/// `blocks` are the texts of the topic's top-level blocks.
/// A topic is not started when it has no blocks or no line of its own that is not a placeholder.
pub fn topic_fill(blocks: &[&str], readable: bool) -> Fill {
    if !readable {
        return Fill::Hidden;
    }
    if !blocks.iter().any(|block| has_writing(block)) {
        return Fill::NotStarted;
    }
    match blocks.iter().map(|block| placeholder_count(block)).sum() {
        0 => Fill::Filled,
        left => Fill::ToFill(left),
    }
}

/// Whether a block holds a line the user wrote: one with words and no placeholder.
/// A heading, a comment, a label with nothing after it, and a table's header, rule and border carry no writing.
fn has_writing(block: &str) -> bool {
    let text = without_comments(block);
    let lines = lines_with_endings(&text);
    lines.iter().enumerate().any(|(at, line)| {
        let header = line.contains('|') && lines.get(at + 1).is_some_and(|next| is_table_rule(next));
        is_content(line) && !is_label_only(line) && !header && placeholder_spans(line).is_empty()
    })
}

fn without_comments(text: &str) -> String {
    let mut kept = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        kept.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => return kept,
        }
    }
    kept.push_str(rest);
    kept
}

fn is_content(line: &str) -> bool {
    !line.trim_start().starts_with('#') && line.chars().any(char::is_alphanumeric)
}

/// `- **Languages:**` and `Skills:` name a field and say nothing yet.
fn is_label_only(line: &str) -> bool {
    let line = line.trim();
    let line = ["- ", "* ", "+ "].iter().find_map(|marker| line.strip_prefix(marker)).unwrap_or(line);
    line.trim_matches(['*', '_', ' ', '\t']).ends_with(':')
}

/// The line under a table's header row, such as `| --- | :-: |`.
fn is_table_rule(line: &str) -> bool {
    let line = line.trim();
    line.contains('-') && line.contains('|') && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// What a placeholder token reads as on screen, such as `to fill: first name` for `FIRST_NAME`.
pub fn blank_words(token: &str) -> String {
    let token = token.trim_matches(['[', ']']);
    if token == "PLACEHOLDER" {
        return "to fill".to_owned();
    }
    format!("to fill: {}", token.replace('_', " ").to_lowercase())
}

/// What a save finds in the file compared with what the user opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveCheck {
    /// The block is as opened, here.
    Same(Range<usize>),
    /// The block changed on disk since the user opened it.
    Changed { now: String },
    Gone,
}

/// `opened_with` is the text of the block when the user opened it.
pub fn check(now_text: &str, key: &BlockKey, opened_with: &str) -> SaveCheck {
    let Some(range) = find(now_text, key) else { return SaveCheck::Gone };
    let now = now_text[range.clone()].trim_end();
    if now == opened_with.trim_end() { SaveCheck::Same(range) } else { SaveCheck::Changed { now: now.to_owned() } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::domain::document::blocks;
    use crate::profile::domain::topic::ProfileFile;

    fn key_of(text: &str, heading: &str) -> (BlockKey, String) {
        let block = blocks(text).into_iter().find(|block| block.heading == heading).unwrap();
        let opened = text[block.range.clone()].trim_end().to_owned();
        (BlockKey { file: ProfileFile::Candidate, level: block.level, heading: block.heading, parent: block.parent, nth: block.nth }, opened)
    }

    #[test]
    fn fill_follows_the_placeholders() {
        assert_eq!(topic_fill(&["## Identity\n\n[FIRST_NAME] [LAST_NAME]\n| --- | --- |\n[EMAIL]\n"], true), Fill::NotStarted);
        assert_eq!(topic_fill(&[], true), Fill::NotStarted);
        assert_eq!(topic_fill(&["## Identity\nCity: Lisbon\nName: [FIRST_NAME] [LAST_NAME]\n"], true), Fill::ToFill(2));
        assert_eq!(topic_fill(&["## Identity\nName: Ada Quill\n"], true), Fill::Filled);
        assert_eq!(topic_fill(&["## Identity\nName: Ada Quill\n"], false), Fill::Hidden);
        assert_eq!(topic_fill(&["## A\n[PLACEHOLDER]\n", "## B\nwritten [X_Y]\nmore\n"], true), Fill::ToFill(2));
    }

    #[test]
    fn comments_labels_and_table_headers_are_not_writing() {
        let comment = "### Education\n<!-- List your degrees, most recent first -->\n- **[DEGREE_LEVEL] in [FIELD]** at [SCHOOL]\n";
        assert_eq!(topic_fill(&[comment], true), Fill::NotStarted);
        let many = "### Education\n<!--\nList your degrees,\nmost recent first\n-->\n- **[DEGREE_LEVEL] in [FIELD]**\n";
        assert_eq!(topic_fill(&[many], true), Fill::NotStarted);
        let labels = "### Identity\n- **Languages:**\n| Language | Level |\n| --- | :-: |\n";
        assert_eq!(topic_fill(&[labels], true), Fill::NotStarted);
        let loose = "### Identity\nSkills:\nLanguage | Level\n--- | ---\n";
        assert_eq!(topic_fill(&[loose], true), Fill::NotStarted);
        let written = "### Identity\n<!-- hint -->\n- **Languages:** English, French\n| Language | Level |\n| --- | --- |\n| English | Native |\n";
        assert_eq!(topic_fill(&[written], true), Fill::Filled);
        let lone = "### Identity\r<!-- hint -->\r- **Languages:**\r- [LANGUAGE]\r";
        assert_eq!(topic_fill(&[lone], true), Fill::NotStarted);
        let lone_written = "### Identity\r- **Languages:** English\r";
        assert_eq!(topic_fill(&[lone_written], true), Fill::Filled);
    }

    #[test]
    fn blank_words_turn_a_token_into_words() {
        assert_eq!(blank_words("FIRST_NAME"), "to fill: first name");
        assert_eq!(blank_words("PLACEHOLDER"), "to fill");
        assert_eq!(blank_words("[LAST_NAME]"), "to fill: last name");
    }

    #[test]
    fn a_save_sees_only_changes_to_its_own_block() {
        let opened_text = "## A\nalpha\n\n## B\nbeta\n\n## C\ngamma\n";
        let (key, opened) = key_of(opened_text, "B");
        assert_eq!(check(opened_text, &key, &opened), SaveCheck::Same(opened_text.find("## B").unwrap()..opened_text.find("## C").unwrap()));
        let other = opened_text.replace("alpha", "alpha changed");
        assert!(matches!(check(&other, &key, &opened), SaveCheck::Same(_)));
        let edited = opened_text.replace("beta", "beta changed");
        assert_eq!(check(&edited, &key, &opened), SaveCheck::Changed { now: "## B\nbeta changed".to_owned() });
        let deleted = "## A\nalpha\n\n## C\ngamma\n";
        assert_eq!(check(deleted, &key, &opened), SaveCheck::Gone);
    }
}
