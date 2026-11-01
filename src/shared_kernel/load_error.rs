use std::fmt;

use crate::shared_kernel::visible_text::visible_chars;

/// Why a source file could not be read. Missing optional files are not errors.
/// Both parts can quote a file, so they keep only what shows on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    pub source: String,
    pub reason: String,
}

impl LoadError {
    pub fn new(source: impl Into<String>, reason: impl ToString) -> Self {
        Self { source: one_line(&source.into()), reason: one_line(&reason.to_string()) }
    }
}

/// Line breaks become spaces, so words on two lines do not run together.
/// Vertical tab, form feed and Unicode's next line, line and paragraph separators break lines too.
fn one_line(text: &str) -> String {
    text.split(['\r', '\n', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}']).map(|line| visible_chars(line).collect::<String>()).filter(|line| !line.trim().is_empty()).collect::<Vec<_>>().join(" ")
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.source, self.reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_load_error_reason_drops_hidden_characters() {
        let error = LoadError::new("job_search_tracker.csv", "line 3: \u{1b}[31mbad\u{1b}[0m field \u{202E}vsc.exe\nsee row 4");
        assert_eq!(error.reason, "line 3: bad field vsc.exe see row 4");
        assert_eq!(LoadError::new("notes\u{200B}.md", "x").source, "notes.md");
        assert_eq!(LoadError::new("f", "one\u{85}two\u{2028}three\u{2029}four\u{b}five\u{c}six").reason, "one two three four five six");
    }
}
