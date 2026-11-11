//! Reads one of `/setup`'s messages. Claude asks in plain text, never through a prompt: a batch of numbered questions,
//! a proposal of numbered changes that ends by asking to confirm, or a few short follow-ups. This finds the numbered
//! items, so a sheet can put a field under each and the reply can be built from their numbers.

use std::collections::HashSet;
use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::shared_kernel::markdown;
use crate::shared_kernel::visible_text::visible_chars;

/// What one message of `/setup` is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Numbered questions, for a sheet with a field under each.
    Questions(Batch),
    /// Numbered changes that end by asking to confirm, for the review card.
    Proposal(Batch),
    /// Anything else: no numbered items, fewer than two, or numbers that do not run 1 to N across the whole message.
    Free,
}

/// The numbered items of a message, in Claude's order, in the groups Claude gave them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub groups: Vec<Group>,
}

impl Batch {
    /// Every item, in Claude's order.
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.groups.iter().flat_map(|group| group.items.iter())
    }

    /// Every item's number, in Claude's order, for the replies.
    pub fn numbers(&self) -> Vec<u32> {
        self.items().map(|item| item.number).collect()
    }
}

/// Items under one title: a heading or a bold-only line that came before them. The items before any title have none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub title: Option<String>,
    pub items: Vec<Item>,
}

/// One numbered item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The number Claude wrote, which is also the list's start plus the item's place in it.
    pub number: u32,
    /// The item as Markdown, without its number, nested lists and lettered lines included, to show as Claude wrote it.
    pub body: String,
    /// The item as one line of plain text, nested lists and lettered lines included.
    pub text: String,
    /// Whether the item's first paragraph, or its first line, ends in "(optional)".
    pub optional: bool,
}

/// Reads `message`. Every kind of line break splits a line, then hidden characters are removed from each, so one inside
/// a number cannot hide the item. The message is `Free` unless each item's typed number is its list's start plus its
/// place, and the numbers run 1 to N across the whole message.
pub fn read_message(message: &str) -> Message {
    let unified = message.replace("\r\n", "\n").replace(['\r', '\u{85}', '\u{2028}', '\u{2029}'], "\n");
    let text = unified.lines().map(|line| visible_chars(line).collect::<String>()).collect::<Vec<_>>().join("\n");
    let Some(scan) = scan(&text) else { return Message::Free };
    let numbered = scan.groups.iter().flat_map(|group| group.items.iter()).enumerate().all(|(at, item)| u64::from(item.number) == at as u64 + 1);
    let batch = Batch { groups: scan.groups };
    if batch.items().count() < 2 || !numbered {
        return Message::Free;
    }
    let asks = scan.closing.as_deref().is_some_and(asks_to_apply);
    if asks && batch.items().all(|item| !item.text.contains('?')) { Message::Proposal(batch) } else { Message::Questions(batch) }
}

/// Whether the closing paragraph holds "apply" as a word, in any case. "Confirm" and "yes" alone do not make a proposal,
/// because a batch of questions can ask to confirm too, and "Yes, apply these changes." answers only a proposal.
fn asks_to_apply(closing: &str) -> bool {
    closing.split(|c: char| !c.is_alphanumeric()).any(|word| word.eq_ignore_ascii_case("apply"))
}

struct Scan {
    groups: Vec<Group>,
    /// The last paragraph outside any list since the last numbered list.
    closing: Option<String>,
}

/// A paragraph or heading outside any list, being read.
#[derive(Default)]
struct Block {
    heading: bool,
    range: Range<usize>,
    text: String,
    strong: usize,
    /// Text outside bold, so a bold-only line is told from a line that only holds some bold.
    plain_text: bool,
    bold_text: bool,
}

/// An item of a top-level numbered list, being read.
struct Open {
    number: Option<u32>,
    start: usize,
    end: usize,
    text: String,
    first: String,
    first_line: String,
    /// The first paragraph has ended, by a break between blocks or by a nested list.
    first_done: bool,
    line_done: bool,
}

/// The numbered items of the top-level ordered lists of `text`, or `None` when a number does not fit.
fn scan(text: &str) -> Option<Scan> {
    let mut groups: Vec<Group> = Vec::new();
    let mut title: Option<String> = None;
    let mut closing = None;
    let mut block: Option<Block> = None;
    let mut open: Option<Open> = None;
    // How many lists, items, quotes, tables and footnotes the parser is inside. Only depth 0 is top level.
    let mut depth = 0usize;
    // The number the next item of the top-level ordered list gets, while inside one.
    let mut next: Option<u64> = None;
    // Whether the last block was a numbered list, so lettered lines that come next belong to its last item.
    let mut after_item = false;
    let mut code: Vec<Range<usize>> = Vec::new();
    let mut item_starts: HashSet<usize> = HashSet::new();
    for (event, range) in Parser::new_ext(text, markdown::options()).into_offset_iter() {
        match event {
            Event::Start(Tag::List(start)) => {
                if depth == 0 {
                    next = start;
                    after_item = false;
                    if start.is_some() {
                        closing = None;
                        match title.take() {
                            Some(title) => groups.push(Group { title: Some(title), items: Vec::new() }),
                            None if groups.is_empty() => groups.push(Group { title: None, items: Vec::new() }),
                            None => {}
                        }
                    }
                }
                if let Some(item) = &mut open {
                    item.first_done = true;
                    item.text.push(' ');
                }
                depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    after_item = next.take().is_some();
                }
            }
            Event::Start(Tag::Item) => {
                item_starts.insert(marker_at(text, range.start));
                if depth == 1
                    && let Some(number) = next.as_mut()
                {
                    let at = *number;
                    *number += 1;
                    // Markdown numbers a list by its first number alone, so the number Claude typed is read from the text.
                    if typed_number(text, range.start) != Some(at) {
                        return None;
                    }
                    open = Some(Open {
                        number: u32::try_from(at).ok(),
                        start: range.start,
                        end: range.end,
                        text: String::new(),
                        first: String::new(),
                        first_line: String::new(),
                        first_done: false,
                        line_done: false,
                    });
                } else if let Some(item) = &mut open {
                    item.text.push(' ');
                }
                depth += 1;
            }
            Event::End(TagEnd::Item) => {
                depth = depth.saturating_sub(1);
                if depth == 1
                    && let Some(item) = open.take()
                {
                    let group = groups.last_mut()?;
                    group.items.push(finish(item, text)?);
                }
            }
            Event::Start(Tag::CodeBlock(_)) => code.push(range),
            Event::Start(tag) if holds_blocks(&tag) => depth += 1,
            Event::End(tag) if ends_blocks(&tag) => depth = depth.saturating_sub(1),
            Event::Start(Tag::Paragraph) | Event::Start(Tag::Heading { .. }) if depth == 0 => {
                block = Some(Block { heading: !matches!(event, Event::Start(Tag::Paragraph)), range, ..Block::default() });
            }
            Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Heading(_)) if depth == 0 => {
                if let Some(done) = block.take() {
                    let line = done.text.split_whitespace().collect::<Vec<_>>().join(" ");
                    let source = text[done.range.clone()].trim();
                    let last = groups.last_mut().and_then(|group| group.items.last_mut());
                    match last {
                        // Lettered lines set apart from their question by a blank line still belong to it.
                        Some(item) if after_item && !done.heading && lettered(source) => {
                            item.body.push_str("\n\n");
                            item.body.push_str(source);
                            item.text.push(' ');
                            item.text.push_str(&line);
                        }
                        _ => {
                            after_item = false;
                            if done.heading || (done.bold_text && !done.plain_text) {
                                title = Some(line.clone()).filter(|line| !line.is_empty()).or(title);
                            } else {
                                title = None;
                            }
                            if !done.heading {
                                closing = Some(line);
                            }
                        }
                    }
                }
            }
            Event::Start(Tag::Strong) => block.iter_mut().for_each(|block| block.strong += 1),
            Event::End(TagEnd::Strong) => block.iter_mut().for_each(|block| block.strong = block.strong.saturating_sub(1)),
            Event::Start(Tag::Paragraph) => {
                if let Some(item) = &mut open {
                    item.first_done |= !item.text.trim().is_empty();
                    item.text.push(' ');
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if let Some(item) = &mut open {
                    item.first_done = true;
                    item.text.push(' ');
                }
            }
            Event::Text(part) | Event::Code(part) => {
                if let Some(item) = &mut open {
                    item.text.push_str(&part);
                    if !item.first_done {
                        item.first.push_str(&part);
                        if !item.line_done {
                            item.first_line.push_str(&part);
                        }
                    }
                } else if let Some(block) = &mut block {
                    block.text.push_str(&part);
                    if !part.trim().is_empty() {
                        if block.strong == 0 {
                            block.plain_text = true;
                        } else {
                            block.bold_text = true;
                        }
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(item) = &mut open {
                    item.text.push(' ');
                    if !item.first_done {
                        item.first.push(' ');
                        item.line_done = true;
                    }
                } else if let Some(block) = &mut block {
                    block.text.push(' ');
                }
            }
            _ => {}
        }
    }
    if hides_numbered_line(text, &code, &item_starts) {
        return None;
    }
    Some(Scan { groups, closing })
}

/// Where the item that starts at `at` has its marker: its range may begin on the indent before it.
fn marker_at(text: &str, at: usize) -> usize {
    let rest = &text[at..];
    at + rest.len() - rest.trim_start_matches([' ', '\t']).len()
}

/// The number typed at the start of the item at `at`.
fn typed_number(text: &str, at: usize) -> Option<u64> {
    let rest = &text[marker_at(text, at)..];
    rest[..rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len()].parse().ok()
}

/// Whether every line of `source` is a letter, a "." or ")" and some text, as in "a. Individual contributor".
fn lettered(source: &str) -> bool {
    source.lines().all(|line| {
        let mut chars = line.trim().chars();
        chars.next().is_some_and(|letter| letter.is_ascii_alphabetic()) && matches!(chars.next(), Some('.' | ')')) && chars.next() == Some(' ')
    })
}

/// Whether a line outside any code starts like a numbered item and yet is no item the parser found, nested or not. One is
/// hiding inside a paragraph, where Markdown does not start a list at a number but 1.
fn hides_numbered_line(text: &str, code: &[Range<usize>], item_starts: &HashSet<usize>) -> bool {
    let mut at = 0;
    for line in text.split('\n') {
        let marker = at + line.len() - line.trim_start_matches([' ', '\t']).len();
        at += line.len() + 1;
        let rest = &text[marker..at.min(text.len())];
        let rest = rest.trim_end_matches('\n');
        let after_digits = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        let numbered =
            after_digits.len() < rest.len() && after_digits.strip_prefix(['.', ')']).is_some_and(|tail| tail.is_empty() || tail.starts_with([' ', '\t']));
        if numbered && !item_starts.contains(&marker) && !code.iter().any(|block| block.contains(&marker)) {
            return true;
        }
    }
    false
}

/// The item read, or `None` when its number does not fit.
fn finish(item: Open, source: &str) -> Option<Item> {
    Some(Item {
        number: item.number?,
        body: body(&source[item.start..item.end]),
        text: item.text.split_whitespace().collect::<Vec<_>>().join(" "),
        optional: ends_optional(&item.first_line) || ends_optional(&item.first),
    })
}

fn ends_optional(text: &str) -> bool {
    text.trim_end().to_lowercase().ends_with("(optional)")
}

/// The item's source without its number, and with the lines after the first moved left by the width of the number.
fn body(source: &str) -> String {
    let source = source.trim_end();
    let digits = source.len() - source.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let after = &source[digits..];
    let after = after.strip_prefix(['.', ')']).unwrap_or(after);
    let content = after.trim_start_matches([' ', '\t']);
    let width = digits + (source.len() - digits - after.len()) + (after.len() - content.len());
    let mut lines = content.split('\n');
    let mut body = lines.next().unwrap_or_default().to_owned();
    for line in lines {
        body.push('\n');
        let indent = line.len() - line.trim_start_matches(' ').len();
        body.push_str(&line[indent.min(width)..]);
    }
    body
}

fn holds_blocks(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::BlockQuote(_) | Tag::FootnoteDefinition(_) | Tag::Table(_) | Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition
    )
}

fn ends_blocks(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::BlockQuote(_)
            | TagEnd::FootnoteDefinition
            | TagEnd::Table
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIVE: &str = include_str!("../../../tests/fixtures/setup/five_questions.md");
    const THIRTEEN: &str = include_str!("../../../tests/fixtures/setup/thirteen_questions.md");
    const SIXTEEN: &str = include_str!("../../../tests/fixtures/setup/sixteen_changes.md");
    const FOLLOW_UPS: &str = include_str!("../../../tests/fixtures/setup/follow_ups.md");
    const UNNUMBERED: &str = include_str!("../../../tests/fixtures/setup/unnumbered.md");
    const GAP: &str = include_str!("../../../tests/fixtures/setup/gap.md");

    fn batch(message: Message) -> Batch {
        match message {
            Message::Questions(batch) | Message::Proposal(batch) => batch,
            Message::Free => panic!("expected numbered items"),
        }
    }

    fn numbers(batch: &Batch) -> Vec<u32> {
        batch.items().map(|item| item.number).collect()
    }

    #[test]
    fn five_questions_are_a_batch_of_questions() {
        let message = read_message(FIVE);
        assert!(matches!(message, Message::Questions(_)));
        let batch = batch(message);
        assert_eq!(numbers(&batch), [1, 2, 3, 4, 5]);
        assert_eq!(batch.groups.len(), 1);
        assert_eq!(batch.groups[0].title, None);
        let optional: Vec<u32> = batch.items().filter(|item| item.optional).map(|item| item.number).collect();
        assert_eq!(optional, [4]);
        assert!(batch.items().next().unwrap().text.starts_with("Your CV says \"Senior Designer\" at Alder Works"));
    }

    #[test]
    fn thirteen_questions_sit_under_five_bold_headings() {
        let message = read_message(THIRTEEN);
        assert!(matches!(message, Message::Questions(_)));
        let batch = batch(message);
        assert_eq!(numbers(&batch), (1..=13).collect::<Vec<u32>>());
        let titles: Vec<_> = batch.groups.iter().map(|group| group.title.as_deref()).collect();
        assert_eq!(
            titles,
            [
                Some("About the roles you want"),
                Some("About the companies"),
                Some("About where and when"),
                Some("About pay"),
                Some("About the application documents"),
            ]
        );
        let sizes: Vec<usize> = batch.groups.iter().map(|group| group.items.len()).collect();
        assert_eq!(sizes, [3, 3, 3, 2, 2]);
        let optional: Vec<u32> = batch.items().filter(|item| item.optional).map(|item| item.number).collect();
        assert_eq!(optional, [3, 9, 11]);
    }

    #[test]
    fn lettered_lines_stay_inside_their_item() {
        let batch = batch(read_message(THIRTEEN));
        let two = batch.items().find(|item| item.number == 2).unwrap();
        assert_eq!(two.text, "Which of these fit you best? a. Individual contributor with deep craft b. Lead of a small team");
        assert_eq!(two.body, "Which of these fit you best?\na. Individual contributor with deep craft\nb. Lead of a small team");
        let seven = batch.items().find(|item| item.number == 7).unwrap();
        assert!(seven.text.contains("b. Longest commute you accept"));
        assert!(!seven.optional);
        assert_eq!(batch.items().count(), 13);
    }

    #[test]
    fn a_nested_list_stays_inside_its_item() {
        let message = read_message("Questions.\n\n1. Which cities?\n   - Lowmere\n   - Port Arden\n2. Remote?\n");
        let batch = batch(message);
        assert_eq!(numbers(&batch), [1, 2]);
        let first = batch.items().next().unwrap();
        assert_eq!(first.text, "Which cities? Lowmere Port Arden");
        assert_eq!(first.body, "Which cities?\n- Lowmere\n- Port Arden");
    }

    #[test]
    fn sixteen_changes_that_end_by_asking_are_a_proposal() {
        let message = read_message(SIXTEEN);
        assert!(matches!(message, Message::Proposal(_)));
        let batch = batch(message);
        assert_eq!(numbers(&batch), (1..=16).collect::<Vec<u32>>());
        let titles: Vec<_> = batch.groups.iter().map(|group| group.title.as_deref()).collect();
        assert_eq!(
            titles,
            [
                Some("CLAUDE.md"),
                Some(".claude/skills/job-application-assistant/"),
                Some("cv/"),
                Some(".claude/skills/job-scraper/search-queries.md"),
                Some(".agents/skills/"),
            ]
        );
        let sizes: Vec<usize> = batch.groups.iter().map(|group| group.items.len()).collect();
        assert_eq!(sizes, [5, 5, 2, 2, 2]);
    }

    #[test]
    fn two_follow_ups_are_questions() {
        let message = read_message(FOLLOW_UPS);
        let batch = batch(message);
        assert_eq!(numbers(&batch), [1, 2]);
        assert_eq!(batch.items().map(|item| item.optional).collect::<Vec<_>>(), [false, true]);
    }

    #[test]
    fn a_message_without_numbers_is_free() {
        assert_eq!(read_message(UNNUMBERED), Message::Free);
        assert_eq!(read_message(""), Message::Free);
    }

    #[test]
    fn numbers_with_a_gap_are_free() {
        assert_eq!(read_message(GAP), Message::Free);
    }

    #[test]
    fn a_gap_or_a_repeat_inside_one_list_is_free() {
        assert_eq!(read_message("1. First?\n2. Second?\n4. Third?\n"), Message::Free);
        assert_eq!(read_message("1. First?\n1. Second?\n1. Third?\n"), Message::Free);
        assert_eq!(read_message("1. First?\n2. Second?\n2. Third?\n"), Message::Free);
        assert_eq!(read_message("1. First?\n3. Second?\n"), Message::Free);
    }

    #[test]
    fn the_number_is_the_one_claude_typed() {
        let batch = batch(read_message("1. First?\n2. Second?\n\nThen.\n\n3. Third?\n4. Fourth?\n"));
        assert_eq!(numbers(&batch), [1, 2, 3, 4]);
    }

    #[test]
    fn under_indented_sub_items_that_become_siblings_are_free() {
        let message = "1. One?\n2. Which?\n  1. sub\n  2. sub\n3. Next?\n";
        assert_eq!(read_message(message), Message::Free);
        let tens = "9. Nine?\n10. Which?\n  1. sub\n  2. sub\n11. Next?\n";
        assert_eq!(read_message(tens), Message::Free);
    }

    #[test]
    fn a_numbered_line_that_hides_in_a_paragraph_makes_the_message_free() {
        let proposal = "**CLAUDE.md**\n1. A\n2. B\n\n**cv/**\n3. C\n4. D\n\nReply yes to apply all, or list the numbers to skip.\n";
        assert_eq!(read_message(proposal), Message::Free);
        let questions = "**About pay**\n1. A?\n2. B?\n\n**About the rest**\n10. C?\n11. D?\n";
        assert_eq!(read_message(questions), Message::Free);
        assert_eq!(read_message("1. A?\n2. B?\n\nSee note\n3) not a list.\n"), Message::Free);
    }

    #[test]
    fn numbered_lines_in_code_or_as_nested_sub_items_do_not_count_as_hidden() {
        let fenced = "1. A?\n2. B?\n\n```\n3. in a fence\n```\n";
        assert_eq!(numbers(&batch(read_message(fenced))), [1, 2]);
        let indented = "1. A?\n2. B?\n\nExample:\n\n    3. in an indented block\n";
        assert_eq!(numbers(&batch(read_message(indented))), [1, 2]);
        let nested = "1. A?\n   1. sub one\n   2. sub two\n2. B?\n";
        let batch = batch(read_message(nested));
        assert_eq!(numbers(&batch), [1, 2]);
        assert_eq!(batch.items().next().unwrap().text, "A? sub one sub two");
    }

    #[test]
    fn every_kind_of_line_break_splits_a_line() {
        for sep in ["\r", "\r\n", "\u{85}", "\u{2028}", "\u{2029}", "\n"] {
            let message = format!("1. A?{sep}2. B?\n3. C?\n4. D?");
            let batch = batch(read_message(&message));
            assert_eq!(numbers(&batch), [1, 2, 3, 4], "separator {sep:?}");
            assert_eq!(batch.items().next().unwrap().text, "A?", "separator {sep:?}");
        }
    }

    #[test]
    fn a_plain_paragraph_after_a_title_clears_it() {
        let batch = batch(read_message("**Note**\n\nSome plain text.\n\n1. A?\n2. B?\n"));
        assert_eq!(batch.groups[0].title, None);
    }

    #[test]
    fn lettered_lines_after_a_blank_line_stay_inside_their_item() {
        let batch = batch(read_message("1. First?\n2. Which?\n\na. IC\nb. Lead\n\n3. Third?\n"));
        assert_eq!(numbers(&batch), [1, 2, 3]);
        let two = batch.items().nth(1).unwrap();
        assert_eq!(two.text, "Which? a. IC b. Lead");
        assert_eq!(two.body, "Which?\n\na. IC\nb. Lead");
        assert_eq!(batch.items().nth(2).unwrap().text, "Third?");
    }

    #[test]
    fn an_ordinary_paragraph_between_items_is_not_pulled_into_one() {
        let batch = batch(read_message("1. First?\n2. Second?\n\nAnd now the next part.\n\n3. Third?\n"));
        assert_eq!(batch.items().nth(1).unwrap().text, "Second?");
    }

    #[test]
    fn numbers_that_do_not_start_at_one_or_restart_are_free() {
        assert_eq!(read_message("3. First\n4. Second\n"), Message::Free);
        assert_eq!(read_message("1. First\n2. Second\n\nThen.\n\n1. Again\n2. Twice\n"), Message::Free);
        assert_eq!(read_message("2. Second\n1. First\n"), Message::Free);
    }

    #[test]
    fn fewer_than_two_items_is_free() {
        assert_eq!(read_message("One question.\n\n1. Which city?\n"), Message::Free);
    }

    #[test]
    fn numbers_run_on_across_lists_with_titles_between() {
        let batch = batch(read_message("**First**\n\n1. A?\n2. B?\n\n**Second**\n\n3. C?\n"));
        assert_eq!(numbers(&batch), [1, 2, 3]);
        assert_eq!(batch.groups.len(), 2);
    }

    #[test]
    fn a_heading_between_lists_is_a_title() {
        let batch = batch(read_message("1. A?\n2. B?\n\n## Next\n\n3. C?\n"));
        let titles: Vec<_> = batch.groups.iter().map(|group| group.title.as_deref()).collect();
        assert_eq!(titles, [None, Some("Next")]);
    }

    #[test]
    fn bold_inside_a_sentence_is_not_a_title() {
        let batch = batch(read_message("1. A?\n2. B?\n\nThis is **important** to know.\n\n3. C?\n"));
        assert_eq!(batch.groups.len(), 1);
        assert_eq!(numbers(&batch), [1, 2, 3]);
    }

    #[test]
    fn a_question_mark_in_an_item_makes_it_questions() {
        let message = read_message("1. Rename the file?\n2. Add a skill.\n\nReply yes to apply all.\n");
        assert!(matches!(message, Message::Questions(_)));
    }

    #[test]
    fn a_proposal_needs_a_closing_paragraph_that_asks() {
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nReply yes to apply all.\n"), Message::Proposal(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nShall I APPLY these?\n"), Message::Proposal(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nApply them, or list the numbers to skip.\n"), Message::Proposal(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n"), Message::Questions(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nTell me when you are ready.\n"), Message::Questions(_)));
    }

    #[test]
    fn a_confirming_question_batch_is_not_a_proposal() {
        let message = "1. Your title is Design Lead.\n2. Your notice period is three months.\n\nCan you confirm these are right?\n";
        assert!(matches!(read_message(message), Message::Questions(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nReply yes if you agree.\n"), Message::Questions(_)));
        assert!(matches!(read_message(SIXTEEN), Message::Proposal(_)));
    }

    #[test]
    fn the_asking_word_is_a_whole_word() {
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nApplying these takes a minute. Yesterday's list.\n"), Message::Questions(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nThese are reapplied, unconfirmed.\n"), Message::Questions(_)));
    }

    #[test]
    fn only_the_paragraph_after_the_last_list_asks() {
        let before = "Reply yes if this looks right.\n\n1. Add a skill.\n2. Rename a file.\n";
        assert!(matches!(read_message(before), Message::Questions(_)));
        let after = "Reply yes if this looks right.\n\n1. Add a skill.\n2. Rename a file.\n\nTake your time.\n";
        assert!(matches!(read_message(after), Message::Questions(_)));
    }

    #[test]
    fn hidden_characters_inside_a_number_still_parse() {
        let message = read_message("Two.\n\n1\u{200B}. First\u{202E} one?\n\u{2060}2\u{FEFF}. Second one?\n");
        let batch = batch(message);
        assert_eq!(numbers(&batch), [1, 2]);
        assert_eq!(batch.items().next().unwrap().text, "First one?");
    }

    #[test]
    fn hidden_characters_cannot_hide_the_closing_word() {
        let message = read_message("1. Add a skill.\n2. Rename a file.\n\nReply y\u{200B}es to apply all.\n");
        assert!(matches!(message, Message::Proposal(_)));
    }

    #[test]
    fn a_code_block_holding_a_number_is_not_an_item() {
        assert_eq!(read_message("Run this:\n\n```\n1. first\n2. second\n```\n"), Message::Free);
        let message = read_message("1. Real one?\n2. Another?\n\n```\n3. not an item\n```\n");
        assert_eq!(numbers(&batch(message)), [1, 2]);
    }

    #[test]
    fn a_list_inside_a_quote_is_not_top_level() {
        assert_eq!(read_message("> 1. first\n> 2. second\n"), Message::Free);
    }

    #[test]
    fn a_bullet_list_is_not_numbered_items() {
        assert_eq!(read_message("- first\n- second\n"), Message::Free);
    }

    #[test]
    fn a_number_that_does_not_fit_is_free() {
        assert_eq!(read_message("99999999999. First\n2. Second\n"), Message::Free);
    }

    #[test]
    fn the_body_drops_the_number_and_shifts_later_lines_left() {
        assert_eq!(body("12. Which?\n    a. one\n    b. two\n"), "Which?\na. one\nb. two");
        assert_eq!(body("1) Which?"), "Which?");
    }

    #[test]
    fn an_optional_mark_is_read_in_any_case_at_the_end_of_the_first_paragraph() {
        let batch = batch(read_message("1. Salary (Optional)\n2. Notice period\n3. Which (optional) one?\n"));
        assert_eq!(batch.items().map(|item| item.optional).collect::<Vec<_>>(), [true, false, false]);
    }
}
