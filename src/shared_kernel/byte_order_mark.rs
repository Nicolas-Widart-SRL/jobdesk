/// The byte order mark Windows editors such as Notepad may write at the start of a UTF-8 file.
pub const BOM: char = '\u{feff}';

/// `text` without a byte order mark at its start, so a heading or front matter on line 1 still reads as one.
pub fn without_bom(text: &str) -> &str {
    text.strip_prefix(BOM).unwrap_or(text)
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
}
