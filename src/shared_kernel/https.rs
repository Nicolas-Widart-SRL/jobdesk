//! The one way Jobdesk reads a file from the network: https only, with fixed redirect, proxy and timeout rules.

use std::io::{self, Read};
use std::time::Duration;

use ureq::config::Config;
use ureq::tls::{RootCerts, TlsConfig};
use ureq::{Agent, Proxy};

/// Finding the server's address, then reaching it.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The whole feed request, redirects and body included, from the first byte sent to the last one read.
const FEED_TIMEOUT: Duration = Duration::from_secs(20);
/// The longest a download may wait for its next bytes.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// The whole of one download. A build of the largest size still arrives in time at about 60 KB/s.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const MAX_REDIRECTS: u32 = 5;

/// What a request is for, which sets how long it may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetch {
    /// A small file read whole, such as a feed: the whole request must end within 20 seconds.
    Feed,
    /// A large file read as it arrives: it fails when its next bytes take over 30 seconds, or after an hour.
    Download,
}

/// Why a request did not get a whole answer. The caller says each in its own words, and the server's text is never kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpsFailure {
    TimedOut,
    Unreachable,
    /// The server answered with this status code.
    Refused(u16),
    /// The body stopped partway, for a reason other than time.
    Interrupted,
}

/// The body of an answer, read as it arrives. A read that waited too long fails with `ErrorKind::TimedOut`.
pub type Body = Box<dyn Read + Send>;

/// Opens an address and hands back its body. The real one is `HttpsTransport`, and tests put a fake in its place.
pub trait Transport: Send + Sync {
    /// The body of `url`, or why it could not be reached. A status other than success is `Refused`.
    fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure>;
}

/// https only: no plain http, even after a redirect, at most 5 redirects, the system's certificates,
/// and the proxy the environment names. It sends no cookie and no credential.
pub struct HttpsTransport {
    feed: Agent,
    download: Agent,
}

impl HttpsTransport {
    /// `current` is the version of this copy, sent as `Jobdesk/<version>` so a server can tell the requests apart.
    pub fn new(current: &str) -> Self {
        let agent = |fetch: Fetch| {
            let builder = Config::builder()
                .https_only(true)
                .max_redirects(MAX_REDIRECTS)
                .save_redirect_history(false)
                .user_agent(format!("Jobdesk/{current}"))
                .proxy(Proxy::try_from_env())
                .http_status_as_error(true)
                .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
                .timeout_resolve(Some(RESOLVE_TIMEOUT))
                .timeout_connect(Some(CONNECT_TIMEOUT));
            let builder = match fetch {
                Fetch::Feed => builder.timeout_global(Some(FEED_TIMEOUT)),
                Fetch::Download => builder.timeout_global(Some(DOWNLOAD_TIMEOUT)).timeout_per_read(Some(READ_TIMEOUT)),
            };
            builder.build().new_agent()
        };
        Self { feed: agent(Fetch::Feed), download: agent(Fetch::Download) }
    }
}

impl Transport for HttpsTransport {
    fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure> {
        let agent = match fetch {
            Fetch::Feed => &self.feed,
            Fetch::Download => &self.download,
        };
        let response = agent.get(url).call().map_err(|error| failure_of(&error))?;
        Ok(Box::new(TimedBody(response.into_body().into_reader())))
    }
}

/// Why a request did not get an answer. The server's text is never kept.
fn failure_of(error: &ureq::Error) -> HttpsFailure {
    match error {
        ureq::Error::StatusCode(code) => HttpsFailure::Refused(*code),
        ureq::Error::Timeout(_) => HttpsFailure::TimedOut,
        ureq::Error::Io(error) if error.kind() == io::ErrorKind::TimedOut => HttpsFailure::TimedOut,
        _ => HttpsFailure::Unreachable,
    }
}

/// Turns a read that waited too long into `ErrorKind::TimedOut`, which ureq reports as an error of its own.
struct TimedBody<R>(R);

impl<R: Read> Read for TimedBody<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf).map_err(|error| {
            let timed_out = error.get_ref().and_then(|inner| inner.downcast_ref::<ureq::Error>()).is_some_and(|inner| matches!(inner, ureq::Error::Timeout(_)));
            if timed_out { io::Error::new(io::ErrorKind::TimedOut, "timed out") } else { error }
        })
    }
}

/// The failure of a read that failed partway.
pub fn read_failure(error: &io::Error) -> HttpsFailure {
    if error.kind() == io::ErrorKind::TimedOut { HttpsFailure::TimedOut } else { HttpsFailure::Interrupted }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_is_refused_with_its_code_and_nothing_else_is_kept() {
        assert_eq!(failure_of(&ureq::Error::StatusCode(404)), HttpsFailure::Refused(404));
        assert_eq!(failure_of(&ureq::Error::HostNotFound), HttpsFailure::Unreachable);
        assert_eq!(failure_of(&ureq::Error::ConnectionFailed), HttpsFailure::Unreachable);
        assert_eq!(failure_of(&ureq::Error::Io(io::Error::new(io::ErrorKind::TimedOut, "slow"))), HttpsFailure::TimedOut);
        assert_eq!(failure_of(&ureq::Error::RequireHttpsOnly("http://example.com".into())), HttpsFailure::Unreachable);
    }

    #[test]
    fn a_read_that_waited_too_long_reads_as_timed_out() {
        struct Slow;
        impl Read for Slow {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(ureq::Error::Timeout(ureq::Timeout::PerRead).into_io())
            }
        }
        let error = TimedBody(Slow).read(&mut [0; 4]).unwrap_err();
        assert_eq!(read_failure(&error), HttpsFailure::TimedOut);
        assert_eq!(read_failure(&io::Error::new(io::ErrorKind::ConnectionReset, "reset")), HttpsFailure::Interrupted);
    }

    #[test]
    fn a_plain_http_address_is_never_requested() {
        let transport = HttpsTransport::new("0.1.0");
        for agent in [&transport.feed, &transport.download] {
            let refused = agent.get("http://127.0.0.1:9/latest.json").call().unwrap_err();
            assert!(matches!(refused, ureq::Error::RequireHttpsOnly(_)), "{refused:?}");
        }
        assert_eq!(transport.open("http://127.0.0.1:9/latest.json", Fetch::Feed).err(), Some(HttpsFailure::Unreachable));
    }
}
