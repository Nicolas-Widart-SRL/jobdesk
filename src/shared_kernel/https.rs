//! How Jobdesk reaches GitHub: the update feed and builds, and the community portal list. Each feature adds its own
//! timeouts and status handling to the one configuration here.

use std::io::{self, Read};
use std::time::Duration;

use ureq::Proxy;
use ureq::config::{Config, ConfigBuilder};
use ureq::tls::{RootCerts, TlsConfig};
use ureq::typestate::AgentScope;

/// Finding GitHub's address, then reaching it.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REDIRECTS: u32 = 5;

/// GitHub over https only: no plain http, even after a redirect, at most 5 redirects, the system's certificates,
/// and the proxy the environment names. It sends no cookie and no credential, only `Jobdesk/<version>` as its user
/// agent, so GitHub can tell the requests apart.
pub fn github_config(version: &str) -> ConfigBuilder<AgentScope> {
    Config::builder()
        .https_only(true)
        .max_redirects(MAX_REDIRECTS)
        .save_redirect_history(false)
        .user_agent(format!("Jobdesk/{version}"))
        .proxy(Proxy::try_from_env())
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .timeout_resolve(Some(RESOLVE_TIMEOUT))
        .timeout_connect(Some(CONNECT_TIMEOUT))
}

/// Turns a read that waited too long into `ErrorKind::TimedOut`, which ureq reports as an error of its own.
pub struct TimedBody<R>(pub R);

impl<R: Read> Read for TimedBody<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf).map_err(|error| {
            let timed_out = error.get_ref().and_then(|inner| inner.downcast_ref::<ureq::Error>()).is_some_and(|inner| matches!(inner, ureq::Error::Timeout(_)));
            if timed_out { io::Error::new(io::ErrorKind::TimedOut, "timed out") } else { error }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_http_address_is_never_requested() {
        let agent = github_config("0.1.0").build().new_agent();
        let refused = agent.get("http://127.0.0.1:9/").call().unwrap_err();
        assert!(matches!(refused, ureq::Error::RequireHttpsOnly(_)), "{refused:?}");
    }
}
