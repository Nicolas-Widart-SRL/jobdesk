use crate::shared_kernel::visible_text::visible_chars;

/// A notice shows at most this many lines. The last ones are kept, since a stderr tail ends on its error.
pub const NOTICE_LINES: usize = 8;

/// A notice drawn as plain text: the lines the story shows, and the whole of it for the hover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeText {
    /// The last `NOTICE_LINES` lines that hold something.
    pub shown: Vec<String>,
    /// Every line that holds something, one per row.
    pub full: String,
}

/// The first line of untrusted text, trimmed, without the characters that could hide or reorder what it says.
pub fn first_line(text: &str) -> String {
    visible(lines(text).next().unwrap_or_default())
}

/// The first line of untrusted text that holds something, cleaned as `first_line` cleans one.
/// A turn's result text opens with what went wrong.
pub fn first_filled_line(text: &str) -> String {
    lines(text).map(visible).find(|line| !line.is_empty()).unwrap_or_default()
}

/// The last line of untrusted text that holds something, cleaned as `first_line` cleans one.
/// A failure reason ends on its error, so this is the line that says what went wrong.
pub fn last_line(text: &str) -> String {
    lines(text).rev().map(visible).find(|line| !line.is_empty()).unwrap_or_default()
}

/// The command word of a command line, such as `/apply`, cleaned as `first_line` cleans a line.
pub fn command_word(line: &str) -> String {
    first_line(line.split_whitespace().next().unwrap_or_default())
}

/// The name a run goes by on screen: its command word, or "Task" for a task in plain words, whose first word names nothing.
pub fn run_name(line: &str) -> String {
    let word = command_word(line);
    if word.starts_with('/') { word } else { "Task".to_owned() }
}

/// Every line of a notice that holds something, each cleaned, with the last `NOTICE_LINES` of them shown.
pub fn notice_text(text: &str) -> NoticeText {
    let lines: Vec<String> = lines(text).map(visible).filter(|line| !line.is_empty()).collect();
    let shown = lines[lines.len().saturating_sub(NOTICE_LINES)..].to_vec();
    NoticeText { shown, full: lines.join("\n") }
}

/// The lines of untrusted text. A lone carriage return ends a line too, so a progress bar cannot run into the error after it.
fn lines(text: &str) -> impl DoubleEndedIterator<Item = &str> {
    text.split(['\r', '\n'])
}

fn visible(line: &str) -> String {
    visible_chars(line).collect::<String>().trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STDERR: &str = "Downloading 10%\nDownloading 55%\n\nDownloading 100%\n   \nerror: auth expired\u{202e}\n";

    #[test]
    fn a_notice_shows_every_line_that_holds_something() {
        let notice = notice_text(STDERR);
        assert_eq!(notice.shown, ["Downloading 10%", "Downloading 55%", "Downloading 100%", "error: auth expired"]);
        assert_eq!(notice.full, "Downloading 10%\nDownloading 55%\nDownloading 100%\nerror: auth expired");
    }

    #[test]
    fn a_long_notice_keeps_its_last_lines() {
        let text = (1..=12).map(|at| format!("Downloading {at}0%")).chain(["error: auth expired".to_owned()]).collect::<Vec<_>>().join("\n");
        let notice = notice_text(&text);
        assert_eq!(notice.shown.len(), NOTICE_LINES);
        assert_eq!(notice.shown.first().map(String::as_str), Some("Downloading 60%"));
        assert_eq!(notice.shown.last().map(String::as_str), Some("error: auth expired"));
        assert_eq!(notice.full.lines().count(), 13);
    }

    #[test]
    fn the_last_line_is_the_error() {
        assert_eq!(last_line(STDERR), "error: auth expired");
        assert_eq!(last_line("\n \u{200b}\n"), "");
    }

    #[test]
    fn the_first_filled_line_skips_blank_ones() {
        assert_eq!(first_filled_line("\n  \nAPI Error: 401 invalid x-api-key\n\nPlease run /login"), "API Error: 401 invalid x-api-key");
        assert_eq!(first_filled_line(" \u{200b}\n"), "");
    }

    #[test]
    fn a_carriage_return_ends_a_line() {
        let text = "Downloading 10%\rDownloading 55%\rerror: auth expired\r\n";
        assert_eq!(notice_text(text).shown, ["Downloading 10%", "Downloading 55%", "error: auth expired"]);
        assert_eq!(last_line(text), "error: auth expired");
        assert_eq!(first_filled_line("\r\rAPI Error: 401\rmore"), "API Error: 401");
        assert_eq!(first_line("/apply x\rignore"), "/apply x");
    }

    #[test]
    fn first_line_and_command_word_are_clean() {
        assert_eq!(first_line("  /apply\u{202e} x \nignore"), "/apply x");
        assert_eq!(command_word("\u{200b}/apply https://x.example\nmore"), "/apply");
        assert_eq!(command_word(""), "");
    }

    #[test]
    fn a_run_is_named_by_its_command_or_as_a_task() {
        assert_eq!(run_name("\u{200b}/apply https://x.example"), "/apply");
        assert_eq!(run_name("Copy the portal. Read its code."), "Task");
        assert_eq!(run_name(""), "Task");
    }
}
