//! What Windows editors leave in a text file: a byte order mark at its start, and CRLF or lone CR line endings.

/// The byte order mark Windows editors such as Notepad may write at the start of a UTF-8 file.
pub const BOM: char = '\u{feff}';

/// `text` without a byte order mark at its start, so a heading or front matter on line 1 still reads as one.
pub fn without_bom(text: &str) -> &str {
    text.strip_prefix(BOM).unwrap_or(text)
}

/// `text` with every CRLF and every lone CR made LF. A text field draws a CR as nothing, so shown text holds none.
pub fn with_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_leading_mark_goes() {
        assert_eq!(without_bom("\u{feff}# A\n"), "# A\n");
        assert_eq!(without_bom("# A\u{feff}\n"), "# A\u{feff}\n");
        assert_eq!(without_bom(""), "");
    }

    #[test]
    fn every_line_ending_becomes_lf() {
        assert_eq!(with_lf("a\r\nb\rc\n\r\n"), "a\nb\nc\n\n");
        assert_eq!(with_lf("\r\r\n"), "\n\n");
    }
}
