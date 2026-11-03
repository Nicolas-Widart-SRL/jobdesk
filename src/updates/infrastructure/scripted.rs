use std::collections::VecDeque;

use crate::updates::domain::{Release, ServiceEvent, UpdateService};

/// A service that never answers: a check or a download started with it stays running. For previews and snapshots.
#[derive(Default)]
pub struct IdleUpdates;

impl UpdateService for IdleUpdates {
    fn check(&mut self) {}

    fn download(&mut self, _release: &Release) {}

    fn cancel(&mut self) {}

    fn poll(&mut self) -> Option<ServiceEvent> {
        None
    }
}

/// A service that answers from a queue at once: a check gets the next `Checked`, and a download gets the progress and
/// the end that follow. For tests and snapshots, so no network or disk is touched.
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

    fn download(&mut self, _release: &Release) {
        while let Some(event @ (ServiceEvent::Progress { .. } | ServiceEvent::Ended(_))) = self.script.front().cloned() {
            self.script.pop_front();
            let ends = matches!(event, ServiceEvent::Ended(_));
            self.sent.push_back(event);
            if ends {
                break;
            }
        }
    }

    fn cancel(&mut self) {
        self.sent.clear();
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
        Release { version: Version { major: 0, minor: 2, patch: 0 }, released: None, notes: String::new(), size: 10, signature: "sig".into() }
    }

    fn drain(service: &mut dyn UpdateService) -> Vec<ServiceEvent> {
        std::iter::from_fn(|| service.poll()).collect()
    }

    #[test]
    fn the_idle_service_never_answers() {
        let mut idle = IdleUpdates;
        idle.check();
        idle.download(&release());
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
    fn a_download_gets_its_progress_and_its_end() {
        let end = ServiceEvent::Ended(DownloadEnd::Done(PathBuf::from("/updates/Jobdesk")));
        let mut service = ScriptedUpdates::new([
            ServiceEvent::Checked(Ok(release())),
            ServiceEvent::Progress { done: 5, total: 10 },
            end.clone(),
            ServiceEvent::Ended(DownloadEnd::BadSignature),
        ]);
        service.download(&release());
        assert_eq!(service.poll(), None, "a check comes first");
        service.check();
        service.download(&release());
        assert_eq!(drain(&mut service), [ServiceEvent::Checked(Ok(release())), ServiceEvent::Progress { done: 5, total: 10 }, end]);
        service.download(&release());
        assert_eq!(drain(&mut service), [ServiceEvent::Ended(DownloadEnd::BadSignature)]);
    }
}
