use std::ops::Range;

/// `/reset` restores tokens such as `[FIRST_NAME]`, and `/setup` replaces them.
pub fn placeholder_count(text: &str) -> usize {
    placeholder_spans(text).len()
}

/// The byte range of each setup token in `text`, brackets included.
///
/// A token is an upper-case name of two characters or more, such as `[FIRST_NAME]`, `[KEY_RESPONSIBILITY_1]` or `[YOUR RESPONSIBILITY]`.
/// The name may be followed by a hint up to the closing bracket: `[CONTEXT - what was happening]`, `[LEVEL, e.g. "Native"]` or `[NOTE: short]`.
/// A link or reference (`[API](url)`, `[API][ref]`, `[API]: url`), an array index or option (`a[0]`, `\usepackage[T1]`) and a lower-case bracket are not tokens.
pub fn placeholder_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans: Vec<Range<usize>> = Vec::new();
    for (start, _) in text.match_indices('[') {
        if spans.last().is_some_and(|last| start < last.end) {
            continue;
        }
        if let Some(end) = token_end(text, start) {
            spans.push(start..end);
        }
    }
    spans
}

/// Where the token that opens at `start` ends, just after its closing bracket.
fn token_end(text: &str, start: usize) -> Option<usize> {
    if text[..start].chars().next_back().is_some_and(char::is_alphanumeric) {
        return None;
    }
    let after = start + 1;
    let name = name_len(&text[after..]);
    if name < 2 {
        return None;
    }
    let rest = &text[after + name..];
    let close = if rest.starts_with(']') {
        after + name
    } else if [" - ", ", ", ": "].iter().any(|separator| rest.starts_with(separator)) {
        matching_close(text, after + name)?
    } else {
        return None;
    };
    let next = text[close + 1..].chars().next();
    let line_start = text[..start].rsplit(['\n', '\r']).next().is_some_and(|line| line.trim().is_empty());
    let link = matches!(next, Some('(' | '[')) || (next == Some(':') && line_start);
    (!link).then_some(close + 1)
}

/// The length of an upper-case name at the start of `text`: A-Z, digits, `_` and `'`, with single spaces between upper-case words.
fn name_len(text: &str) -> usize {
    if !text.starts_with(|c: char| c.is_ascii_uppercase()) {
        return 0;
    }
    let bytes = text.as_bytes();
    let mut len = 0;
    while len < bytes.len() {
        let c = bytes[len];
        let word = c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_' || c == b'\'';
        let joined = c == b' ' && bytes.get(len + 1).is_some_and(u8::is_ascii_uppercase);
        if !(word || joined) {
            break;
        }
        len += 1;
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
    use super::{placeholder_count, placeholder_spans};

    fn found(text: &str) -> Vec<&str> {
        placeholder_spans(text).into_iter().map(|span| &text[span]).collect()
    }

    #[test]
    fn counts_only_setup_tokens() {
        assert_eq!(placeholder_count(r"\name{[FIRST_NAME]}{[LAST_NAME]} [x] [AB] [Note] - [ ] a[0] [YOUR_ADDRESS] [A]"), 4);
    }

    #[test]
    fn spans_cover_the_brackets() {
        let text = "a [FIRST_NAME] b [x] [[PLACEHOLDER]]";
        assert_eq!(found(text), ["[FIRST_NAME]", "[PLACEHOLDER]"]);
    }

    #[test]
    fn a_name_may_hold_digits_apostrophes_and_spaces() {
        let text = "[RESPONSIBILITY_OR_ACHIEVEMENT_1] [DRIVE_1] [WHAT_DOESN'T_WORK] **T:** [YOUR RESPONSIBILITY] [YOUR_PROFILE_STATEMENT_TEMPLATE_1] [T1]";
        assert_eq!(found(text), ["[RESPONSIBILITY_OR_ACHIEVEMENT_1]", "[DRIVE_1]", "[WHAT_DOESN'T_WORK]", "[YOUR RESPONSIBILITY]", "[YOUR_PROFILE_STATEMENT_TEMPLATE_1]", "[T1]"]);
    }

    #[test]
    fn a_hint_after_the_name_belongs_to_the_token() {
        let text = r#"- [CONTEXT - what was happening, who was involved] and [LEVEL, e.g. "Native" / "C2" / "B1/B2 (conversational)"] and [NOTE: short] [PREPARE YOUR ANSWER - be honest]"#;
        assert_eq!(
            found(text),
            ["[CONTEXT - what was happening, who was involved]", r#"[LEVEL, e.g. "Native" / "C2" / "B1/B2 (conversational)"]"#, "[NOTE: short]", "[PREPARE YOUR ANSWER - be honest]"]
        );
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
}
