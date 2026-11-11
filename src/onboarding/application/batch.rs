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

/// The numbered items of a message, in Claude's order, in the groups Claude gave them, and the whole message as its
/// blocks in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub groups: Vec<Group>,
    /// Every block of the message, in order, read from the same text as the items: so a screen shows all of it, with
    /// each item where Claude put it, and never looks for an item itself.
    pub blocks: Vec<MessageBlock>,
}

/// One block of a message, as the parser read it, without hidden characters and with every line break a newline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageBlock {
    /// Claude's words between the items, as Markdown source: a paragraph, a list of bullets, a code block, a quote.
    Prose(String),
    /// A heading, or a bold line alone, that titles the items under it, as plain words.
    Title(String),
    /// The numbered item of this number.
    Item(u32),
    /// A proposal's closing paragraph, which asks to confirm, as Markdown source.
    Ask(String),
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
    let count = scan.groups.iter().map(|group| group.items.len()).sum::<usize>();
    if count < 2 || !numbered {
        return Message::Free;
    }
    // "Apply all except 2." would be ambiguous to Claude when a sub-list repeats the numbers, so that is never a proposal.
    let asks = !scan.nested && scan.closing.as_deref().is_some_and(asks_to_apply);
    // A change is never optional, so an item marked "(optional)" asks something.
    let proposal = asks && scan.groups.iter().flat_map(|group| group.items.iter()).all(|item| !item.text.contains('?') && !item.optional);
    let ask = if proposal { scan.closing_range.clone() } else { None };
    let batch = Batch { blocks: blocks(&text, &scan.spans, ask), groups: scan.groups };
    if proposal { Message::Proposal(batch) } else { Message::Questions(batch) }
}

/// What a stretch of the message is, past its prose.
#[derive(Debug, Clone)]
enum Span {
    Item(u32),
    Title(String),
}

/// The message's blocks in order: each item and title where its span lies, the proposal's ask apart, and the text
/// between them as prose, one block for each stretch.
fn blocks(text: &str, spans: &[(Range<usize>, Span)], ask: Option<Range<usize>>) -> Vec<MessageBlock> {
    // The ask wins over anything else read at its paragraph, such as a title when it is all bold.
    let asked = |range: &Range<usize>| ask.as_ref().is_some_and(|ask| range.start < ask.end && ask.start < range.end);
    let mut marked: Vec<(Range<usize>, Option<&Span>)> =
        spans.iter().filter(|(range, _)| !asked(range)).map(|(range, span)| (range.clone(), Some(span))).collect();
    marked.extend(ask.clone().map(|range| (range, None)));
    marked.sort_by_key(|(range, _)| range.start);
    let mut found = Vec::new();
    let mut at = 0;
    let prose = |found: &mut Vec<MessageBlock>, range: Range<usize>| {
        let source = text[range].trim();
        if !source.is_empty() {
            found.push(MessageBlock::Prose(source.to_owned()));
        }
    };
    for (range, span) in marked {
        if range.start < at {
            continue;
        }
        prose(&mut found, at..range.start);
        found.push(match span {
            Some(Span::Item(number)) => MessageBlock::Item(*number),
            Some(Span::Title(words)) => MessageBlock::Title(words.clone()),
            None => MessageBlock::Ask(text[range.clone()].trim().to_owned()),
        });
        at = range.end;
    }
    prose(&mut found, at..text.len());
    found
}

/// Whether the closing paragraph holds the whole words "apply" and "skip", in any case. That is the form of a proposal,
/// "Reply yes to apply all, or list the numbers to skip.". "Confirm" and "yes" alone do not make one, and neither does
/// a mention of applying, even beside "number", as in "Reply with the number and your answer for each, and I'll apply
/// them.", because "Yes, apply these changes." answers only a proposal.
fn asks_to_apply(closing: &str) -> bool {
    let has = |ask: &str| closing.split(|c: char| !c.is_alphanumeric()).any(|word| word.eq_ignore_ascii_case(ask));
    has("apply") && has("skip")
}

struct Scan {
    groups: Vec<Group>,
    /// The last paragraph outside any list since the last numbered list.
    closing: Option<String>,
    /// Whether a top-level item holds a numbered list of its own.
    nested: bool,
    /// Where the closing paragraph is in the text.
    closing_range: Option<Range<usize>>,
    /// Where each top-level item and each title is in the text, in order.
    spans: Vec<(Range<usize>, Span)>,
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
    // Whether a top-level item holds a numbered list of its own, whose numbers would repeat the message's.
    let mut nested = false;
    // Whether a heading or a paragraph came since the last numbered list, so the next list is a group of its own.
    let mut separated = false;
    let mut code: Vec<Range<usize>> = Vec::new();
    let mut item_starts: HashSet<usize> = HashSet::new();
    let mut closing_range = None;
    let mut spans: Vec<(Range<usize>, Span)> = Vec::new();
    for (event, range) in Parser::new_ext(text, markdown::options()).into_offset_iter() {
        match event {
            Event::Start(Tag::List(start)) => {
                if depth == 0 {
                    next = start;
                    after_item = false;
                    // A bullet list between numbered ones sets them apart.
                    separated |= start.is_none();
                    if start.is_some() {
                        (closing, closing_range) = (None, None);
                        // A heading, a bold line or a plain paragraph before the list starts a group, titled or not.
                        if groups.is_empty() || separated || title.is_some() {
                            groups.push(Group { title: title.take(), items: Vec::new() });
                        }
                        separated = false;
                    }
                } else if start.is_some() {
                    // Only the numbered list of a top-level item that starts at 1, its sub-steps, may sit deeper. Any other
                    // numbered item would be approved with no box of its own.
                    if start != Some(1) || open.is_none() {
                        return None;
                    }
                    nested = true;
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
                    let range = item.start..item.end;
                    let done = finish(item, text)?;
                    spans.push((range, Span::Item(done.number)));
                    group.items.push(done);
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                code.push(range);
                if depth == 0 {
                    after_item = false;
                    separated = true;
                }
            }
            Event::Rule | Event::Start(Tag::HtmlBlock) if depth == 0 => {
                after_item = false;
                separated = true;
            }
            Event::Start(tag) if holds_blocks(&tag) => {
                if depth == 0 {
                    after_item = false;
                    separated = true;
                }
                depth += 1;
            }
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
                            if let Some((range, _)) = spans.iter_mut().rev().find(|(_, span)| matches!(span, Span::Item(_))) {
                                range.end = range.end.max(done.range.end);
                            }
                        }
                        _ => {
                            after_item = false;
                            separated = true;
                            // A plain paragraph between a title and its list, such as an intro line, keeps the title.
                            if (done.heading || (done.bold_text && !done.plain_text)) && !line.is_empty() {
                                spans.push((done.range.clone(), Span::Title(line.clone())));
                                title = Some(line.clone());
                            }
                            if !done.heading {
                                closing = Some(line);
                                closing_range = Some(done.range.clone());
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
    Some(Scan { groups, closing, nested, closing_range, spans })
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
        // Quote markers come before the number of a quoted line.
        let skipped = line.len() - line.trim_start_matches([' ', '\t', '>']).len();
        let marker = at + skipped;
        at += line.len() + 1;
        let rest = &line[skipped..];
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
    fn an_ordered_item_anywhere_but_a_top_level_list_makes_the_message_free() {
        let ask = "\n\nReply yes to apply all, or list the numbers to skip.";
        let nested = format!("1. Set the name.\n2. **cv/**\n\n   3. Build the CV.\n   4. File the letter.{ask}");
        assert_eq!(read_message(&nested), Message::Free);
        let under_bullet = format!("1. A\n2. B\n\n- **cv/**\n\n  3. Build the CV.\n  4. File the letter.{ask}");
        assert_eq!(read_message(&under_bullet), Message::Free);
        let quoted = format!("1. A\n2. B\n\n> 3. C\n> 4. D{ask}");
        assert_eq!(read_message(&quoted), Message::Free);
        let quoted_in_paragraph = format!("1. A\n2. B\n\n> note\n> 3. C{ask}");
        assert_eq!(read_message(&quoted_in_paragraph), Message::Free);
        let nested_not_at_one = "1. A?\n   2. sub\n   3. sub\n2. B?\n";
        assert_eq!(read_message(nested_not_at_one), Message::Free);
    }

    #[test]
    fn nested_sub_steps_never_make_a_proposal() {
        let message = "1. **CLAUDE.md**\n\n   1. Set the name.\n   2. Add the title.\n\n2. **cv/**\n\n   1. Build the CV.\n\nReply yes to apply all, or list the numbers to skip.";
        let Message::Questions(batch) = read_message(message) else { panic!("expected questions") };
        assert_eq!(numbers(&batch), [1, 2]);
        // Without the sub-steps the same closing paragraph still makes a proposal.
        let flat = "1. **CLAUDE.md**\n2. **cv/**\n\nReply yes to apply all, or list the numbers to skip.";
        assert!(matches!(read_message(flat), Message::Proposal(_)));
    }

    #[test]
    fn a_block_between_two_lists_starts_a_new_group() {
        for between in ["- a note", "> a quote", "```\ncode\n```", "---", "| a | b |\n|---|---|\n| 1 | 2 |", "<div>x</div>"] {
            let batch = batch_of(&format!("## One\n\n1. A?\n2. B?\n\n{between}\n\n3. C?\n"));
            let groups: Vec<(Option<&str>, Vec<u32>)> =
                batch.groups.iter().map(|group| (group.title.as_deref(), group.items.iter().map(|item| item.number).collect())).collect();
            assert_eq!(groups, [(Some("One"), vec![1, 2]), (None, vec![3])], "between: {between:?}");
        }
    }

    #[test]
    fn a_sub_list_that_starts_at_one_inside_an_item_is_still_parsed() {
        let message = "1. Steps?\n\n   1. open it\n   2. close it\n\n2. B?\n";
        let batch = batch_of(message);
        assert_eq!(numbers(&batch), [1, 2]);
        assert_eq!(batch.items().next().unwrap().text, "Steps? open it close it");
    }

    #[test]
    fn a_plain_paragraph_between_a_title_and_its_list_keeps_the_title() {
        let batch = batch(read_message("**Note**\n\nSome plain text.\n\n1. A?\n2. B?\n"));
        assert_eq!(batch.groups[0].title.as_deref(), Some("Note"));
    }

    #[test]
    fn a_heading_and_its_intro_title_the_list_for_a_proposal_and_for_questions() {
        let proposal =
            "## CLAUDE.md\n\n1. A.\n2. B.\n\n## cv/\n\nThese two touch your CV.\n\n3. C.\n4. D.\n\nReply yes to apply all, or list the numbers to skip.\n";
        let Message::Proposal(batch) = read_message(proposal) else { panic!("expected a proposal") };
        let groups: Vec<(Option<&str>, Vec<u32>)> =
            batch.groups.iter().map(|group| (group.title.as_deref(), group.items.iter().map(|item| item.number).collect())).collect();
        assert_eq!(groups, [(Some("CLAUDE.md"), vec![1, 2]), (Some("cv/"), vec![3, 4])]);
        let questions = "**About pay**\n\n1. A?\n2. B?\n\n**About the CV**\n\nThese two are about your CV.\n\n3. C?\n4. D?\n";
        let Message::Questions(batch) = read_message(questions) else { panic!("expected questions") };
        let groups: Vec<(Option<&str>, Vec<u32>)> =
            batch.groups.iter().map(|group| (group.title.as_deref(), group.items.iter().map(|item| item.number).collect())).collect();
        assert_eq!(groups, [(Some("About pay"), vec![1, 2]), (Some("About the CV"), vec![3, 4])]);
    }

    #[test]
    fn a_list_never_joins_the_group_before_a_separator() {
        let batch = batch(read_message("## One\n\n1. A?\n2. B?\n\nAnd now the rest.\n\n3. C?\n"));
        let titles: Vec<_> = batch.groups.iter().map(|group| group.title.as_deref()).collect();
        assert_eq!(titles, [Some("One"), None]);
        assert_eq!(batch.groups[1].items.iter().map(|item| item.number).collect::<Vec<_>>(), [3]);
        let batch = batch_of("1. A?\n2. B?\n\n## Two\n\n3. C?\n");
        assert_eq!(batch.groups.len(), 2);
    }

    fn batch_of(message: &str) -> Batch {
        batch(read_message(message))
    }

    /// The blocks of `message`, with each item by its number, a title by its words, prose by its source.
    fn blocks_of(message: &str) -> Vec<String> {
        batch_of(message)
            .blocks
            .iter()
            .map(|block| match block {
                MessageBlock::Prose(source) => format!("prose: {source}"),
                MessageBlock::Title(words) => format!("title: {words}"),
                MessageBlock::Item(number) => format!("item {number}"),
                MessageBlock::Ask(source) => format!("ask: {source}"),
            })
            .collect()
    }

    #[test]
    fn a_message_is_its_blocks_in_order_with_each_item_in_place() {
        let message = "## Your profile\n\nA few things first.\n\n1. Which city?\n2. Which salary?\n\nNow the documents.\n\n**Letters**\n\n3. Which language?\n\nTake your time.";
        assert_eq!(
            blocks_of(message),
            [
                "title: Your profile",
                "prose: A few things first.",
                "item 1",
                "item 2",
                "prose: Now the documents.",
                "title: Letters",
                "item 3",
                "prose: Take your time."
            ]
        );
        let lettered = blocks_of("Intro.\n\n1. First?\n2. Which?\n\na. IC\nb. Lead\n\n3. Third?\n\nThanks.");
        assert_eq!(lettered, ["prose: Intro.", "item 1", "item 2", "item 3", "prose: Thanks."], "lettered lines stay in their item");
    }

    /// The blocks come from the text the parser read: hidden characters and odd line breaks change nothing.
    #[test]
    fn hidden_characters_and_odd_line_breaks_keep_every_item_once_in_place() {
        let plain = blocks_of("Intro.\n\n1. Which city?\n2. Which salary?\n\nThanks.");
        assert_eq!(blocks_of("Intro.\n\n\u{200b}1. Which city?\n2. Which salary?\n\nThanks."), plain);
        assert_eq!(blocks_of("Intro.\u{2029}\u{2029}1. Which city?\u{2028}2. Which salary?\u{2028}\u{2028}Thanks."), plain);
        assert_eq!(blocks_of("Intro.\r\n\r\n1. Which city?\r2. Which salary?\r\n\r\nThanks."), plain);
    }

    /// A fenced code block in the lead that starts with "1." is prose, and the items keep their own place.
    #[test]
    fn a_numbered_line_inside_code_stays_in_its_code_block() {
        let blocks = blocks_of("Here is the format:\n\n```\n1. Example\n```\n\n1. Which city?\n2. Which salary?");
        assert_eq!(blocks, ["prose: Here is the format:\n\n```\n1. Example\n```", "item 1", "item 2"]);
    }

    /// A closing ask in bold is still the ask, never a group's title.
    #[test]
    fn a_bold_closing_ask_is_the_ask() {
        let message = "Here is what I will write.\n\n1. Add the title.\n2. Add the city.\n\n**Reply yes to apply all, or list the numbers to skip.**";
        assert!(matches!(read_message(message), Message::Proposal(_)));
        assert_eq!(blocks_of(message).last().map(String::as_str), Some("ask: **Reply yes to apply all, or list the numbers to skip.**"));
        assert!(!blocks_of(message).iter().any(|block| block.starts_with("title")));
    }

    /// The paragraph that makes a message a proposal is its ask, so the review card can leave it out.
    #[test]
    fn a_proposals_closing_ask_is_a_block_of_its_own() {
        let blocks = blocks_of(SIXTEEN);
        assert_eq!(blocks.last().map(String::as_str), Some("ask: Reply yes to apply all, or list the numbers to skip."));
        assert_eq!(blocks.iter().filter(|block| block.starts_with("item")).count(), 16);
        let questions = blocks_of(&format!("{FIVE}\nTake your time."));
        assert_eq!(questions.last().map(String::as_str), Some("prose: Take your time."), "a question batch has no ask");
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
        let titles: Vec<_> = batch.groups.iter().map(|group| group.title.as_deref()).collect();
        assert_eq!(titles, [None, None]);
        assert_eq!(numbers(&batch), [1, 2, 3]);
    }

    #[test]
    fn a_question_mark_in_an_item_makes_it_questions() {
        let message = read_message("1. Rename the file?\n2. Add a skill.\n\nReply yes to apply all.\n");
        assert!(matches!(message, Message::Questions(_)));
    }

    #[test]
    fn a_proposal_needs_a_closing_paragraph_that_asks() {
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nReply yes to apply all, or list the numbers to skip.\n"), Message::Proposal(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nApply them, or give a number to SKIP.\n"), Message::Proposal(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nApply them, or list the numbers to skip.\n"), Message::Proposal(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nReply yes to apply all.\n"), Message::Questions(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nShall I APPLY these?\n"), Message::Questions(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n"), Message::Questions(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nTell me when you are ready.\n"), Message::Questions(_)));
    }

    /// Follow-ups written as noun phrases hold no "?", and their closing line may say "number" and "apply". Only "apply"
    /// and "skip" together make a proposal, and an item marked "(optional)" is a question.
    #[test]
    fn noun_phrase_follow_ups_are_questions_and_upstreams_closing_is_a_proposal() {
        let follow_ups = "A few more things:\n\n1. Career goals and target role types\n2. Deal-breakers and must-haves\n3. Salary expectations (optional)\n\nReply with the number and your answer for each, and I'll apply them to your profile.";
        assert!(matches!(read_message(follow_ups), Message::Questions(_)));
        let without_optional = "1. Career goals and target role types\n2. Deal-breakers and must-haves\n\nReply with the number and your answer for each, and I'll apply them to your profile.";
        assert!(matches!(read_message(without_optional), Message::Questions(_)), "\"number\" is not \"skip\"");
        let optional = "1. Add the Harbor certification.\n2. Add a salary floor (optional)\n\nReply yes to apply all, or list the numbers to skip.";
        assert!(matches!(read_message(optional), Message::Questions(_)), "an optional item asks");
        let upstream = "1. Add the Harbor certification.\n2. Add the Lowmere office.\n\nReply **yes** to apply all, or list the numbers you want to skip.";
        assert!(matches!(read_message(upstream), Message::Proposal(_)));
        assert!(matches!(read_message(SIXTEEN), Message::Proposal(_)));
    }

    #[test]
    fn a_confirming_question_batch_is_not_a_proposal() {
        let message = "1. Your title is Design Lead.\n2. Your notice period is three months.\n\nCan you confirm these are right?\n";
        assert!(matches!(read_message(message), Message::Questions(_)));
        assert!(matches!(read_message("1. Add a skill.\n2. Rename a file.\n\nReply yes if you agree.\n"), Message::Questions(_)));
        assert!(matches!(read_message(SIXTEEN), Message::Proposal(_)));
    }

    #[test]
    fn instruction_style_questions_that_mention_applying_are_not_a_proposal() {
        let message =
            "1. Name three companies you admire.\n2. List the cities you would work from.\n\nOnce you answer, I will apply your answers to your profile.\n";
        assert!(matches!(read_message(message), Message::Questions(_)));
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
        let message = read_message("1. Add a skill.\n2. Rename a file.\n\nReply yes to app\u{200B}ly all, or list the numbers to skip.\n");
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
