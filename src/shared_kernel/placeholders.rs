use std::ops::Range;

/// `/reset` restores tokens such as `[FIRST_NAME]`, and `/setup` replaces them.
pub fn placeholder_count(text: &str) -> usize {
    placeholder_spans(text).len()
}

/// The byte range of each setup token in `text`, brackets included.
pub fn placeholder_spans(text: &str) -> Vec<Range<usize>> {
    text.match_indices('[')
        .filter_map(|(start, _)| {
            let rest = &text[start + 1..];
            let end = rest.find(']')?;
            let token = &rest[..end];
            let setup = token.len() >= 3 && token.chars().all(|c| c.is_ascii_uppercase() || c == '_') && token.chars().any(|c| c.is_ascii_uppercase());
            setup.then_some(start..start + end + 2)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{placeholder_count, placeholder_spans};

    #[test]
    fn counts_only_setup_tokens() {
        assert_eq!(placeholder_count(r"\name{[FIRST_NAME]}{[LAST_NAME]} [x] [AB] [Note] - [ ] a[0] [YOUR_ADDRESS]"), 3);
    }

    #[test]
    fn spans_cover_the_brackets() {
        let text = "a [FIRST_NAME] b [x] [[PLACEHOLDER]]";
        let found: Vec<&str> = placeholder_spans(text).into_iter().map(|span| &text[span]).collect();
        assert_eq!(found, ["[FIRST_NAME]", "[PLACEHOLDER]"]);
    }
}
