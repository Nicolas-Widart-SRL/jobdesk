use std::sync::Arc;

/// An Anthropic API key. It has no `Clone` and no `Display`, and its `Debug` shows no text,
/// so a key cannot reach a log, a panic message or the screen by accident.
pub struct ApiKey(String);

impl ApiKey {
    /// The key without the whitespace around it. None for text that is empty or holds anything but visible ASCII inside:
    /// real keys are ASCII, and a space, a control, a zero-width or a bidi character could hide in a pasted key.
    pub fn parse(text: &str) -> Option<ApiKey> {
        let key = text.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_graphic()) {
            return None;
        }
        Some(ApiKey(key.to_owned()))
    }

    /// The key itself. Only the keychain and the process that needs it should call this.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(…)")
    }
}

/// Variables from the shell that started Jobdesk that make the `claude` CLI sign in some other way than with the saved key:
/// a bearer token, or the switch of another provider (Bedrock, Vertex, Foundry, Mantle, a gateway, Anthropic on AWS or Google Cloud).
/// A process that runs under the key removes these. `ANTHROPIC_BASE_URL` is left alone on purpose: it moves where requests go, not how they sign in,
/// and a user may set it for a proxy they trust.
pub const ENV_THAT_OUTRANKS_THE_KEY: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    "CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_USE_GATEWAY",
    "CLAUDE_CODE_USE_MANTLE",
    "CLAUDE_CODE_USE_VERTEX",
];

/// Reads the saved key when a task's process starts. `Ok(None)` is no key saved, and `Err` says why it could not be read.
pub type KeyReader = Arc<dyn Fn() -> Result<Option<ApiKey>, String> + Send + Sync>;

/// The variable that moves where the `claude` CLI sends its requests, and so the key.
pub const BASE_URL_VARIABLE: &str = "ANTHROPIC_BASE_URL";

/// Why a process that would get the key does not start. It names no address, since a URL can hold a password.
pub const PLAIN_HTTP_REFUSED: &str =
    "ANTHROPIC_BASE_URL is set to a plain http address, so Jobdesk will not send your API key there. Use https, or pick the Claude sign-in.";

/// Reads one variable of the environment Jobdesk started with. Tests pass their own, so none changes the process environment.
pub type EnvReader = dyn Fn(&str) -> Option<String>;

/// The process environment, as a process Jobdesk starts inherits it.
pub fn inherited_env(name: &str) -> Option<String> {
    std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
}

/// Whether `url` sends requests in the clear to a host other than this machine: an `http` scheme, in any case,
/// with a host that is not `localhost`, an address in `127.0.0.0/8` or `::1`. An `http` address with no host is flagged too.
/// Text with no scheme, garbage or an empty string is not flagged: the CLI rejects it itself and sends nothing.
/// It reads the address as a browser would: tabs and line breaks anywhere and spaces at the ends are dropped,
/// any run of slashes or backslashes may follow the scheme, and the host follows the last `@`. A loopback host
/// written some other way, such as `127.1` or `localhost.`, is flagged, so a doubt refuses the key rather than sends it.
pub fn plain_http_endpoint(url: &str) -> bool {
    let url: String = url.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
    let url = url.trim_matches(|c: char| c <= ' ');
    let Some((scheme, rest)) = url.split_once(':') else { return false };
    if !scheme.eq_ignore_ascii_case("http") {
        return false;
    }
    let rest = rest.trim_start_matches(['/', '\\']);
    let authority = rest.split(['/', '\\', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    !is_loopback(host)
}

/// Whether a URL's host, with its port, names this machine.
fn is_loopback(host: &str) -> bool {
    if let Some(inside) = host.strip_prefix('[') {
        let Some((address, port)) = inside.split_once(']') else { return false };
        return (port.is_empty() || port.starts_with(':')) && address.parse::<std::net::Ipv6Addr>().is_ok_and(|address| address.is_loopback());
    }
    let name = host.split_once(':').map_or(host, |(name, _)| name);
    name.eq_ignore_ascii_case("localhost") || name.parse::<std::net::Ipv4Addr>().is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";

    #[test]
    fn debug_shows_no_key_text() {
        let key = ApiKey::parse(FAKE).unwrap();
        assert_eq!(format!("{key:?}"), "ApiKey(…)");
        let wrapped = format!("{:?} {:#?}", Some(&key), Ok::<_, String>(&key));
        assert!(!wrapped.contains("sk-ant") && !wrapped.contains("R2kd"));
    }

    #[test]
    fn parse_trims_the_ends() {
        let key = ApiKey::parse(&format!("  \t{FAKE}\r\n")).unwrap();
        assert_eq!(key.expose(), FAKE);
    }

    #[test]
    fn parse_refuses_empty_text() {
        for text in ["", "   ", "\n\t "] {
            assert!(ApiKey::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn parse_refuses_whitespace_or_control_characters_inside() {
        for text in ["sk-ant- 1234", "sk-ant-\n1234", "sk-ant-\t1234", "sk-ant-\u{a0}1234", "sk-ant-\u{0}1234", "sk-ant-\u{7f}1234"] {
            assert!(ApiKey::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn parse_refuses_letters_outside_ascii() {
        for text in ["sk-ant-é1234", "sk-ant-日本1234", "sk-ant-1234😀"] {
            assert!(ApiKey::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn https_and_loopback_addresses_are_not_plain_http() {
        for url in [
            "https://api.anthropic.com",
            "HTTPS://Proxy.Example.com:8443/v1",
            "http://localhost",
            "http://LOCALHOST:4000/",
            "http://127.0.0.1:8080",
            "http://127.42.0.9/v1",
            "http://[::1]",
            "http://[::1]:8080/v1",
            "http://user:pw@127.0.0.1",
            "http://user@localhost:3000?x=1",
            "  http://localhost  ",
        ] {
            assert!(!plain_http_endpoint(url), "{url}");
        }
    }

    #[test]
    fn http_to_another_host_is_plain_http() {
        for url in [
            "http://example.com",
            "HTTP://Example.com",
            "hTtP://proxy.example.com:8080/v1",
            "http://localhost.evil.com",
            "http://127.0.0.1.evil.com",
            "http://127.0.0.1@evil.com",
            "http://localhost:80@evil.com/",
            "http://10.0.0.5:8080",
            "http://[2001:db8::1]",
            "http://[::1].evil.com",
            "http:evil.com",
            "http:\\\\evil.com",
            "http:///evil.com",
            " http://evil.com",
            "ht\ttp://evil.com",
            "http://evil.com\\@localhost",
            "http://",
        ] {
            assert!(plain_http_endpoint(url), "{url}");
        }
    }

    /// The CLI rejects these itself and sends nothing, so they are not this check's to refuse.
    #[test]
    fn no_scheme_or_garbage_is_not_flagged() {
        for url in ["", "   ", "example.com", "localhost:8080", "not a url", "ftp://example.com", "httpx://example.com"] {
            assert!(!plain_http_endpoint(url), "{url:?}");
        }
    }

    #[test]
    fn the_refusal_names_no_address() {
        assert!(!PLAIN_HTTP_REFUSED.contains("http://") && PLAIN_HTTP_REFUSED.contains("https"));
    }

    #[test]
    fn parse_refuses_invisible_characters_anywhere() {
        for hidden in ['\u{200B}', '\u{FEFF}', '\u{200E}', '\u{202E}', '\u{2066}', '\u{2060}', '\u{00AD}', '\u{FE0F}', '\u{E0100}', '\u{1BCA0}'] {
            for text in [format!("{FAKE}{hidden}"), format!("{hidden}{FAKE}"), format!("sk-ant-{hidden}api03-FAKE"), format!("{FAKE}{hidden}  ")] {
                assert!(ApiKey::parse(&text).is_none(), "{text:?}");
            }
        }
    }
}
