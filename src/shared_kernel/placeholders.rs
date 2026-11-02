use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::shared_kernel::markdown;

/// The setup tokens a page of profile Markdown draws, as `page` counts them.
pub fn placeholder_count(text: &str) -> usize {
    page(text).count
}

/// Profile Markdown as a page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The text the viewer renders: the text as given, with each token it draws in prose swapped for a blank, an inline
    /// math span whose content `blank_index` reads. Nothing else in it changes.
    pub text: String,
    /// Each blank, by index.
    pub blanks: Arc<[Blank]>,
    /// The tokens the page draws: its blanks, and the tokens shown as written in code.
    pub count: usize,
}

/// One blank on a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blank {
    pub words: String,
    /// The level of the heading it sits in, from 1 to 6, or none.
    pub heading: Option<usize>,
}

/// `/reset` restores tokens such as `[FIRST_NAME]`, and `/setup` replaces them. A token counts only where the page draws
/// it, as the viewer parses the page with its own options:
///
/// - as a blank, when it reads as prose text: each such token is swapped for an inline math span holding this process's
///   nonce and the blank's index, and it counts only if that span survives as one in the parse of the swapped text;
/// - as written, in a code span or a code block, where a one-word name (`[DEFAULT]`) does not count but a name with `_`
///   or spaces does, because the templates keep blanks like `[YOUR_CITY]` in their example queries and LaTeX.
///
/// Nothing else counts: link or image text, destinations and reference labels, reference definitions, raw HTML, a
/// token inside a math span of the text's own, an escaped bracket or an entity. The count and the blanks come from the
/// same parse of the same text, so they cannot disagree.
///
/// A token is an upper-case name such as `[FIRST_NAME]`, `[KEY_RESPONSIBILITY_1]`, `[YOUR RESPONSIBILITY]` or `[1-2 SENTENCE_SUMMARY]`.
/// A name of one word has three letters or more and no digit, so `[B2]`, `[EU]` and `[AB12]` are the user's own words.
/// The name may be followed by a hint up to the closing bracket: `[CONTEXT - what was happening]` or `[LEVEL, e.g. "Native"]`.
/// A bracket right after a letter or a digit (`a[0]`, `\usepackage[T1]`) and a lower-case bracket are not tokens.
pub fn page(text: &str) -> Page {
    let found = prose_tokens(text);
    let mut marked = vec![true; found.len()];
    loop {
        // `owners[i]` is the token blank `i` stands for.
        let owners: Vec<usize> = (0..found.len()).filter(|&index| marked[index]).collect();
        let swapped = swapped(text, &found, &owners);
        let drawn = drawn(&swapped, owners.len());
        let failed: Vec<usize> = (0..owners.len()).filter(|&blank| drawn.seen[blank] != 1).collect();
        if failed.is_empty() {
            let blanks = owners.iter().zip(drawn.headings).map(|(&owner, heading)| Blank { words: blank_words(&text[found[owner].clone()]), heading }).collect();
            return Page { text: swapped, blanks, count: owners.len() + drawn.code };
        }
        // A swap that did not come out as one blank, such as one that paired with a `$` of the text's own, stays as written.
        for blank in failed {
            marked[owners[blank]] = false;
        }
    }
}

/// What a parse of a swapped page draws: how often each blank's span came out, the heading each sits in, and the
/// tokens shown as written in code.
struct Drawn {
    seen: Vec<usize>,
    headings: Vec<Option<usize>>,
    code: usize,
}

fn drawn(text: &str, blanks: usize) -> Drawn {
    let mut drawn = Drawn { seen: vec![0; blanks], headings: vec![None; blanks], code: 0 };
    let (mut heading, mut in_block, mut in_image) = (None, false, 0);
    for (event, range) in Parser::new_ext(text, page_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => heading = Some(level as usize),
            Event::End(TagEnd::Heading(_)) => heading = None,
            Event::Start(Tag::CodeBlock(_)) => in_block = true,
            Event::End(TagEnd::CodeBlock) => in_block = false,
            // An image shows its alt text as plain text, so nothing in it is a blank or code.
            Event::Start(Tag::Image { .. }) => in_image += 1,
            Event::End(TagEnd::Image) => in_image -= 1,
            Event::InlineMath(content) if in_image == 0 => {
                if let Some(blank) = blank_index(&content).filter(|&blank| blank < blanks) {
                    drawn.seen[blank] += 1;
                    drawn.headings[blank] = heading;
                }
            }
            Event::Text(_) if in_block => drawn.code += code_tokens(&text[range]),
            Event::Code(_) if in_image == 0 => drawn.code += code_tokens(&text[range]),
            _ => {}
        }
    }
    drawn
}

/// The tokens in prose text, where the page would draw them as blanks: text outside links and images, unescaped.
fn prose_tokens(text: &str) -> Vec<Range<usize>> {
    let mut prose: Vec<Range<usize>> = Vec::new();
    let (mut in_block, mut in_link, mut in_image) = (false, 0, 0);
    for (event, range) in Parser::new_ext(text, page_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_block = true,
            Event::End(TagEnd::CodeBlock) => in_block = false,
            Event::Start(Tag::Link { .. }) => in_link += 1,
            Event::End(TagEnd::Link) => in_link -= 1,
            Event::Start(Tag::Image { .. }) => in_image += 1,
            Event::End(TagEnd::Image) => in_image -= 1,
            Event::Text(_) if !in_block && in_link == 0 && in_image == 0 => prose.push(range),
            _ => {}
        }
    }
    // Brackets that form no link reach the page as text events of their own, so neighbours merge into one run.
    let mut runs: Vec<Range<usize>> = Vec::new();
    for range in prose {
        match runs.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => runs.push(range),
        }
    }
    let mut tokens: Vec<Range<usize>> = Vec::new();
    for run in runs {
        for span in line_placeholders(&text[run.clone()]) {
            let span = run.start + span.start..run.start + span.end;
            if !escaped_at(text, span.start) && !(span.start > 0 && text[..span.start].chars().next_back().is_some_and(char::is_alphanumeric)) {
                tokens.push(span);
            }
        }
    }
    tokens
}

/// `text` with the tokens in `found` that `owners` names swapped for their blanks, in order.
fn swapped(text: &str, found: &[Range<usize>], owners: &[usize]) -> String {
    let mut out = String::with_capacity(text.len() + owners.len() * 40);
    let mut copied = 0;
    for (blank, &owner) in owners.iter().enumerate() {
        let span = &found[owner];
        out.push_str(&text[copied..span.start]);
        out.push_str(&format!("${}{blank}$", nonce()));
        copied = span.end;
    }
    out.push_str(&text[copied..]);
    out
}

/// The tokens a code span or a code block shows as written: names with `_` or spaces.
fn code_tokens(code: &str) -> usize {
    line_placeholders(code).into_iter().filter(|span| token_name(&code[span.start + 1..]).contains(['_', ' '])).count()
}

/// The blank an inline math span on a page stands for, when its content is this process's nonce and an index. Untrusted
/// text cannot guess the nonce, so no math of its own reads as a blank.
pub fn blank_index(content: &str) -> Option<usize> {
    let index = content.strip_prefix(nonce())?;
    if index.is_empty() || !index.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    index.parse().ok()
}

/// A random alphanumeric string, made once per process.
pub(crate) fn nonce() -> &'static str {
    static NONCE: OnceLock<String> = OnceLock::new();
    NONCE.get_or_init(|| {
        let mut nonce = String::from("blank");
        for salt in 0..2u8 {
            let mut hasher = RandomState::new().build_hasher();
            hasher.write_u8(salt);
            nonce.push_str(&format!("{:016x}", hasher.finish()));
        }
        nonce.push('n');
        nonce
    })
}

/// The first heading of a page as it shows, for a role line that is drawn outside Markdown: each blank is the character
/// `blank_char` gives, and a character of the text's own in that range shows as U+FFFD, so the text cannot forge one.
pub fn heading_text(page: &Page) -> Option<String> {
    let mut shown: Option<String> = None;
    for event in Parser::new_ext(&page.text, page_options()) {
        let Some(out) = shown.as_mut() else {
            if let Event::Start(Tag::Heading { .. }) = event {
                shown = Some(String::new());
            }
            continue;
        };
        match event {
            Event::End(TagEnd::Heading(_)) => break,
            Event::Text(text) | Event::Code(text) => out.extend(text.chars().map(|c| if blank_of(c).is_some() { '\u{FFFD}' } else { c })),
            Event::InlineMath(content) => match blank_index(&content).filter(|&blank| blank < page.blanks.len()) {
                Some(blank) => out.push(blank_char(blank)),
                None => out.push_str(&format!("${content}$")),
            },
            Event::DisplayMath(content) => out.push_str(&format!("$${content}$$")),
            Event::InlineHtml(html) | Event::Html(html) => out.push_str(&html),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    shown
}

/// The first of the private-use characters that stand for blanks in a `heading_text`.
const BLANK_CHARS: u32 = 0xF0000;

/// The character that stands for blank `index` in a `heading_text`.
pub fn blank_char(index: usize) -> char {
    u32::try_from(index).ok().and_then(|index| char::from_u32(BLANK_CHARS + index.min(0xFFFD))).unwrap_or('\u{FFFD}')
}

/// The blank a character of a `heading_text` stands for, if any.
pub fn blank_of(c: char) -> Option<usize> {
    let code = u32::from(c);
    (BLANK_CHARS..=BLANK_CHARS + 0xFFFD).contains(&code).then(|| (code - BLANK_CHARS) as usize)
}

/// The text a profile page shows for a block: the block with its HTML comments removed, so no `<!-- … -->` reaches the
/// paper. A comment inside code is code, and stays.
pub fn shown_text(block: &str) -> String {
    if !block.contains("<!--") {
        return block.to_owned();
    }
    let mut comments: Vec<Range<usize>> = Vec::new();
    for (event, range) in Parser::new_ext(block, page_options()).into_offset_iter() {
        if let Event::Html(html) | Event::InlineHtml(html) = event
            && html.trim_start().starts_with("<!--")
            && comments.last().is_none_or(|last| range.start >= last.end)
        {
            let start = range.start + block[range.start..].find("<!--").unwrap_or(0);
            let end = block[start..].find("-->").map_or(block.len(), |end| start + end + 3);
            comments.push(start..end);
        }
    }
    let mut shown = String::with_capacity(block.len());
    let mut copied = 0;
    for comment in comments {
        shown.push_str(&block[copied..comment.start]);
        copied = comment.end;
    }
    shown.push_str(&block[copied..]);
    shown
}

/// The setup tokens in plain text, by the name rule alone, with no Markdown, such as for text a screen paints itself.
pub fn line_placeholders(line: &str) -> Vec<Range<usize>> {
    let mut spans: Vec<Range<usize>> = Vec::new();
    for (start, _) in line.match_indices('[') {
        if spans.last().is_some_and(|last| start < last.end) {
            continue;
        }
        if let Some(end) = token_end(line, start) {
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
    use super::{Blank, blank_char, blank_index, blank_of, blank_words, heading_text, line_placeholders, nonce, page, placeholder_count, shown_text, token_name};

    /// The tokens a page swaps for blanks, as written.
    fn found(text: &str) -> Vec<String> {
        let page = page(text);
        let tokens: Vec<String> = super::prose_tokens(text).into_iter().map(|span| text[span].to_owned()).collect();
        assert_eq!(tokens.len(), page.blanks.len(), "{text}: every token found in prose comes out as a blank");
        tokens
    }

    /// The words of each blank the page draws.
    fn words_of(text: &str) -> Vec<String> {
        page(text).blanks.iter().map(|blank| blank.words.clone()).collect()
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
    fn a_page_changes_only_its_blanks() {
        let text = "Pay $1$ [AMOUNT] in `$x$` and <https://x.example/$y$> or [s](https://x.example/$z$) and [r]\n\n[r]: https://x.example/$q$";
        let swapped = page(text).text;
        assert_eq!(swapped, text.replace("[AMOUNT]", &format!("${}0$", nonce())));
        assert_eq!(page("Paid \\$5 and \\\\$6 [CITY]").text, format!("Paid \\$5 and \\\\$6 ${}0$", nonce()));
    }

    #[test]
    fn a_blank_knows_its_heading() {
        let page = page("## Goals [GOAL_NAME]\n\nRun `[YOUR_CITY]` for [FIRST_NAME].\n");
        let headings: Vec<Option<usize>> = page.blanks.iter().map(|blank| blank.heading).collect();
        assert_eq!(headings, [Some(2), None]);
        assert_eq!(page.count, 3);
    }

    #[test]
    fn only_the_nonce_names_a_blank() {
        assert_eq!(blank_index(&format!("{}12", nonce())), Some(12));
        for content in ["12", "blank12", "", nonce(), &format!("{}1a", nonce()), &format!("x{}1", nonce())] {
            assert_eq!(blank_index(content), None, "{content}");
        }
        assert!(nonce().len() > 30 && nonce().bytes().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn a_math_span_of_the_texts_own_holds_no_blank() {
        // `$5 [UNIT]$` pairs as math in the page, so its token is the text's own math, drawn as written.
        assert_eq!(placeholder_count("Costs $5 [UNIT_NAME]$ a day"), 0);
        // A `$` that pairs with a blank's swap leaves that token as written, and uncounted.
        let page = page("Pay $5 for[x] and $[ROLE_NAME] now [CITY]");
        assert_eq!(page.count, page.blanks.len());
    }

    #[test]
    fn a_hint_with_a_dollar_sign_is_one_blank() {
        assert_eq!(words_of("Salary: [LEVEL - about $50k]"), ["to fill: level - about $50k"]);
        assert_eq!(placeholder_count("Salary: [LEVEL - about $50k] and [FLOOR - $40k at least]"), 2);
    }

    #[test]
    fn a_bracket_the_page_shows_as_text_is_a_blank_whatever_follows() {
        assert_eq!(words_of("[ROLE_TITLE]: Led the platform team"), ["to fill: role title"]);
        assert_eq!(words_of("[FIRST_NAME][LAST_NAME]"), ["to fill: first name", "to fill: last name"]);
        assert_eq!(words_of("[LAST_NAME](pronouns: she/her)"), ["to fill: last name"]);
    }

    #[test]
    fn a_reference_label_with_code_or_html_stays_a_link() {
        assert_eq!(page("See [run `$x`].\n\n[run `$x`]: https://x.example").text, "See [run `$x`].\n\n[run `$x`]: https://x.example");
        let html = "See [a <b>x</b>] about [CITY].\n\n[a <b>x</b>]: https://x.example";
        assert_eq!(page(html).text, html.replace("[CITY]", &format!("${}0$", nonce())));
    }

    #[test]
    fn an_escaped_or_encoded_bracket_is_no_blank() {
        for text in ["### Lead \\[TEAM_NAME] - Acme (2020)\nShipped.", "### Lead &#91;TEAM_NAME&#93; - Acme (2020)\nShipped."] {
            let page = page(text);
            assert_eq!(page.count, 0, "{text}");
            assert_eq!(heading_text(&page).as_deref(), Some("Lead [TEAM_NAME] - Acme (2020)"));
        }
        let page = page("### [JOB_TITLE] - Acme (2020)\nShipped.");
        assert_eq!(page.count, 1);
        assert_eq!(heading_text(&page), Some(format!("{} - Acme (2020)", blank_char(0))));
        assert_eq!(page.blanks[..], [Blank { words: "to fill: job title".to_owned(), heading: Some(3) }]);
    }

    #[test]
    fn a_heading_cannot_forge_a_blank() {
        let page = page(&format!("### Lead {} - Acme (2020)", blank_char(0)));
        assert_eq!(page.count, 0);
        assert!(heading_text(&page).is_some_and(|shown| shown.chars().all(|c| blank_of(c).is_none())));
    }

    #[test]
    fn the_page_shows_no_comment() {
        assert_eq!(shown_text("### Identity\n<!-- e.g. Ada --> [FIRST_NAME]\n"), "### Identity\n [FIRST_NAME]\n");
        assert_eq!(shown_text("a <!-- x --> b\n<!--\n[X_Y]\n-->\nc"), "a  b\n\nc");
        assert_eq!(shown_text("Run `<!-- [YOUR_CITY] -->`"), "Run `<!-- [YOUR_CITY] -->`");
        assert_eq!(placeholder_count(&shown_text("### Identity\n<!-- e.g. Ada --> [FIRST_NAME]\n")), 1);
        assert_eq!(placeholder_count(&shown_text("Run `<!-- [YOUR_CITY] -->`")), 1);
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
