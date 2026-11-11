use std::collections::VecDeque;

use crate::updates::domain::{DownloadId, Release, ServiceEvent, UpdateService};

/// A service that never answers: a check or a download started with it stays running. For previews and snapshots.
#[derive(Default)]
pub struct IdleUpdates;

impl UpdateService for IdleUpdates {
    fn check(&mut self) {}

    fn download(&mut self, _release: &Release, _id: DownloadId) {}

    fn cancel(&mut self) {}

    fn poll(&mut self) -> Option<ServiceEvent> {
        None
    }
}

/// A service that answers from a queue at once: a check gets the next `Checked`, and a download gets the progress and
/// the end that follow, stamped with the number of that download whatever number the script gave them.
/// For tests and snapshots, so no network or disk is touched.
pub struct ScriptedUpdates {
    script: VecDeque<ServiceEvent>,
    sent: VecDeque<ServiceEvent>,
}

impl ScriptedUpdates {
    pub fn new(script: impl IntoIterator<Item = ServiceEvent>) -> Self {
        Self { script: script.into_iter().collect(), sent: VecDeque::new() }
    }
}

impl UpdateService for ScriptedUpdates {
    fn check(&mut self) {
        if matches!(self.script.front(), Some(ServiceEvent::Checked(_))) {
            self.sent.extend(self.script.pop_front());
        }
    }

    fn download(&mut self, _release: &Release, id: DownloadId) {
        loop {
            let event = match self.script.front() {
                Some(ServiceEvent::Progress { done, total, .. }) => ServiceEvent::Progress { id, done: *done, total: *total },
                Some(ServiceEvent::Ended { end, .. }) => ServiceEvent::Ended { id, end: end.clone() },
                _ => break,
            };
            self.script.pop_front();
            let ends = matches!(event, ServiceEvent::Ended { .. });
            self.sent.push_back(event);
            if ends {
                break;
            }
        }
    }

    /// Drops the download's events that were not read yet, and no answer to a check.
    fn cancel(&mut self) {
        self.sent.retain(|event| matches!(event, ServiceEvent::Checked(_)));
    }

    fn poll(&mut self) -> Option<ServiceEvent> {
        self.sent.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::updates::domain::{DownloadEnd, Failure, Version};

    fn release() -> Release {
        Release { version: Version { major: 0, minor: 2, patch: 0 }, released: None, notes: String::new(), size: 10, signature: "sig".into(), pins: Vec::new() }
    }

    fn drain(service: &mut dyn UpdateService) -> Vec<ServiceEvent> {
        std::iter::from_fn(|| service.poll()).collect()
    }

    fn progress(id: u64, done: u64) -> ServiceEvent {
        ServiceEvent::Progress { id: DownloadId(id), done, total: 10 }
    }

    fn ended(id: u64, end: DownloadEnd) -> ServiceEvent {
        ServiceEvent::Ended { id: DownloadId(id), end }
    }

    #[test]
    fn the_idle_service_never_answers() {
        let mut idle = IdleUpdates;
        idle.check();
        idle.download(&release(), DownloadId(1));
        idle.cancel();
        assert_eq!(idle.poll(), None);
    }

    #[test]
    fn a_check_gets_its_answer_at_once_and_nothing_before_it() {
        let mut service = ScriptedUpdates::new([ServiceEvent::Checked(Ok(release())), ServiceEvent::Checked(Err(Failure::TimedOut))]);
        assert_eq!(service.poll(), None, "nothing was asked yet");
        service.check();
        assert_eq!(drain(&mut service), [ServiceEvent::Checked(Ok(release()))]);
        service.check();
        assert_eq!(drain(&mut service), [ServiceEvent::Checked(Err(Failure::TimedOut))]);
        service.check();
        assert_eq!(service.poll(), None, "an empty script stays quiet");
    }

    #[test]
    fn a_download_gets_its_progress_and_its_end_with_its_own_number() {
        let done = DownloadEnd::Done(PathBuf::from("/updates/Jobdesk"));
        let mut service =
            ScriptedUpdates::new([ServiceEvent::Checked(Ok(release())), progress(0, 5), ended(0, done.clone()), ended(0, DownloadEnd::BadSignature)]);
        service.download(&release(), DownloadId(1));
        assert_eq!(service.poll(), None, "a check comes first");
        service.check();
        service.download(&release(), DownloadId(1));
        assert_eq!(drain(&mut service), [ServiceEvent::Checked(Ok(release())), progress(1, 5), ended(1, done)]);
        service.download(&release(), DownloadId(2));
        assert_eq!(drain(&mut service), [ended(2, DownloadEnd::BadSignature)]);
    }

    #[test]
    fn cancel_drops_only_the_events_of_the_download() {
        let mut service = ScriptedUpdates::new([ServiceEvent::Checked(Ok(release())), progress(0, 5), ended(0, DownloadEnd::BadSignature)]);
        service.check();
        service.download(&release(), DownloadId(1));
        service.cancel();
        assert_eq!(drain(&mut service), [ServiceEvent::Checked(Ok(release()))], "an unread answer to a check stays");
    }
}
