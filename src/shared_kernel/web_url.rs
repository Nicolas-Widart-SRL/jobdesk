/// A link that is safe to hand to the browser: `http` or `https` only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebUrl(String);

impl WebUrl {
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        let lower = raw.to_ascii_lowercase();
        let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"))?;
        let has_host = rest.chars().next().is_some_and(|c| c != '/');
        (has_host && !raw.chars().any(char::is_control)).then(|| Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::WebUrl;

    #[test]
    fn accepts_only_http_and_https() {
        assert!(WebUrl::parse("https://example.org/jobs/1").is_some());
        assert!(WebUrl::parse(" HTTP://example.org ").is_some());
        for bad in ["file:///etc/passwd", "javascript:alert(1)", "https://", "", "ftp://x"] {
            assert!(WebUrl::parse(bad).is_none(), "{bad}");
        }
    }
}
