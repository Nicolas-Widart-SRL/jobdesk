/// The line without escape sequences, control characters other than tab, bidi marks, overrides and isolates,
/// zero-width, invisible, filler, line-separator or annotation characters or tag characters,
/// so untrusted output cannot restyle, reorder or hide what the screen shows.
pub fn visible_chars(line: &str) -> impl Iterator<Item = char> + '_ {
    let mut chars = line.chars();
    std::iter::from_fn(move || {
        loop {
            let c = chars.next()?;
            match c {
                '\u{1b}' => skip_escape(&mut chars),
                // The 8-bit forms of `ESC [`, `ESC ]` and `ESC P`, `ESC X`, `ESC ^`, `ESC _`.
                '\u{9b}' => skip_csi(&mut chars),
                '\u{9d}' => skip_string(&mut chars, true),
                '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => skip_string(&mut chars, false),
                _ if c.is_control() && c != '\t' || hidden(c) => {}
                _ => return Some(c),
            }
        }
    })
}

/// Whether every character of `text` shows: `visible_chars` keeps them all, and none is a variation selector. A line
/// break does not show.
pub fn all_visible(text: &str) -> bool {
    visible_chars(text).eq(text.chars()) && !text.chars().any(variation_selector)
}

/// Whether `c` is a variation selector. One passes `visible_chars`, because an emoji or a CJK character needs it, but
/// after a letter it shows nothing and can carry a payload.
pub fn variation_selector(c: char) -> bool {
    matches!(c, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}

fn hidden(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{115F}' | '\u{1160}' | '\u{17B4}' | '\u{17B5}' | '\u{180B}'..='\u{180F}' | '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}' | '\u{3164}' | '\u{FEFF}' | '\u{FFA0}' | '\u{FFF9}'..='\u{FFFB}' | '\u{1D173}'..='\u{1D17A}' | '\u{E0000}'..='\u{E007F}'
    )
}

/// Skips what follows an escape: a CSI sequence up to its final byte, a DCS, SOS, OSC, PM or APC string up to its terminator,
/// an intermediate byte and its final byte, or one final byte.
fn skip_escape(chars: &mut std::str::Chars) {
    match chars.next() {
        Some('[') => skip_csi(chars),
        Some('\u{20}'..='\u{2f}') => {
            chars.next();
        }
        Some(']') => skip_string(chars, true),
        Some('P' | 'X' | '^' | '_') => skip_string(chars, false),
        _ => {}
    }
}

/// Skips a control string's body up to and including its terminator: `ESC \` or U+009C, or BEL when `bel_ends`,
/// which only OSC allows. An unterminated string takes the rest of the line.
fn skip_string(chars: &mut std::str::Chars, bel_ends: bool) {
    let mut escaped = false;
    chars.find(|&c| {
        let end = c == '\u{9c}' || escaped && c == '\\' || bel_ends && c == '\u{7}';
        escaped = c == '\u{1b}';
        end
    });
}

/// Skips a CSI sequence's parameters up to and including its final byte.
fn skip_csi(chars: &mut std::str::Chars) {
    chars.find(|c| ('\u{40}'..='\u{7e}').contains(c));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(raw: &str) -> String {
        visible_chars(raw).collect()
    }

    #[test]
    fn hidden_characters_are_dropped() {
        let raw = "a\u{200B}b\u{200C}c\u{200D}d\u{200E}e\u{200F}f\u{061C}g\u{2060}h\u{FEFF}i\u{E0001}j\u{E0041}\u{E007F}k";
        assert_eq!(clean(raw), "abcdefghijk");
    }

    #[test]
    fn invisible_fillers_and_selectors_are_dropped() {
        let raw = "a\u{AD}b\u{34F}c\u{115F}d\u{1160}e\u{3164}f\u{FFA0}g\u{180E}h\u{2061}i\u{2062}j\u{2063}k\u{2064}l";
        assert_eq!(clean(raw), "abcdefghijkl");
    }

    #[test]
    fn deprecated_annotation_and_separator_characters_are_dropped() {
        let raw = "a\u{206A}b\u{206F}c\u{FFF9}d\u{FFFB}e\u{180B}f\u{180F}g\u{17B4}h\u{17B5}i\u{1D173}j\u{1D17A}k\u{2028}l\u{2029}m";
        assert_eq!(clean(raw), "abcdefghijklm");
    }

    #[test]
    fn variation_selectors_stay_for_emoji_and_cjk() {
        assert_eq!(clean("\u{2764}\u{FE0F} \u{8FBB}\u{E0100}"), "\u{2764}\u{FE0F} \u{8FBB}\u{E0100}");
    }

    #[test]
    fn short_and_eight_bit_escapes_are_dropped() {
        assert_eq!(clean("\u{1b}(Bplain\u{1b}7 text\u{1b}8 \u{9b}1;31mred\u{9b}0m end"), "plain text red end");
    }

    #[test]
    fn control_strings_are_dropped() {
        for introducer in ["\u{1b}P", "\u{1b}X", "\u{1b}]", "\u{1b}^", "\u{1b}_", "\u{90}", "\u{98}", "\u{9d}", "\u{9e}", "\u{9f}"] {
            for terminator in ["\u{1b}\\", "\u{9c}"] {
                assert_eq!(clean(&format!("before {introducer}payload{terminator}after")), "before after", "{introducer:?} {terminator:?}");
            }
        }
    }

    #[test]
    fn bel_ends_only_osc() {
        assert_eq!(clean("\u{1b}]title\u{7}after"), "after");
        assert_eq!(clean("\u{9d}title\u{7}after"), "after");
        assert_eq!(clean("\u{1b}Pa\u{7}b\u{1b}\\after"), "after");
        assert_eq!(clean("\u{9f}a\u{7}b\u{9c}after"), "after");
    }

    #[test]
    fn tabs_stay_and_other_controls_go() {
        assert_eq!(clean("a\tb\nc\rd"), "a\tbcd");
    }

    #[test]
    fn all_visible_refuses_what_shows_nothing() {
        assert!(all_visible("Côte d’Ivoire, 東京 \u{2764}"));
        for hidden in ["a\nb", "a\u{200B}b", "a\u{E0041}b", "a\u{FE0F}", "\u{8FBB}\u{E0100}", "\u{1b}[31ma"] {
            assert!(!all_visible(hidden), "{hidden:?}");
        }
    }
}
