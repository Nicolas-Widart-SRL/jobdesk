use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::shared_kernel::markdown;
use crate::shared_kernel::markdown::only_comments;

/// The setup tokens a page of profile Markdown shows, as `placeholder_spans` finds them.
pub fn placeholder_count(text: &str) -> usize {
    placeholder_spans(text).len()
}

/// `/reset` restores tokens such as `[FIRST_NAME]`, and `/setup` replaces them. Each token the page shows, as the byte
/// range of `text` it covers, brackets included, from one parse with the viewer's options. The page shows tokens as
/// written, so a token counts where the page draws it:
///
/// - wholly inside one run of text the page draws: prose, a link's text or an image's alt text;
/// - inside a code span or a code block, where a one-word name (`[DEFAULT]`) does not count but a name with `_` or
///   spaces does, because the templates keep blanks like `[YOUR_CITY]` in their example queries and LaTeX.
///
/// A token never counts in a reference definition, a link's or image's destination, a reference label or HTML,
/// comments included, because none of them is drawn as text. An escaped bracket (`\[NAME]`) or an entity
/// (`&#91;NAME&#93;`) opens no token. The cost is linear in the length of `text`.
///
/// A token is an upper-case name such as `[FIRST_NAME]`, `[KEY_RESPONSIBILITY_1]`, `[YOUR RESPONSIBILITY]` or `[1-2 SENTENCE_SUMMARY]`.
/// A name of one word has three letters or more and no digit, so `[B2]`, `[EU]` and `[AB12]` are the user's own words.
/// The name may be followed by a hint up to the closing bracket: `[CONTEXT - what was happening]` or `[LEVEL, e.g. "Native"]`.
/// A bracket right after a letter or a digit (`a[0]`, `\usepackage[T1]`) and a lower-case bracket are not tokens.
pub fn placeholder_spans(text: &str) -> Vec<Range<usize>> {
    if !text.contains('[') {
        return Vec::new();
    }
    let (mut runs, mut code): (Vec<Range<usize>>, Vec<Range<usize>>) = (Vec::new(), Vec::new());
    let mut in_block = false;
    for (event, range) in Parser::new_ext(text, markdown::options()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_block = true,
            Event::End(TagEnd::CodeBlock) => in_block = false,
            Event::Text(_) if in_block => code.push(range),
            Event::Code(_) => code.push(range),
            // Brackets that form no link reach the page as text events of their own, so neighbours merge into one run.
            Event::Text(_) => match runs.last_mut() {
                Some(last) if range.start == last.end => last.end = range.end,
                _ => runs.push(range),
            },
            _ => {}
        }
    }
    let mut spans: Vec<Range<usize>> = Vec::new();
    for run in runs {
        spans.extend(tokens_in(text, run).filter(|span| !escaped_at(text, span.start)));
    }
    for range in code {
        let plain = |span: &Range<usize>| !token_name(&text[span.start + 1..]).contains(['_', ' ']);
        spans.extend(tokens_in(text, range).filter(|span| !plain(span)));
    }
    spans.sort_by_key(|span| span.start);
    spans
}

/// What a page shows of a part of a heading, such as a role's title, from the same parse that counts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// Text as the page shows it, escapes and entities read.
    Text(String),
    /// A counted token in prose, with its words (`blank_words`).
    Blank(String),
}

/// The pieces `text` shows in `part`, a byte range inside one heading line, such as a role's title. A blank stands
/// exactly where `placeholder_spans` counted a prose token, by source position, and nowhere else: no text of any kind
/// can make one. A token in a code span is counted, as in any code, but shows as written, with the code's text.
/// The cost is linear in the length of `text`.
pub fn shown_pieces(text: &str, part: Range<usize>) -> Vec<Piece> {
    let spans = placeholder_spans(text);
    // The spans that reach past `start`, in order: they are sorted and never overlap.
    let from = |start: usize| spans[spans.partition_point(|span| span.end <= start)..].iter();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut push = |piece: Piece| match (pieces.last_mut(), piece) {
        (Some(Piece::Text(last)), Piece::Text(more)) => last.push_str(&more),
        (_, piece) => pieces.push(piece),
    };
    for (event, range) in Parser::new_ext(text, markdown::options()).into_offset_iter() {
        if range.end <= part.start || range.start >= part.end {
            continue;
        }
        match event {
            // A text event that reads as its source maps by position, so it can be cut around the tokens in it.
            Event::Text(shown) if *shown == text[range.clone()] => {
                let (start, end) = (range.start.max(part.start), range.end.min(part.end));
                let mut at = start;
                for span in from(start).take_while(|span| span.start < end) {
                    if span.start > at {
                        push(Piece::Text(text[at..span.start].to_owned()));
                    }
                    if span.start >= at {
                        push(Piece::Blank(blank_words(&text[span.clone()])));
                    }
                    at = span.end.min(end);
                }
                if at < end {
                    push(Piece::Text(text[at..end].to_owned()));
                }
            }
            // An escape or an entity inside a token belongs to its blank.
            Event::Text(shown) if range.start >= part.start && from(range.start).next().is_none_or(|span| span.start >= range.end) => push(Piece::Text(shown.to_string())),
            Event::Code(shown) | Event::InlineHtml(shown) | Event::Html(shown) if range.start >= part.start => push(Piece::Text(shown.to_string())),
            Event::SoftBreak | Event::HardBreak if range.start >= part.start => push(Piece::Text(" ".to_owned())),
            _ => {}
        }
    }
    pieces
}

/// Where the first heading's own content sits in `text`, from the parse: without its `#` markers, a closing sequence
/// of `#`, or comments after the last of its words.
pub fn heading_content(text: &str) -> Option<Range<usize>> {
    let mut parts: Vec<(Range<usize>, bool)> = Vec::new();
    let mut inside = false;
    for (event, range) in Parser::new_ext(text, markdown::options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { .. }) if !inside => inside = true,
            Event::End(TagEnd::Heading(_)) => break,
            Event::InlineHtml(html) if inside => parts.push((range, only_comments(&html))),
            _ if inside => parts.push((range, false)),
            _ => {}
        }
    }
    while parts.last().is_some_and(|(range, comment)| *comment || text[range.clone()].trim().is_empty()) {
        parts.pop();
    }
    let (start, end) = (parts.first()?.0.start, parts.last()?.0.end);
    let content = &text[start..end];
    let start = start + (content.len() - content.trim_start().len());
    Some(start..start + content.trim().len())
}

/// Whether a block shows anything under its headings: any text, code or HTML. Blank lines and HTML that holds only
/// comments, which the Profile paper leaves out, show nothing.
pub fn shows_content(text: &str) -> bool {
    let (mut in_heading, mut html) = (false, String::new());
    for event in Parser::new_ext(text, markdown::options()) {
        match event {
            Event::Start(Tag::Heading { .. }) => in_heading = true,
            Event::End(TagEnd::Heading(_)) => in_heading = false,
            Event::Text(_) | Event::Code(_) if !in_heading => return true,
            Event::Html(part) => html.push_str(&part),
            Event::End(TagEnd::HtmlBlock) if !only_comments(&std::mem::take(&mut html)) => return true,
            Event::InlineHtml(part) if !in_heading && !only_comments(&part) => return true,
            _ => {}
        }
    }
    false
}

/// The tokens in `range` of `text` by the name rule alone, each wholly inside it.
fn tokens_in(text: &str, range: Range<usize>) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut taken = range.start;
    text[range.clone()].match_indices('[').filter_map(move |(at, _)| {
        let start = range.start + at;
        if start < taken {
            return None;
        }
        let end = token_end(text, start).filter(|&end| end <= range.end)?;
        taken = end;
        Some(start..end)
    })
}

/// Whether the character at `at` is escaped by the backslashes before it.
fn escaped_at(text: &str, at: usize) -> bool {
    text[..at].bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

/// Where the token that opens at `start` ends, just after its closing bracket. A bracket right after a letter or a digit opens none.
fn token_end(text: &str, start: usize) -> Option<usize> {
    if text[..start].chars().next_back().is_some_and(char::is_alphanumeric) {
        return None;
    }
    let after = start + 1;
    let name = name_len(&text[after..]);
    if name == 0 {
        return None;
    }
    let rest = &text[after + name..];
    let close = if rest.starts_with(']') {
        after + name
    } else if rest.starts_with(" - ") || rest.starts_with(", e.g.") {
        matching_close(text, after + name)?
    } else {
        return None;
    };
    Some(close + 1)
}

/// The name of a token, such as `FIRST_NAME` for `FIRST_NAME` or `LEVEL` for `LEVEL, e.g. "Native"`. The brackets are not part of `token`.
pub fn token_name(token: &str) -> &str {
    &token[..name_len(token)]
}

/// The length of a token name at the start of `text`, or 0 when there is none.
/// A name is upper-case words of A-Z, digits, `_` and `'`, joined by single spaces, and may start with a range such as `1-2 `.
/// A word without `_` has three letters or more and no digit, so `[B2]`, `[EU UK]` and `[AB12]` are the user's own words.
fn name_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let digits = |from: usize| bytes[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut start = digits(0);
    if start > 0 {
        let range = if bytes.get(start) == Some(&b'-') { digits(start + 1) } else { 0 };
        if range == 0 || bytes.get(start + 1 + range) != Some(&b' ') {
            return 0;
        }
        start += range + 2;
    }
    if !bytes.get(start).is_some_and(u8::is_ascii_uppercase) {
        return 0;
    }
    let mut len = start;
    while len < bytes.len() {
        let c = bytes[len];
        let word = c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_' || c == b'\'';
        let joined = c == b' ' && bytes.get(len + 1).is_some_and(u8::is_ascii_uppercase);
        if !(word || joined) {
            break;
        }
        len += 1;
    }
    let name = &text[start..len];
    let plain = |word: &str| word.len() >= 3 && word.bytes().all(|c| c.is_ascii_uppercase() || c == b'\'');
    if name.len() < 3 || !name.split(' ').all(|word| word.contains('_') || plain(word)) {
        return 0;
    }
    len
}

/// What a placeholder token reads as on screen, such as `to fill: first name` for `FIRST_NAME`.
/// The name reads in lower case. A hint after it keeps its case: `LEVEL, e.g. "Native"` gives `to fill: level, e.g. "Native"`.
pub fn blank_words(token: &str) -> String {
    let token = token.strip_prefix('[').unwrap_or(token);
    let token = token.strip_suffix(']').unwrap_or(token);
    if token == "PLACEHOLDER" {
        return "to fill".to_owned();
    }
    let name = token_name(token);
    format!("to fill: {}{}", name.replace('_', " ").to_lowercase(), &token[name.len()..])
}

/// The longest hint a token may hold, so a line of unclosed hints costs a bounded scan per bracket and counting stays
/// linear in the length of the text.
const MAX_HINT: usize = 240;

/// The `]` that closes the token whose hint starts at `from`, skipping brackets nested in the hint. A line break, or a
/// hint longer than `MAX_HINT`, ends the search.
fn matching_close(text: &str, from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, c) in text[from..].char_indices().take_while(|(at, _)| *at <= MAX_HINT) {
        match c {
            '[' => depth += 1,
            ']' if depth == 0 => return Some(from + at),
            ']' => depth -= 1,
            '\n' | '\r' => return None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Piece, blank_words, heading_content, placeholder_count, placeholder_spans, shown_pieces, shows_content, token_name};

    /// The tokens the page shows, as written.
    fn found(text: &str) -> Vec<&str> {
        placeholder_spans(text).into_iter().map(|span| &text[span]).collect()
    }

    #[test]
    fn counts_only_setup_tokens() {
        assert_eq!(placeholder_count(r"\name{[FIRST_NAME]}{[LAST_NAME]} [x] [AB] [Note] - [ ] a[0] [YOUR_ADDRESS] [A]"), 3);
    }

    #[test]
    fn spans_cover_the_brackets() {
        let text = "a [FIRST_NAME] b [x] [[PLACEHOLDER]]";
        assert_eq!(found(text), ["[FIRST_NAME]", "[PLACEHOLDER]"]);
    }

    #[test]
    fn a_name_may_hold_digits_apostrophes_and_spaces() {
        let text = "[RESPONSIBILITY_OR_ACHIEVEMENT_1] [DRIVE_1] [WHAT_DOESN'T_WORK] **T:** [YOUR RESPONSIBILITY] [YOUR_PROFILE_STATEMENT_TEMPLATE_1] [END]";
        assert_eq!(found(text), ["[RESPONSIBILITY_OR_ACHIEVEMENT_1]", "[DRIVE_1]", "[WHAT_DOESN'T_WORK]", "[YOUR RESPONSIBILITY]", "[YOUR_PROFILE_STATEMENT_TEMPLATE_1]", "[END]"]);
    }

    #[test]
    fn a_name_may_start_with_a_count() {
        assert_eq!(found("[1-2 SENTENCE_SUMMARY] [1-2] [1- SUMMARY_X] [12 ab] [1-2 ab]"), ["[1-2 SENTENCE_SUMMARY]"]);
        assert_eq!(placeholder_count("[2024 UPDATE] [5 YEARS] [3 KEY_POINTS] [2024 UPDATE_X]"), 0);
    }

    #[test]
    fn every_word_of_a_name_with_spaces_is_a_word() {
        assert_eq!(placeholder_count("Remote [EU UK] and [AI ML] and German [C1 B2] and [YOUR UK] and [WHAT 2 DO]"), 0);
        assert_eq!(found("[WHAT YOU DID] [PREPARE YOUR ANSWER - be honest] [YOUR RESPONSIBILITY]"), ["[WHAT YOU DID]", "[PREPARE YOUR ANSWER - be honest]", "[YOUR RESPONSIBILITY]"]);
        assert_eq!(found("[YOUR KEY_SKILL] [AWS] [AWS - Solutions Architect]"), ["[YOUR KEY_SKILL]", "[AWS]", "[AWS - Solutions Architect]"]);
    }

    #[test]
    fn the_users_own_short_words_are_not_tokens() {
        assert_eq!(placeholder_count("- German [B2], [C1], [EU], [UK], [AI], [AB12]"), 0);
        assert_eq!(placeholder_count("Cloud [AWS, GCP] and Remote [EU, UK] and [UPDATE: moved to Berlin]"), 0);
        assert_eq!(placeholder_count("[LEVEL, EU] [LEVEL: b2]"), 0);
    }

    #[test]
    fn a_one_word_bracket_in_code_is_not_a_token() {
        assert_eq!(placeholder_count("Set `[DEFAULT]` and ``[OTHER]`` here, then [REAL_ONE]"), 1);
        assert_eq!(placeholder_count("Search `[YOUR_JOB_BOARD] [YOUR_CITY]` for \\name{[FIRST_NAME]}"), 3);
        assert_eq!(placeholder_count("```ini\n[DEFAULT]\n[SECOND]\n```\n[REAL_ONE]\n"), 1);
        assert_eq!(placeholder_count("```latex\n\\name{[FIRST_NAME]}\n```\n"), 1);
        assert_eq!(placeholder_count("~~~\n[DEFAULT]\n~~~\n"), 0);
    }

    #[test]
    fn a_hint_after_the_name_belongs_to_the_token() {
        let text = r#"- [CONTEXT - what was happening, who was involved] and [LEVEL, e.g. "Native" / "C2" / "B1/B2 (conversational)"] and [PREPARE YOUR ANSWER - be honest]"#;
        assert_eq!(found(text), ["[CONTEXT - what was happening, who was involved]", r#"[LEVEL, e.g. "Native" / "C2" / "B1/B2 (conversational)"]"#, "[PREPARE YOUR ANSWER - be honest]"]);
        assert_eq!(placeholder_count("[CONTEXT - a [nested] hint] [END]"), 2);
        assert_eq!(placeholder_count("[CONTEXT - never closed"), 0);
        assert_eq!(placeholder_count("[CONTEXT - runs\nto the next line]"), 0);
        assert_eq!(placeholder_count("[Role - Company] [Month Year] [optional]"), 0);
    }

    #[test]
    fn links_references_checkboxes_and_options_are_not_tokens() {
        assert_eq!(placeholder_count("[Jobdesk](https://x.example) [API](url) [ROLE][ref]\n\n[ref]: https://x.example"), 0);
        // With no definition, `[API][ref]` is text on the page, so its token is a blank.
        assert_eq!(placeholder_count("[API][ref]"), 1);
        assert_eq!(placeholder_count("[REF]: https://x.example"), 0);
        assert_eq!(placeholder_count("see [REF]: later"), 1);
        assert_eq!(placeholder_count("[API]: not a definition"), 1);
        assert_eq!(placeholder_count("- [ ] task\n- [x] done\n- [X] done"), 0);
        assert_eq!(placeholder_count(r"\usepackage[T1]{fontenc} a[BC]"), 0);
    }

    #[test]
    fn a_token_counts_in_any_text_the_page_draws() {
        // Link text and image alt text are drawn as written.
        assert_eq!(found("[[YOUR_NAME]](https://x.example/[YOUR_HANDLE])"), ["[YOUR_NAME]"]);
        assert_eq!(found("Logo ![alt [LOGO_NAME]](https://x.example/l.png)"), ["[LOGO_NAME]"]);
        assert_eq!(found("![alt [CITY] $x](u.png) y$"), ["[CITY]"]);
        // A page with no inline math reads a dollar sign as text.
        assert_eq!(found("Salary: [LEVEL - about $50k] and [FLOOR - $40k at least]"), ["[LEVEL - about $50k]", "[FLOOR - $40k at least]"]);
        assert_eq!(found("[ROLE_TITLE]: Led the platform team\n\n[FIRST_NAME][LAST_NAME]\n\n[LAST_NAME](pronouns: she/her)"), ["[ROLE_TITLE]", "[FIRST_NAME]", "[LAST_NAME]", "[LAST_NAME]"]);
    }

    #[test]
    fn a_token_in_link_syntax_or_html_does_not_count() {
        assert_eq!(placeholder_count("[LinkedIn](https://linkedin.com/in/[YOUR_HANDLE])"), 0);
        assert_eq!(placeholder_count("![logo](https://x.example/[LOGO_FILE].png)"), 0);
        assert_eq!(placeholder_count("See [the page][NAME].\n\n[NAME]: https://x.example/c"), 0);
        assert_eq!(placeholder_count("[r]: https://x.example/[PATH]\n\nSee [r]."), 0);
        // A link that reads its own label shows the label without brackets.
        assert_eq!(placeholder_count("See [FIRST_NAME].\n\n[FIRST_NAME]: https://x.example"), 0);
        assert_eq!(placeholder_count("<!-- all [PLACEHOLDER] tokens -->\n\nName: <b>[FIRST_NAME]</b> [LAST_NAME]"), 2);
        assert_eq!(placeholder_count("<div>\n[NAME_X]\n</div>\n\n<!-- old -->[docs]: https://evil.example [LINK_X]"), 0);
    }

    #[test]
    fn a_reference_definition_draws_nothing_so_its_tokens_do_not_count() {
        assert_eq!(found("[FIRST_NAME] [LAST_NAME]\n\n[SECTOR_1]: [EXAMPLE_COMPANIES]\n[x]: https://x.example/$1$"), ["[FIRST_NAME]", "[LAST_NAME]"]);
        assert_eq!(placeholder_count("- [SECTOR_1]: [EXAMPLE_COMPANIES]\n- [SECTOR_1]: [EXAMPLE_COMPANIES]\n"), 0);
        assert_eq!(placeholder_count("[r]: https://x.example\n[r]: https://y.example/[PATH]\n"), 0);
        assert_eq!(placeholder_count("[SECTOR_1]: [EXAMPLE_COMPANIES]\n[Sector_1]: [OTHER_COMPANIES]\n"), 0);
    }

    #[test]
    fn an_escaped_or_encoded_bracket_opens_no_token() {
        assert_eq!(placeholder_count("### Lead \\[TEAM_NAME] - Acme (2020)\nShipped."), 0);
        assert_eq!(placeholder_count("### Lead &#91;TEAM_NAME&#93; - Acme (2020)\nShipped."), 0);
        assert_eq!(found("An escaped backslash, then \\\\[NAME]"), ["[NAME]"]);
    }

    #[test]
    fn a_role_line_has_a_blank_exactly_where_the_count_found_a_token() {
        let part = |text: &str, title: &str| {
            let start = text.find(title).unwrap();
            shown_pieces(text, start..start + title.len())
        };
        let text = "### [JOB_TITLE] - Acme (2020)\nShipped.";
        assert_eq!(part(text, "[JOB_TITLE]"), [Piece::Blank("to fill: job title".to_owned())]);
        assert_eq!(part(text, "Acme"), [Piece::Text("Acme".to_owned())]);
        let text = "### Lead \\[TEAM_NAME] - Acme (2020)";
        assert_eq!(part(text, "Lead \\[TEAM_NAME]"), [Piece::Text("Lead [TEAM_NAME]".to_owned())]);
        let text = "### Lead &#91;TEAM_NAME&#93; - Acme (2020)";
        assert_eq!(part(text, "Lead &#91;TEAM_NAME&#93;"), [Piece::Text("Lead [TEAM_NAME]".to_owned())]);
        let text = "### Run `[YOUR_CITY]` and [CITY_NAME] - Acme (2020)";
        assert_eq!(part(text, "Run `[YOUR_CITY]` and [CITY_NAME]"), [Piece::Text("Run [YOUR_CITY] and ".to_owned()), Piece::Blank("to fill: city name".to_owned())]);
    }

    /// No text of any kind can make a blank on a role line: only a counted token does.
    #[test]
    fn a_role_line_cannot_forge_a_blank() {
        for text in ["### Lead $\u{F0000}$ - [COMPANY] (2020)", "### Lead <b>[TEAM_NAME]</b> - Acme (2020)", "### Lead <span>\u{F0000}</span> - Acme"] {
            let pieces = shown_pieces(text, 4..text.len());
            let blanks = pieces.iter().filter(|piece| matches!(piece, Piece::Blank(_))).count();
            let counted = placeholder_spans(text).len();
            assert_eq!(blanks, counted, "{text}: {pieces:?}");
        }
        assert_eq!(placeholder_count("### Lead $\u{F0000}$ - [COMPANY] (2020)"), 1);
    }

    #[test]
    fn a_block_of_comments_shows_nothing() {
        assert!(!shows_content("## Goals\n<!-- What you want next -->\n\n<!--\n[GOAL_NAME]\n-->\n\n### Sub\n"));
        // A line that opens with a comment is a block of HTML to its end: the words after the comment show, but count nothing.
        assert!(shows_content("## Goals\n<!-- e.g. Ada --> [FIRST_NAME]\n"));
        assert_eq!(placeholder_count("## Goals\n<!-- e.g. Ada --> [FIRST_NAME]\n"), 0);
        assert!(shows_content("## Goals\nRun `x`\n"));
        assert!(shows_content("## Goals\n- [GOAL_NAME]\n"));
    }

    /// A long crafted line counts in time linear in its length.
    #[test]
    fn a_crafted_line_counts_quickly() {
        let crafted = "[AB_C - [[x ".repeat(100 * 1024 / 12) + &"\\".repeat(1000) + &"[A_B".repeat(2000) + &"[A ".repeat(2000);
        let started = std::time::Instant::now();
        assert_eq!(placeholder_count(&crafted), 0);
        assert!(crafted.len() > 100 * 1024);
        assert!(started.elapsed() < std::time::Duration::from_millis(500), "counting took {:?}", started.elapsed());
    }

    #[test]
    fn a_code_token_in_a_role_heading_counts_and_shows_as_written() {
        let text = "### Run `[YOUR_CITY]` - Acme (2020)";
        assert_eq!(placeholder_count(text), 1);
        let title = text.find("Run").unwrap()..text.find(" - ").unwrap();
        assert_eq!(shown_pieces(text, title), [Piece::Text("Run [YOUR_CITY]".to_owned())]);
    }

    #[test]
    fn a_heading_ends_before_its_closing_hashes_and_trailing_comments() {
        for text in [
            "### Lead Engineer - Calder Freight (2021 – present) ###\nShipped.",
            "### Lead Engineer - Calder Freight (2021 – present) <!-- check dates -->\nShipped.",
            "### Lead Engineer - Calder Freight (2021 – present)   <!-- a --> <!-- b --> ##",
        ] {
            assert_eq!(heading_content(text).map(|range| &text[range]), Some("Lead Engineer - Calder Freight (2021 – present)"), "{text}");
        }
        let text = "### Lead <!-- note --> Engineer - Acme (2020)";
        assert_eq!(heading_content(text).map(|range| &text[range]), Some("Lead <!-- note --> Engineer - Acme (2020)"));
        assert_eq!(heading_content("No heading here."), None);
    }

    /// Cutting a heading part costs time linear in the block's length, however many tokens it holds.
    #[test]
    fn a_heading_with_many_tokens_cuts_quickly() {
        let text = format!("### {} - Acme (2020)", "[ROLE_NAME] ".repeat(20_000));
        let started = std::time::Instant::now();
        let pieces = shown_pieces(&text, 4..text.find(" - ").unwrap());
        assert_eq!(pieces.iter().filter(|piece| matches!(piece, Piece::Blank(_))).count(), 20_000);
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "cutting took {:?}", started.elapsed());
    }

    #[test]
    fn html_that_shows_text_is_content() {
        assert!(shows_content("## Identity\n<!-->Open to relocation: no [CITY_NAME]\n"));
        assert!(shows_content("## Identity\n<!-- a --!> Visible\n-->\n"));
        assert!(shows_content("## Identity\n<div>Lisbon</div>\n"));
        assert!(!shows_content("## Identity\n<!---->\n<!-- Your name and city -->\n"));
    }

    #[test]
    fn the_name_stops_before_the_hint() {
        assert_eq!(token_name("FIRST_NAME"), "FIRST_NAME");
        assert_eq!(token_name(r#"LEVEL, e.g. "Native""#), "LEVEL");
        assert_eq!(token_name("1-2 SENTENCE_SUMMARY"), "1-2 SENTENCE_SUMMARY");
        assert_eq!(token_name("CONTEXT - what"), "CONTEXT");
        assert_eq!(token_name("lower"), "");
    }

    #[test]
    fn blank_words_turn_a_token_into_words() {
        assert_eq!(blank_words("FIRST_NAME"), "to fill: first name");
        assert_eq!(blank_words("PLACEHOLDER"), "to fill");
        assert_eq!(blank_words("[LAST_NAME]"), "to fill: last name");
        assert_eq!(blank_words(r#"LEVEL, e.g. "Native" / "C2" / "B1/B2 (conversational)""#), r#"to fill: level, e.g. "Native" / "C2" / "B1/B2 (conversational)""#);
        assert_eq!(blank_words("[CONTEXT - What Was Happening]"), "to fill: context - What Was Happening");
        assert_eq!(blank_words("1-2 SENTENCE_SUMMARY"), "to fill: 1-2 sentence summary");
    }
}
