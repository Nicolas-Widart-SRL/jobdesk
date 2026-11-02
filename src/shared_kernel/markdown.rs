use pulldown_cmark::Options;

/// The extensions every reader of Markdown here turns on, so the viewer and the run logic read a text the same way.
pub fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES | Options::ENABLE_DEFINITION_LIST
}

/// Whether `html` holds nothing but comments and white space, so a page can leave it out. A comment ends as a browser
/// ends it: at `-->` or `--!>`, and `<!-->` and `<!--->` are empty comments. An unclosed comment runs to the end.
pub fn only_comments(html: &str) -> bool {
    let mut rest = html;
    loop {
        let Some(start) = rest.find("<!--") else { return rest.trim().is_empty() };
        if !rest[..start].trim().is_empty() {
            return false;
        }
        let body = &rest[start + 4..];
        let closed = || [body.find("-->").map(|at| at + 3), body.find("--!>").map(|at| at + 4)].into_iter().flatten().min().map(|end| &body[end..]);
        match body.strip_prefix('>').or_else(|| body.strip_prefix("->")).or_else(closed) {
            Some(after) => rest = after,
            None => return true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::only_comments;

    #[test]
    fn only_text_inside_comments_counts_as_comments() {
        assert!(only_comments(" <!-- a -->\n<!-- b\n"));
        assert!(only_comments("  \n"));
        assert!(only_comments("<!---->\n<!-- a --!>"));
        assert!(!only_comments("<!-- a --> b"));
        assert!(!only_comments("<!-->Open to relocation: no [CITY_NAME]\n"));
        assert!(!only_comments("<!--->x"));
        assert!(!only_comments("<!-- a --!> Visible\n-->"));
        assert!(!only_comments("<div>\n<!-- a -->\n</div>"));
    }
}
