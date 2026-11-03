use std::path::{Path, PathBuf};

/// Tracker cells are free text, so a stored path may only point inside the workspace.
/// Both separators count on every system, so a `..\` path is refused on macOS too.
/// Refused on every system: an empty path, a leading `/` or `\` (which covers a UNC start `\\`),
/// a `..` part, and a part, the first included, that starts with an ASCII letter and a `:`
/// (`C:` or `C:x`). On Windows such a part replaces the path built so far.
/// Refused on Windows only: any `:` in a part, which names an alternate data stream
/// (`a.pdf:stream`). Elsewhere `:` is an ordinary character apart from the drive form above,
/// as in `acme:-labs.json` or a Finder-renamed `CV 10:2024`. A company `X: Labs` gives
/// `x:-labs.json`, which is refused on every system: an accepted trade-off.
/// Refused on Windows only: a part named like a device (`con.json`), which Windows opens as the
/// device, not as a file.
/// A path with no normal part resolves to the root itself and is refused, so a caller that
/// changes its extension never reaches a file beside the workspace.
pub fn within_workspace(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = relative.trim();
    if relative.is_empty() || relative.starts_with(['/', '\\']) {
        return None;
    }
    let mut path = root.to_path_buf();
    let mut normal = false;
    for part in relative.split(['/', '\\']) {
        let bytes = part.as_bytes();
        let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
        if drive || (cfg!(windows) && (part.contains(':') || is_device_name(part))) {
            return None;
        }
        match part {
            ".." => return None,
            "" | "." => {}
            part => {
                normal = true;
                path.push(part);
            }
        }
    }
    normal.then_some(path)
}

/// Whether Windows reads the part as a device (`CON`, `PRN`, `AUX`, `NUL`, `COM0-9`, `LPT0-9`, and the superscript
/// `COM¹-³`, `LPT¹-³`), whatever its extension or case: the stem before the first `.` decides.
fn is_device_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or_default().trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper.strip_prefix("COM").or_else(|| upper.strip_prefix("LPT")).is_some_and(|port| {
            let mut chars = port.chars();
            matches!((chars.next(), chars.next()), (Some('0'..='9' | '¹' | '²' | '³'), None))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_paths_cannot_leave_the_workspace() {
        let root = Path::new("/w");
        assert_eq!(within_workspace(root, "cv/main_a.tex"), Some(PathBuf::from("/w/cv/main_a.tex")));
        assert_eq!(within_workspace(root, "cv\\main_a.tex"), Some(PathBuf::from("/w/cv/main_a.tex")));
        assert_eq!(within_workspace(root, "cv/Résumé 2024 (final).tex"), Some(PathBuf::from("/w/cv/Résumé 2024 (final).tex")));
        for bad in [
            "../secret.pdf",
            "/etc/passwd",
            "cv/../../x",
            "  ",
            "..\\secret.pdf",
            "cv\\..\\..\\x",
            "C:x",
            "C:\\x",
            "\\\\host\\share\\x",
            "cv\\C:\\x",
            "cv/C:/x",
            "a/z:/x",
            ".",
            "./",
            ".\\",
            "",
            "./.",
            "company_research/x:-labs.json",
        ] {
            assert_eq!(within_workspace(root, bad), None, "{bad}");
        }
    }

    #[test]
    fn device_names_are_told_by_the_stem_in_any_case() {
        for device in
            ["con", "CON", "Con.json", "nul.tar.gz", "aux", "PRN.txt", "com1", "COM0.json", "lpt9", "Lpt3.x", "con .json", "COM¹.json", "LPT².txt", "com³"]
        {
            assert!(is_device_name(device), "{device}");
        }
        for plain in ["acme.json", "console.json", "com10", "com", "lpt", "com1x", "xcon", "", "a.con", "comma", "COM⁴", "COM¹¹"] {
            assert!(!is_device_name(plain), "{plain}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_device_name_is_refused_on_windows() {
        let root = Path::new("/w");
        for bad in
            ["company_research/con.json", "company_research\\NUL.json", "aux", "cv/Com1.tex", "company_research/COM¹.json", "company_research\\LPT².json"]
        {
            assert_eq!(within_workspace(root, bad), None, "{bad}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn a_colon_is_an_ordinary_character_off_windows() {
        let root = Path::new("/w");
        for ok in ["file.pdf:stream", "company_research/acme:-labs.json", "CV 10:2024"] {
            assert_eq!(within_workspace(root, ok), Some(root.join(ok)), "{ok}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn any_colon_is_refused_on_windows() {
        let root = Path::new("/w");
        for bad in ["file.pdf:stream", "company_research/acme:-labs.json", "CV 10:2024"] {
            assert_eq!(within_workspace(root, bad), None, "{bad}");
        }
    }
}
