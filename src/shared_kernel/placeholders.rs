use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};

use crate::shared_kernel::markdown;

/// `/reset` restores tokens such as `[FIRST_NAME]`, and `/setup` replaces them.
pub fn placeholder_count(text: &str) -> usize {
    placeholder_spans(text).len()
}

/// The byte range of each setup token in `text`, brackets included.
///
/// A token is an upper-case name such as `[FIRST_NAME]`, `[KEY_RESPONSIBILITY_1]`, `[YOUR RESPONSIBILITY]` or `[1-2 SENTENCE_SUMMARY]`.
/// A name of one word has three letters or more and no digit, so `[B2]`, `[EU]` and `[AB12]` are the user's own words.
/// The name may be followed by a hint up to the closing bracket: `[CONTEXT - what was happening]` or `[LEVEL, e.g. "Native"]`.
/// A link or reference (`[API](url)`, `[API][ref]`, `[API]: url`), an array index or option (`a[0]`, `\usepackage[T1]`),
/// a lower-case bracket and a one-word name in a code span or a fenced block (`[DEFAULT]`) are not tokens.
/// A name with `_` or spaces counts in code too, because the templates keep blanks like `[YOUR_CITY]` in their example queries and LaTeX.
pub fn placeholder_spans(text: &str) -> Vec<Range<usize>> {
    let code = code_ranges(text);
    let mut spans: Vec<Range<usize>> = Vec::new();
    for (start, _) in text.match_indices('[') {
        if spans.last().is_some_and(|last| start < last.end) {
            continue;
        }
        let in_code = code.iter().any(|range| range.contains(&start));
        if let Some((end, plain)) = token_end(text, start)
            && !(in_code && plain)
        {
            spans.push(start..end);
        }
    }
    spans
}

/// The code spans and fenced blocks of `text`.
fn code_ranges(text: &str) -> Vec<Range<usize>> {
    if !text.contains('`') && !text.contains("~~~") {
        return Vec::new();
    }
    Parser::new_ext(text, markdown::options())
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Code(_) | Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) => Some(range),
            _ => None,
        })
        .collect()
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
    use super::{placeholder_count, placeholder_spans, token_name};

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
        assert_eq!(placeholder_count("[Jobdesk](https://x.example) [API][ref] [API](url)"), 0);
        assert_eq!(placeholder_count("[REF]: https://x.example"), 0);
        assert_eq!(placeholder_count("see [REF]: later"), 1);
        assert_eq!(placeholder_count("- [ ] task\n- [x] done\n- [X] done"), 0);
        assert_eq!(placeholder_count(r"\usepackage[T1]{fontenc} a[BC]"), 0);
    }

    #[test]
    fn the_name_stops_before_the_hint() {
        assert_eq!(token_name("FIRST_NAME"), "FIRST_NAME");
        assert_eq!(token_name(r#"LEVEL, e.g. "Native""#), "LEVEL");
        assert_eq!(token_name("1-2 SENTENCE_SUMMARY"), "1-2 SENTENCE_SUMMARY");
        assert_eq!(token_name("CONTEXT - what"), "CONTEXT");
        assert_eq!(token_name("lower"), "");
    }
}
