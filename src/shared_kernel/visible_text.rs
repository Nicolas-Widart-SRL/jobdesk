/// The line without escape sequences, control characters other than tab, bidi marks, overrides and isolates,
/// zero-width characters or tag characters, so untrusted output cannot restyle, reorder or hide what the screen shows.
pub fn visible_chars(line: &str) -> impl Iterator<Item = char> + '_ {
    let mut chars = line.chars();
    std::iter::from_fn(move || loop {
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
    })
}

fn hidden(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{E0000}'..='\u{E007F}')
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
