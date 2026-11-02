use std::ops::Range;

use crate::profile::domain::document::{BlockKey, find, heading_len};
use crate::shared_kernel::placeholders::{placeholder_count, shows_content};

/// How much of a topic the user has written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    Filled,
    ToFill(usize),
    NotStarted,
    /// A file of the topic could not be read, so its state is unknown.
    Hidden,
}

/// `blocks` are the texts of the topic's top-level blocks, each starting with its heading.
/// A topic is not started when it has no blocks, or when no block shows anything under its headings: no text, code or
/// HTML, only blank lines and HTML that holds only comments (`shows_content`).
/// Otherwise it has as much left to fill as it has setup tokens.
pub fn topic_fill(blocks: &[&str], readable: bool) -> Fill {
    if !readable {
        return Fill::Hidden;
    }
    if blocks.iter().all(|block| body_is_empty(block)) {
        return Fill::NotStarted;
    }
    match blocks.iter().map(|block| open_placeholders(block)).sum() {
        0 => Fill::Filled,
        left => Fill::ToFill(left),
    }
}

/// The setup tokens left in a block: the tokens its page shows.
fn open_placeholders(block: &str) -> usize {
    placeholder_count(block)
}

/// Under its heading a block shows no text or code: only blank lines, comments, other HTML and the headings of its children.
fn body_is_empty(block: &str) -> bool {
    !shows_content(&block[heading_len(block).min(block.len())..])
}

/// What a save finds in the file compared with what the user opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveCheck {
    /// The block is as opened, here.
    Same(Range<usize>),
    /// The block changed on disk since the user opened it.
    Changed {
        now: String,
    },
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
    fn fill_follows_the_tokens_left() {
        assert_eq!(topic_fill(&[], true), Fill::NotStarted);
        assert_eq!(topic_fill(&["## Identity\nCity: Lisbon\nName: [FIRST_NAME] [LAST_NAME]\n"], true), Fill::ToFill(2));
        assert_eq!(topic_fill(&["## Identity\n\n[FIRST_NAME] [LAST_NAME]\n"], true), Fill::ToFill(2));
        assert_eq!(topic_fill(&["## Identity\nName: Ada Quill\n"], true), Fill::Filled);
        assert_eq!(topic_fill(&["## Identity\nName: Ada Quill\n"], false), Fill::Hidden);
        assert_eq!(topic_fill(&[], false), Fill::Hidden);
        assert_eq!(topic_fill(&["## A\n[PLACEHOLDER]\n", "## B\nwritten [X_Y]\nmore\n"], true), Fill::ToFill(2));
    }

    #[test]
    fn a_topic_with_only_comments_under_its_headings_is_not_started() {
        assert_eq!(topic_fill(&["## Identity\n"], true), Fill::NotStarted);
        assert_eq!(topic_fill(&["## Identity"], true), Fill::NotStarted);
        assert_eq!(topic_fill(&["## Identity\n\n<!-- List your degrees, [DEGREE] first -->\n\n"], true), Fill::NotStarted);
        assert_eq!(topic_fill(&["## Identity\n<!--\nmany\nlines [X_Y]\n-->\n", "### Education\r<!-- hint -->\r"], true), Fill::NotStarted);
        assert_eq!(topic_fill(&["## Identity\n<!-- hint -->\n", "## Other\nSome text\n"], true), Fill::Filled);
        assert_eq!(topic_fill(&["## Experience\n<!-- hint -->\n### Role\n"], true), Fill::NotStarted);
        assert_eq!(topic_fill(&["## Experience\n<!-- hint -->\n### Role\nShipped a thing.\n"], true), Fill::Filled);
    }

    #[test]
    fn a_section_of_empty_child_headings_is_not_started() {
        let text = "## Technical Skills\n\n### Programming & ML\n\n### Domain Expertise\n\n### Software & Tools\n";
        assert_eq!(topic_fill(&[text], true), Fill::NotStarted);
        let tokens = "## Technical Skills\n\n### [JOB_TITLE] - [COMPANY] ([START] - [END])\n\n### Tools\n";
        assert_eq!(topic_fill(&[tokens], true), Fill::NotStarted);
        let written = "## Technical Skills\n\n### Programming & ML\nRust, Python\n\n### Domain Expertise\n";
        assert_eq!(topic_fill(&[written], true), Fill::Filled);
    }

    #[test]
    fn a_token_in_a_comment_is_not_counted() {
        let text = "### Identity\n<!-- Format: [CITY], [COUNTRY] -->\nLisbon, Portugal\n";
        assert_eq!(topic_fill(&[text], true), Fill::Filled);
        assert_eq!(open_placeholders(text), 0);
        assert_eq!(open_placeholders("a [CITY]\n<!--\n[X_Y]\n-->\n"), 1);
    }

    #[test]
    fn the_wider_token_shapes_count() {
        let text = "## Experience\n- [RESPONSIBILITY_OR_ACHIEVEMENT_1]\n- [CONTEXT - what was happening]\n| [LEVEL, e.g. \"Native\" / \"C2\"] |\n[Jobdesk](https://x.example) [x][ref]\n- [ ] task\n";
        assert_eq!(topic_fill(&[text], true), Fill::ToFill(3));
        assert_eq!(open_placeholders(text), 3);
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
