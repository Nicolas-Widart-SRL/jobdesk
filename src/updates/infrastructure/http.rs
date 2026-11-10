use std::io::{self, Read};
use std::time::Duration;

use ureq::Agent;

use crate::shared_kernel::https::{TimedBody, github_config};
use crate::updates::domain::Failure;

/// The whole feed request, redirects and body included, from the first byte sent to the last one read.
const FEED_TIMEOUT: Duration = Duration::from_secs(20);
/// The longest a download may wait for its next bytes.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// The whole of one download. A build of the largest size still arrives in time at about 60 KB/s.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// What a request is for, which sets how long it may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetch {
    Feed,
    Download,
}

/// The body of an answer, read as it arrives. A read that waited too long fails with `ErrorKind::TimedOut`.
pub type Body = Box<dyn Read + Send>;

/// Opens an address and hands back its body. The real one is GitHub over https, and tests put a fake in its place.
pub trait Transport: Send + Sync {
    /// The body of `url`, or why it could not be reached. A status other than success is `Refused`.
    fn open(&self, url: &str, fetch: Fetch) -> Result<Body, Failure>;
}

/// GitHub as `github_config` reaches it: https only, at most 5 redirects, the system's certificates and the
/// environment's proxy, with no cookie and no credential. Any status but success is an error.
pub struct HttpsTransport {
    feed: Agent,
    download: Agent,
}

impl HttpsTransport {
    /// `current` is the version of this copy, sent as `Jobdesk/<version>` so GitHub can tell the requests apart.
    pub fn new(current: &str) -> Self {
        let agent = |fetch: Fetch| {
            let builder = github_config(current).http_status_as_error(true);
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
    fn open(&self, url: &str, fetch: Fetch) -> Result<Body, Failure> {
        let agent = match fetch {
            Fetch::Feed => &self.feed,
            Fetch::Download => &self.download,
        };
        let response = agent.get(url).call().map_err(|error| failure_of(&error))?;
        Ok(Box::new(TimedBody(response.into_body().into_reader())))
    }
}

/// Jobdesk's own word for why a request did not get an answer. The server's text is never kept.
fn failure_of(error: &ureq::Error) -> Failure {
    match error {
        ureq::Error::StatusCode(code) => Failure::Refused(*code),
        ureq::Error::Timeout(_) => Failure::TimedOut,
        ureq::Error::Io(error) if error.kind() == io::ErrorKind::TimedOut => Failure::TimedOut,
        _ => Failure::Unreachable,
    }
}

/// Jobdesk's word for a read that failed partway.
pub fn read_failure(error: &io::Error) -> Failure {
    if error.kind() == io::ErrorKind::TimedOut { Failure::TimedOut } else { Failure::Interrupted }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_is_refused_with_its_code_and_nothing_else_is_kept() {
        assert_eq!(failure_of(&ureq::Error::StatusCode(404)), Failure::Refused(404));
        assert_eq!(failure_of(&ureq::Error::HostNotFound), Failure::Unreachable);
        assert_eq!(failure_of(&ureq::Error::ConnectionFailed), Failure::Unreachable);
        assert_eq!(failure_of(&ureq::Error::Io(io::Error::new(io::ErrorKind::TimedOut, "slow"))), Failure::TimedOut);
        assert_eq!(failure_of(&ureq::Error::RequireHttpsOnly("http://example.com".into())), Failure::Unreachable);
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
        assert_eq!(read_failure(&error), Failure::TimedOut);
        assert_eq!(read_failure(&io::Error::new(io::ErrorKind::ConnectionReset, "reset")), Failure::Interrupted);
    }

    #[test]
    fn a_plain_http_address_is_never_requested() {
        let transport = HttpsTransport::new("0.1.0");
        for agent in [&transport.feed, &transport.download] {
            let refused = agent.get("http://127.0.0.1:9/latest.json").call().unwrap_err();
            assert!(matches!(refused, ureq::Error::RequireHttpsOnly(_)), "{refused:?}");
        }
        assert_eq!(transport.open("http://127.0.0.1:9/latest.json", Fetch::Feed).err(), Some(Failure::Unreachable));
    }
}
