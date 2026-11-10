use crate::shared_kernel::WebUrl;
use crate::shared_kernel::visible_text::all_visible;

/// The command that has Claude build a portal.
pub const ADD_PORTAL: &str = "/add-portal";

/// A field of the Add a portal form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddField {
    Website,
    Country,
    Language,
    TestTitle,
}

/// Why the form makes no launcher line yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddError {
    /// The website is empty.
    NoWebsite,
    /// The website is not an `http` or `https` address with a dotted host.
    NotWebAddress,
    /// The website holds a space or a line break, which no web address does. On the line, what follows one would read
    /// as more words for `/add-portal`, such as an option.
    SpaceInWebsite,
    /// The field holds characters that show nothing, which the line would carry to Claude unseen.
    Hidden(AddField),
}

/// The launcher line for a board at `website`, such as
/// `/add-portal https://jobs.sample-board.example country: Belgium, language: French, test title: Lead Backend Engineer`.
/// A website without a scheme at its start gets `https://`, and a website holding a space or a line break is refused.
/// Each other field is trimmed and its line breaks and other spaces collapse to one space, so the line stays one line. An empty optional field stays off the line, because `/add-portal` then asks for
/// it in the task.
pub fn add_portal_line(website: &str, country: &str, language: &str, title: &str) -> Result<String, AddError> {
    // A tab or a line break reads as the space it shows as, so it is refused as one.
    let website = spaced(website);
    if !all_visible(&website) {
        return Err(AddError::Hidden(AddField::Website));
    }
    if website.trim().is_empty() {
        return Err(AddError::NoWebsite);
    }
    let website = web_address(website.trim()).ok_or(AddError::NotWebAddress)?;
    if website.contains(char::is_whitespace) {
        return Err(AddError::SpaceInWebsite);
    }
    let mut answers = Vec::new();
    for (key, text, which) in [("country", country, AddField::Country), ("language", language, AddField::Language), ("test title", title, AddField::TestTitle)]
    {
        let value = field(text, which)?;
        if !value.is_empty() {
            answers.push(format!("{key}: {value}"));
        }
    }
    let mut line = format!("{ADD_PORTAL} {website}");
    if !answers.is_empty() {
        line.push(' ');
        line.push_str(&answers.join(", "));
    }
    Ok(line)
}

/// `text` trimmed, with every run of spaces and line breaks as one space. A character that shows nothing refuses the
/// field: the launcher would refuse the line, or Claude would read what the user never saw.
fn field(text: &str, which: AddField) -> Result<String, AddError> {
    let spaced = spaced(text);
    if !all_visible(&spaced) {
        return Err(AddError::Hidden(which));
    }
    Ok(spaced.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// `text` with every space and line break as a plain space, so only characters that show nothing fail `all_visible`.
fn spaced(text: &str) -> String {
    text.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).collect()
}

/// Whether `website` starts with a scheme, such as `https://`: letters first, then letters, digits, `+`, `-` or `.`,
/// then `://`. One later in the address, such as in its query, is not its scheme.
fn has_scheme(website: &str) -> bool {
    website.split_once("://").is_some_and(|(scheme, _)| {
        scheme.starts_with(|c: char| c.is_ascii_alphabetic()) && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// The website as an `http` or `https` address, with `https://` put first when it names no scheme. Its host must hold a
/// dot between two labels of letters, digits and `-`, with an optional port, so a word or a sentence is no address.
fn web_address(website: &str) -> Option<String> {
    let address = if has_scheme(website) { website.to_owned() } else { format!("https://{website}") };
    let url = WebUrl::parse(&address)?;
    let rest = &url.as_str()[url.as_str().find("://")? + 3..];
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        Some(_) => return None,
        None => authority,
    };
    let label = |label: &str| !label.is_empty() && label.chars().all(|c| c.is_alphanumeric() || c == '-');
    let labels: Vec<&str> = host.split('.').collect();
    (labels.len() >= 2 && labels.iter().all(|part| label(part))).then(|| url.as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITE: &str = "https://jobs.sample-board.example";

    #[test]
    fn every_answer_goes_on_the_line_in_order() {
        let line = add_portal_line(SITE, "Belgium", "French", "Lead Backend Engineer").unwrap();
        assert_eq!(line, "/add-portal https://jobs.sample-board.example country: Belgium, language: French, test title: Lead Backend Engineer");
    }

    #[test]
    fn an_empty_field_stays_off_the_line() {
        assert_eq!(add_portal_line(SITE, "Belgium", "", "  ").unwrap(), "/add-portal https://jobs.sample-board.example country: Belgium");
        assert_eq!(
            add_portal_line(SITE, "", "", "Lead Backend Engineer").unwrap(),
            "/add-portal https://jobs.sample-board.example test title: Lead Backend Engineer"
        );
        assert_eq!(add_portal_line(SITE, "", "", "").unwrap(), "/add-portal https://jobs.sample-board.example");
    }

    #[test]
    fn a_website_without_a_scheme_gets_https() {
        assert_eq!(add_portal_line(" jobs.sample-board.example/listings ", "", "", "").unwrap(), "/add-portal https://jobs.sample-board.example/listings");
        assert_eq!(add_portal_line("http://jobs.sample-board.example:8080", "", "", "").unwrap(), "/add-portal http://jobs.sample-board.example:8080");
        assert_eq!(add_portal_line("HTTPS://Jobs.Sample-Board.example", "", "", "").unwrap(), "/add-portal HTTPS://Jobs.Sample-Board.example");
    }

    /// A scheme counts only at the start, so an address that holds another one in its query still gets `https://`.
    #[test]
    fn a_scheme_counts_only_at_the_start() {
        let line = add_portal_line("jobs.example.be/jobs?redirect=https://jobs.example.be/results", "", "", "").unwrap();
        assert_eq!(line, "/add-portal https://jobs.example.be/jobs?redirect=https://jobs.example.be/results");
        assert_eq!(add_portal_line("jobs.example.be/go?to=ftp://x", "", "", ""), Ok("/add-portal https://jobs.example.be/go?to=ftp://x".to_owned()));
        assert_eq!(add_portal_line("ftp://jobs.example.be/?to=https://jobs.example.be", "", "", ""), Err(AddError::NotWebAddress));
    }

    /// A web address holds no space, so nothing after one can reach `/add-portal` as an option such as `--list`.
    #[test]
    fn a_website_with_a_space_is_refused() {
        for spaced in ["jobs.example.be/x --list", "https://jobs.example.be/a b", "jobs.example.be/?q=a\tb", "jobs.example.be/\nx"] {
            assert_eq!(add_portal_line(spaced, "", "", ""), Err(AddError::SpaceInWebsite), "{spaced:?}");
        }
        assert_eq!(add_portal_line("  jobs.example.be/x  ", "", "", ""), Ok("/add-portal https://jobs.example.be/x".to_owned()), "only the ends are trimmed");
        assert_eq!(add_portal_line("jobs sample board", "", "", ""), Err(AddError::NotWebAddress), "no host is no web address");
    }

    #[test]
    fn a_website_must_be_a_web_address() {
        assert_eq!(add_portal_line("", "Belgium", "", ""), Err(AddError::NoWebsite));
        assert_eq!(add_portal_line(" \n ", "", "", ""), Err(AddError::NoWebsite));
        for bad in [
            "jobs sample board",
            "localhost",
            "ftp://jobs.sample-board.example",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://",
            "https://.example",
            "https://jobs..example",
            "jobs.example.",
            "https://jobs.example:port",
            "https://user@jobs.example",
        ] {
            assert_eq!(add_portal_line(bad, "", "", ""), Err(AddError::NotWebAddress), "{bad}");
        }
    }

    #[test]
    fn line_breaks_and_runs_of_space_collapse() {
        let line = add_portal_line(SITE, " Belgium\n", "French\r\n  Dutch", "Lead\tBackend\u{2028}Engineer").unwrap();
        assert_eq!(line, "/add-portal https://jobs.sample-board.example country: Belgium, language: French Dutch, test title: Lead Backend Engineer");
        assert!(!line.contains(['\n', '\r', '\t']));
    }

    #[test]
    fn a_field_with_hidden_characters_is_refused() {
        for (country, language, title, field) in [
            ("Bel\u{200B}gium", "", "", AddField::Country),
            ("", "French\u{202E}", "", AddField::Language),
            ("", "", "Lead\u{E0041}Engineer", AddField::TestTitle),
            ("", "", "Lead\u{FE0F} Engineer", AddField::TestTitle),
            ("\u{1b}[31mBelgium", "", "", AddField::Country),
        ] {
            assert_eq!(add_portal_line(SITE, country, language, title), Err(AddError::Hidden(field)), "{country:?} {language:?} {title:?}");
        }
        assert_eq!(add_portal_line("jobs.sample\u{200B}-board.example", "", "", ""), Err(AddError::Hidden(AddField::Website)));
    }

    /// Every line the form makes opens the launcher on `/add-portal` with the line as it is.
    #[test]
    fn the_line_opens_the_launcher() {
        for line in [
            add_portal_line(SITE, "Belgium", "French", "Lead Backend Engineer").unwrap(),
            add_portal_line("jobs.sample-board.example", "Belgium", "", "").unwrap(),
            add_portal_line(SITE, "Côte d’Ivoire", "Français", "Développeur «senior»").unwrap(),
        ] {
            let mut launcher = crate::runs::ui::Launcher::default();
            assert!(launcher.prefill(&line), "{line}");
            assert_eq!(launcher.words(), None, "a command, not a task in plain words");
            assert_eq!(launcher.line(), line);
        }
    }
}
