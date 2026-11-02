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

/// Why a process that would get the key does not start when a proxy would carry it in the clear. It names no value.
pub const PROXY_REFUSED: &str = "ANTHROPIC_BASE_URL is a plain http address on this machine, and HTTP_PROXY or ALL_PROXY is set, so your API key could go through that proxy in the clear. Jobdesk will not send it. Use https, unset the proxy variable, or pick the Claude sign-in.";

/// Variables that can send a plain http request through a proxy, even one to this machine.
pub const PROXY_VARIABLES: &[&str] = &["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"];

/// Reads one variable of the environment Jobdesk started with. Tests pass their own, so none changes the process environment.
pub type EnvReader = dyn Fn(&str) -> Option<String>;

/// The process environment, as a process Jobdesk starts inherits it.
pub fn inherited_env(name: &str) -> Option<String> {
    std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
}

/// Why the key must not go to a process started under `env`, or None when its requests stay encrypted or on this machine.
/// No base URL, or a blank one, leaves the CLI on its own `https` address. An `http` base URL on this machine is refused
/// when a proxy variable is set and not empty, whatever `NO_PROXY` says.
pub fn key_route_refusal(env: &EnvReader) -> Option<&'static str> {
    let url = env(BASE_URL_VARIABLE)?;
    match base_url_kind(&url) {
        BaseUrl::Blank | BaseUrl::Https => None,
        BaseUrl::Other => Some(PLAIN_HTTP_REFUSED),
        BaseUrl::LocalHttp => PROXY_VARIABLES.iter().any(|name| env(name).is_some_and(|value| !value.is_empty())).then_some(PROXY_REFUSED),
    }
}

/// What a base URL is, as far as the key is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaseUrl {
    /// Nothing but blanks.
    Blank,
    Https,
    /// `http` to `localhost`, an address in `127.0.0.0/8` or `::1`.
    LocalHttp,
    /// Anything else: `http` to another host, another scheme, or text that is no URL. A doubt refuses the key rather than sends it.
    Other,
}

/// Reads the address as a browser would: tabs and line breaks anywhere, and blanks, byte order marks and controls at the ends,
/// are dropped, the scheme is in any case, any run of slashes or backslashes may follow it, and the host follows the last `@`.
/// A loopback host written some other way, such as `127.1` or `localhost.`, is `Other`.
fn base_url_kind(url: &str) -> BaseUrl {
    let url: String = url.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
    let url = url.trim_matches(|c: char| c.is_whitespace() || c.is_control() || c == '\u{FEFF}');
    if url.is_empty() {
        return BaseUrl::Blank;
    }
    let Some((scheme, rest)) = url.split_once(':') else { return BaseUrl::Other };
    if scheme.eq_ignore_ascii_case("https") {
        return BaseUrl::Https;
    }
    if !scheme.eq_ignore_ascii_case("http") {
        return BaseUrl::Other;
    }
    let rest = rest.trim_start_matches(['/', '\\']);
    let authority = rest.split(['/', '\\', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    if is_loopback(host) { BaseUrl::LocalHttp } else { BaseUrl::Other }
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
    fn https_is_https() {
        for url in ["https://api.anthropic.com", "HTTPS://Proxy.Example.com:8443/v1", "  https://proxy.example.com\n", "\u{a0}https://proxy.example.com"] {
            assert_eq!(base_url_kind(url), BaseUrl::Https, "{url:?}");
        }
    }

    #[test]
    fn http_to_this_machine_is_local() {
        for url in [
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
            assert_eq!(base_url_kind(url), BaseUrl::LocalHttp, "{url:?}");
        }
    }

    #[test]
    fn http_to_another_host_is_refused() {
        for url in [
            "http://example.com",
            "HTTP://Example.com",
            "hTtP://proxy.example.com:8080/v1",
            "http://localhost.evil.com",
            "http://127.0.0.1.evil.com",
            "http://127.0.0.1@evil.com",
            "http://localhost:80@evil.com/",
            "http://10.0.0.5:8080",
            "http://127.1",
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
            assert_eq!(base_url_kind(url), BaseUrl::Other, "{url:?}");
        }
    }

    /// JavaScript's `trim` drops these too, so each could hide the scheme from a narrower trim.
    #[test]
    fn a_blank_or_mark_before_the_scheme_hides_nothing() {
        for blank in ['\u{a0}', '\u{feff}', '\u{2028}', '\u{2029}', '\u{3000}', '\u{1680}', '\u{85}', '\u{b}', '\u{c}'] {
            assert_eq!(base_url_kind(&format!("{blank}http://example.com{blank}")), BaseUrl::Other, "{blank:?}");
            assert_eq!(base_url_kind(&format!("{blank}http://localhost{blank}")), BaseUrl::LocalHttp, "{blank:?}");
        }
    }

    #[test]
    fn anything_but_https_or_local_http_is_refused() {
        for url in ["proxy.corp:8080", "example.com", "localhost:8080", "not a url", "ftp://x", "httpx://example.com", "\u{200b}https://example.com"] {
            assert_eq!(base_url_kind(url), BaseUrl::Other, "{url:?}");
        }
        for url in ["", "   ", "\u{a0}\u{feff}\n"] {
            assert_eq!(base_url_kind(url), BaseUrl::Blank, "{url:?}");
        }
    }

    fn env_of(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| vars.iter().find(|(var, _)| *var == name).map(|(_, value)| (*value).to_owned())
    }

    #[test]
    fn the_key_route_is_refused_only_where_it_could_go_in_the_clear() {
        assert_eq!(key_route_refusal(&env_of(&[])), None, "no base URL");
        assert_eq!(key_route_refusal(&env_of(&[("ANTHROPIC_BASE_URL", "")])), None, "an empty one");
        assert_eq!(key_route_refusal(&env_of(&[("ANTHROPIC_BASE_URL", "https://proxy.example.com"), ("HTTP_PROXY", "http://p.example.com")])), None);
        assert_eq!(key_route_refusal(&env_of(&[("ANTHROPIC_BASE_URL", "http://127.0.0.1:8080")])), None);
        assert_eq!(key_route_refusal(&env_of(&[("ANTHROPIC_BASE_URL", "http://example.com")])), Some(PLAIN_HTTP_REFUSED));
        assert_eq!(key_route_refusal(&env_of(&[("ANTHROPIC_BASE_URL", "proxy.corp:8080")])), Some(PLAIN_HTTP_REFUSED));
        assert_eq!(key_route_refusal(&env_of(&[("HTTP_PROXY", "http://p.example.com")])), None, "a proxy alone keeps https encrypted");
    }

    #[test]
    fn a_proxy_variable_refuses_a_local_http_base_url() {
        for name in ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
            assert!(PROXY_VARIABLES.contains(&name), "{name}");
        }
        for name in PROXY_VARIABLES {
            let local = |var: &str| (var == BASE_URL_VARIABLE).then(|| "http://localhost:4000".to_owned());
            let set = move |var: &str| if var == *name { Some("http://p.example.com:3128".to_owned()) } else { local(var) };
            assert_eq!(key_route_refusal(&set), Some(PROXY_REFUSED), "{name}");
            let empty = move |var: &str| if var == *name { Some(String::new()) } else { local(var) };
            assert_eq!(key_route_refusal(&empty), None, "an empty {name}");
        }
    }

    #[test]
    fn the_refusals_name_no_address() {
        for notice in [PLAIN_HTTP_REFUSED, PROXY_REFUSED] {
            assert!(!notice.contains("http://") && notice.contains("https"), "{notice}");
        }
    }

    #[test]
    fn the_inherited_environment_is_the_process_environment() {
        assert_eq!(inherited_env("PATH"), std::env::var_os("PATH").map(|path| path.to_string_lossy().into_owned()));
        assert!(inherited_env("PATH").is_some());
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
