use std::ops::Range;

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

use crate::shared_kernel::markdown;

/// The setup tokens a page of profile Markdown draws, as `page_placeholders` finds them.
pub fn placeholder_count(text: &str) -> usize {
    page_placeholders(text).tokens.len()
}

/// Profile Markdown as a page shows it: `text`, with every `$` of its own escaped, and the setup tokens it draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub text: String,
    /// In order, as byte ranges of `text`, brackets included.
    pub tokens: Vec<PageToken>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageToken {
    pub span: Range<usize>,
    pub place: Place,
}

/// Where a page draws a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// In prose, where it reads as a blank, inside a heading of `heading` from 1 to 6 or none.
    Prose { heading: Option<usize> },
    /// As written, in a code span or a code block.
    Code,
}

/// `/reset` restores tokens such as `[FIRST_NAME]`, and `/setup` replaces them. A token counts only where the page draws
/// it: in prose text, where it reads as a blank, or as written in a code span or a code block.
///
/// Nothing else counts, because the page draws it as no blank and no code: a link's or image's text, destination or
/// reference label, a reference definition, duplicates included (the template's `- [SECTOR_1]: [EXAMPLE_COMPANIES]` is
/// one), and an HTML block or tag, comments included. Text between two HTML tags is prose. The page is parsed once, with the options the viewer uses, so the count and the blanks come from the same
/// positions. Every `$` of the text's own is escaped first, outside code, HTML and autolinks, so none of it can open
/// inline math on the page, and a reference label keeps matching its definition because both are escaped alike.
///
/// A token is an upper-case name such as `[FIRST_NAME]`, `[KEY_RESPONSIBILITY_1]`, `[YOUR RESPONSIBILITY]` or `[1-2 SENTENCE_SUMMARY]`.
/// A name of one word has three letters or more and no digit, so `[B2]`, `[EU]` and `[AB12]` are the user's own words.
/// The name may be followed by a hint up to the closing bracket: `[CONTEXT - what was happening]` or `[LEVEL, e.g. "Native"]`.
/// A bracket right after a letter or a digit (`a[0]`, `\usepackage[T1]`), a lower-case bracket, a bracket followed by
/// `(` or `[`, or by `:` at the start of a line, and an escaped bracket are not tokens. In code, a one-word name
/// (`[DEFAULT]`) is not a token, but a name with `_` or spaces is, because the templates keep blanks like `[YOUR_CITY]`
/// in their example queries and LaTeX.
pub fn page_placeholders(text: &str) -> Page {
    let text = without_math(text);
    let mut prose: Vec<(Range<usize>, Option<usize>)> = Vec::new();
    let mut code: Vec<Range<usize>> = Vec::new();
    let (mut heading, mut in_block, mut in_link, mut in_image) = (None, false, 0, 0);
    for (event, range) in Parser::new_ext(&text, page_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => heading = Some(level as usize),
            Event::End(TagEnd::Heading(_)) => heading = None,
            Event::Start(Tag::CodeBlock(_)) => in_block = true,
            Event::End(TagEnd::CodeBlock) => in_block = false,
            Event::Start(Tag::Link { .. }) => in_link += 1,
            Event::End(TagEnd::Link) => in_link -= 1,
            Event::Start(Tag::Image { .. }) => in_image += 1,
            Event::End(TagEnd::Image) => in_image -= 1,
            Event::Text(_) if in_block => code.push(range),
            Event::Text(_) if in_link == 0 && in_image == 0 => prose.push((range, heading)),
            Event::Code(_) if in_image == 0 => code.push(range),
            _ => {}
        }
    }
    // Brackets that form no link reach the page as text events of their own, so neighbours merge into one run.
    let mut runs: Vec<(Range<usize>, Option<usize>)> = Vec::new();
    for (range, level) in prose {
        match runs.last_mut() {
            Some((last, last_level)) if range.start <= last.end && *last_level == level => last.end = last.end.max(range.end),
            _ => runs.push((range, level)),
        }
    }
    let mut tokens: Vec<PageToken> = Vec::new();
    for (start, _) in text.match_indices('[') {
        if tokens.last().is_some_and(|last| start < last.span.end) {
            continue;
        }
        let Some((end, plain)) = token_end(&text, start) else { continue };
        let place = if let Some((_, level)) = runs.iter().find(|(run, _)| run.start <= start && end <= run.end) {
            (!escaped_at(&text, start)).then_some(Place::Prose { heading: *level })
        } else {
            code.iter().any(|range| range.start <= start && end <= range.end && !plain).then_some(Place::Code)
        };
        if let Some(place) = place {
            tokens.push(PageToken { span: start..end, place });
        }
    }
    Page { text, tokens }
}

/// The setup tokens in one plain line, such as a role's title, which is never Markdown.
pub fn line_placeholders(line: &str) -> Vec<Range<usize>> {
    let mut spans: Vec<Range<usize>> = Vec::new();
    for (start, _) in line.match_indices('[') {
        if spans.last().is_some_and(|last| start < last.end) {
            continue;
        }
        if let Some((end, _)) = token_end(line, start) {
            spans.push(start..end);
        }
    }
    spans
}

/// The options the viewer parses a page of profile Markdown with: its blanks are inline math.
fn page_options() -> Options {
    markdown::options() | Options::ENABLE_MATH
}

/// Whether the character at `at` is escaped by the backslashes before it.
fn escaped_at(text: &str, at: usize) -> bool {
    text[..at].bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

/// `text` with each `$` escaped where Markdown reads escapes: everywhere but code, raw HTML and autolinks, where a
/// backslash would show or change an address. A destination or a title reads the escape back, and a reference label
/// matches its definition still, because both are escaped alike.
fn without_math(text: &str) -> String {
    if !text.contains('$') {
        return text.to_owned();
    }
    let kept: Vec<Range<usize>> = Parser::new_ext(text, markdown::options())
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::Link { link_type: LinkType::Autolink | LinkType::Email, .. }) | Event::Code(_) | Event::Html(_) | Event::InlineHtml(_) => Some(range),
            _ => None,
        })
        .collect();
    let mut out = String::with_capacity(text.len() + 8);
    for (at, c) in text.char_indices() {
        if c == '$' && !escaped_at(text, at) && !kept.iter().any(|range| range.contains(&at)) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Where the token that opens at `start` ends, just after its closing bracket, and whether its name is a single plain word such as `DEFAULT`.
fn token_end(text: &str, start: usize) -> Option<(usize, bool)> {
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
    let next = text[close + 1..].chars().next();
    let line_start = text[..start].rsplit(['\n', '\r']).next().is_some_and(|line| line.trim().is_empty());
    let link = matches!(next, Some('(' | '[')) || (next == Some(':') && line_start);
    (!link).then_some((close + 1, !text[after..after + name].contains(['_', ' '])))
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

/// The `]` that closes the token whose hint starts at `from`, skipping brackets nested in the hint. A line break ends the search.
fn matching_close(text: &str, from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, c) in text[from..].char_indices() {
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
    use super::{Place, blank_words, line_placeholders, page_placeholders, placeholder_count, token_name};

    fn found(text: &str) -> Vec<String> {
        let page = page_placeholders(text);
        page.tokens.iter().map(|token| page.text[token.span.clone()].to_owned()).collect()
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
        assert_eq!(placeholder_count("[Jobdesk](https://x.example) [API][ref] [API](url)"), 0);
        assert_eq!(placeholder_count("[REF]: https://x.example"), 0);
        assert_eq!(placeholder_count("see [REF]: later"), 1);
        assert_eq!(placeholder_count("- [ ] task\n- [x] done\n- [X] done"), 0);
        assert_eq!(placeholder_count(r"\usepackage[T1]{fontenc} a[BC]"), 0);
    }

    #[test]
    fn a_token_the_viewer_hides_in_link_syntax_is_not_counted() {
        assert_eq!(placeholder_count("[LinkedIn](https://linkedin.com/in/[YOUR_HANDLE])"), 0);
        assert_eq!(placeholder_count("![logo](https://x.example/[LOGO_FILE].png)"), 0);
        assert_eq!(placeholder_count("See [the page][NAME].\n\n[NAME]: https://x.example/c"), 0);
        assert_eq!(placeholder_count("[r]: https://x.example/[PATH]\n\nSee [r]."), 0);
        // Link text shows without its brackets, so a token there is no blank and does not count.
        assert_eq!(placeholder_count("[[YOUR_NAME]](https://x.example/[YOUR_HANDLE])"), 0);
        assert_eq!(placeholder_count("See [FIRST_NAME].\n\n[FIRST_NAME]: https://x.example"), 0);
        assert_eq!(placeholder_count("Logo ![alt [LOGO_NAME]](https://x.example/l.png)"), 0);
    }

    #[test]
    fn a_reference_definition_draws_nothing_so_its_tokens_do_not_count() {
        assert_eq!(found("[FIRST_NAME] [LAST_NAME]\n\n[SECTOR_1]: [EXAMPLE_COMPANIES]\n[x]: https://x.example/$1$"), ["[FIRST_NAME]", "[LAST_NAME]"]);
        assert_eq!(placeholder_count("- [SECTOR_1]: [EXAMPLE_COMPANIES]\n- [SECTOR_1]: [EXAMPLE_COMPANIES]\n"), 0);
        assert_eq!(placeholder_count("[r]: https://x.example\n[r]: https://y.example/[PATH]\n"), 0);
        assert_eq!(placeholder_count("[SECTOR_1]: [EXAMPLE_COMPANIES]\n[Sector_1]: [OTHER_COMPANIES]\n"), 0);
    }

    #[test]
    fn a_comment_draws_no_blank_so_its_tokens_do_not_count() {
        assert_eq!(placeholder_count("<!-- all [PLACEHOLDER] tokens -->\n\nName: <b>[FIRST_NAME]</b> [LAST_NAME]"), 2);
    }

    #[test]
    fn a_page_escapes_every_dollar_sign_of_its_own() {
        let page = page_placeholders("Pay $1$ [AMOUNT] in `$x$` and <https://x.example/$y$> or [s](https://x.example/$z$)");
        assert_eq!(page.text, "Pay \\$1\\$ [AMOUNT] in `$x$` and <https://x.example/$y$> or [s](https://x.example/\\$z\\$)");
        assert_eq!(page.tokens.len(), 1);
        assert_eq!(page_placeholders("Paid \\$5 and \\\\$6").text, "Paid \\$5 and \\\\\\$6");
    }

    #[test]
    fn a_token_knows_where_the_page_draws_it() {
        let page = page_placeholders("## Goals [GOAL_NAME]\n\nRun `[YOUR_CITY]` for [FIRST_NAME].\n");
        let places: Vec<Place> = page.tokens.iter().map(|token| token.place).collect();
        assert_eq!(places, [Place::Prose { heading: Some(2) }, Place::Code, Place::Prose { heading: None }]);
    }

    #[test]
    fn a_plain_line_is_not_markdown() {
        let line = "Lead - [COMPANY] (`[DATES]`)";
        let found: Vec<&str> = line_placeholders(line).into_iter().map(|span| &line[span]).collect();
        assert_eq!(found, ["[COMPANY]", "[DATES]"]);
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
