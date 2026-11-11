//! The Profile chapter's `/setup` run: Read my documents copies the documents and starts it, Jobdesk answers its path
//! question once with the documents path, and every other reply goes only when the user sends it. The phase the screen
//! shows comes from the run and the parse of Claude's last message, never from the disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{NaiveDate, NaiveDateTime, Timelike};

use crate::onboarding::application::work::{SoFar, WorkLine, so_far, work_lines};
use crate::onboarding::application::{Batch, CopyStop, Documents, Message, read_message};
use crate::onboarding::domain::{
    CV_LOOKS_GOOD, DocumentCopier, DocumentKind, Draft, PATH_A_REPLY, PICK_UP_REPLY, Progress, ProgressStore, Saved, joined_reply, review_reply, tool_line,
};
use crate::profile::application::{CvShelf, PdfDocument};
use crate::runs::application::{
    ActivityKind, CapCheck, Decision, Run, RunDefaults, RunDesk, RunStatus, StartError, StartOptions, closing_question, notice_text, spec_for_line,
    start_values,
};
use crate::shared_kernel::FolderWait;
use crate::shared_kernel::folder_wait::UNLOCKABLE;
use crate::shared_kernel::visible_text::visible_chars;

/// The command the chapter runs, unchanged and with no arguments.
pub const SETUP: &str = "/setup";

/// A draft is written to the record at most this often while the user types.
pub const DRAFT_SAVE_EVERY: Duration = Duration::from_secs(1);

/// The longest a draft that could not be written waits before it is tried again.
pub const DRAFT_RETRY_MOST: Duration = Duration::from_secs(60);

/// The longest the end of a turn waits for a snapshot read after it ended, before the CV check or the end of setup is
/// decided from the snapshot on show.
pub const SNAPSHOT_WAIT: Duration = Duration::from_secs(2);

/// What `/setup`'s closing summary says, in any case: ai-job-search v1.7.5, `.claude/commands/setup.md`, Step 4
/// ("Confirm & Next Steps"), opens it with "**Setup complete!**". No other message of `/setup` ends the chapter.
const SETUP_COMPLETE: &str = "setup complete";

/// What `/setup`'s path question names, in any case: v1.7.5's Step 0 offers "**Path A: Read my documents folder**" or
/// "**Path A: Documents folder**". A turn that does not name it, such as Step 0's warning about a public fork, is the
/// user's to answer.
const PATH_A: &str = "path a";

/// The progress line a write under `cv/` or a LaTeX build lights.
const MAKING_CV: &str = "Making your CV";

/// How a run's `started_at` is written.
const STARTED_AT: &str = "%Y-%m-%d %H:%M";

const YOU_STOPPED: &str = "You stopped setup.";
const STOPPED_BEFORE_READING: &str = "Setup stopped before Claude read your documents.";
const STOPPED_PARTWAY: &str = "Setup stopped before it finished.";

/// What the Profile chapter shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfilePhase {
    /// No run yet: the documents to add, and Read my documents.
    Documents,
    /// Read my documents was pressed: the documents are being copied, and the run starts once they are.
    Copying,
    /// Claude works on the turns Jobdesk started: the command line and the documents path.
    Reading,
    /// Claude works on a turn the user started.
    Working,
    /// Claude's message asks numbered questions: a field under each.
    Sheet { message: String, batch: Batch },
    /// Claude's message has nothing Jobdesk can count, or says nothing setup knows: the message beside one field.
    FallbackSheet { message: String },
    /// Claude proposes numbered changes and asks to confirm: a checkbox each.
    Review { message: String, batch: Batch },
    /// Claude built a CV PDF this turn: its pages, Looks good and Ask for a change.
    CvCheck { message: String, pdf: PdfDocument },
    /// A question from `AskUserQuestion`, or a permission prompt, waits on the user. Jobdesk never answers it.
    Choice,
    /// The run failed, ran out of budget, was stopped, or its last turn was cut off. The reason is plain text, without
    /// hidden characters.
    Stopped { reason: String },
    /// `/setup` said setup is complete, in a turn that ended cleanly: the profile is built.
    Done,
}

/// What Read my documents is checked against, as the launcher checks Run.
#[derive(Clone, Copy)]
pub struct StartRules<'a> {
    pub defaults: &'a RunDefaults,
    pub cap: &'a CapCheck,
    /// Settings' Test connection is running.
    pub testing: bool,
}

/// What `ProfileRun::poll` reads each frame.
pub struct PollInput<'a> {
    /// The snapshot's CV documents.
    pub cv: &'a CvShelf,
    /// When the snapshot's read of the folder began, on the window's clock. `None` before the first read.
    pub read_at: Option<Instant>,
    /// Whether a newer snapshot can come. Without a worker to read the folder again, the one on show is the newest.
    pub rereads: bool,
    /// The user went on without LaTeX, so no CV check can come.
    pub without_pdfs: bool,
    /// The rules a run starting once its documents are copied is checked against.
    pub rules: StartRules<'a>,
    /// The window's clock, for the draft's pace.
    pub now: Instant,
    /// The local time, for a run's start and the record's day.
    pub wall: NaiveDateTime,
}

/// Why Read my documents, or Pick up where it stopped, does not go. No line names a sum of money.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartRefusal {
    NoDocuments,
    /// A document is still being looked at.
    Looking,
    /// This setup's run exists already.
    AlreadyStarted,
    /// Another task works, and one run works at a time.
    Busy,
    Testing,
    /// Jobdesk updates the job search folder, or cannot lock it: no run starts and no reply goes, as for every task.
    FolderWait(FolderWait),
    CapReached {
        reopens_on: NaiveDate,
    },
    /// A cap is in force and this month's spend is not read yet.
    CapReading,
    Copy(CopyStop),
    /// This build's catalog has no `/setup`.
    NoCommand,
    /// Nothing is stopped to pick up.
    NotNow,
}

impl StartRefusal {
    pub fn line(&self) -> String {
        match self {
            Self::NoDocuments => "Add a document first.".to_owned(),
            Self::Looking => "Jobdesk is still reading your documents.".to_owned(),
            Self::AlreadyStarted => "Setup has already started reading your documents.".to_owned(),
            Self::Busy => "Another task is running. Setup can start once it ends.".to_owned(),
            Self::Testing => "Settings is testing the connection. Setup can start once the test ends.".to_owned(),
            Self::FolderWait(FolderWait::Updating) => "Jobdesk is updating your job search folder. Setup can start once it ends.".to_owned(),
            Self::FolderWait(FolderWait::Unlockable) => UNLOCKABLE.to_owned(),
            // A no-break space holds the day to its month.
            Self::CapReached { reopens_on } => {
                format!("This month's tasks reached the monthly cap you set in Settings. Setup can start on {}.", reopens_on.format("%-d\u{a0}%B"))
            }
            Self::CapReading => "Setup waits while this month's tasks are counted.".to_owned(),
            Self::Copy(stop) => stop.line(),
            Self::NoCommand => "This copy of Jobdesk cannot build a profile.".to_owned(),
            Self::NotNow => SendRefusal::NotNow.line().to_owned(),
        }
    }
}

/// Why a reply was not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendRefusal {
    /// Claude is not waiting on this reply now: it works, or shows something else.
    NotNow,
    Empty,
    /// The review has no reply to send: no change is ticked.
    NoReply,
    Busy,
    Testing,
    /// Jobdesk updates the job search folder, or cannot lock it.
    FolderWait(FolderWait),
}

impl SendRefusal {
    pub fn line(self) -> &'static str {
        match self {
            Self::NotNow => "Claude is not waiting on this right now.",
            Self::Empty => "Write something first.",
            Self::NoReply => "Tick a change, or use Change something.",
            Self::Busy => "Another task is running. Send once it ends.",
            Self::Testing => "Settings is testing the connection. Send once the test ends.",
            Self::FolderWait(FolderWait::Updating) => "Jobdesk is updating your job search folder. Send once it ends.",
            Self::FolderWait(FolderWait::Unlockable) => UNLOCKABLE,
        }
    }
}

impl From<SendRefusal> for StartRefusal {
    fn from(refusal: SendRefusal) -> Self {
        match refusal {
            SendRefusal::Busy => Self::Busy,
            SendRefusal::Testing => Self::Testing,
            SendRefusal::FolderWait(wait) => Self::FolderWait(wait),
            SendRefusal::NotNow | SendRefusal::Empty | SendRefusal::NoReply => Self::NotNow,
        }
    }
}

/// Why the record does not hold what the user typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveTrouble {
    /// The draft would make the record too long to read back, so the record was written without it.
    TooLong,
    /// The record could not be written.
    CouldNotWrite,
}

/// The Profile chapter over the job search folder: its documents, its `/setup` run on the shell's `RunDesk`, and the
/// progress record, which keeps the run's id, whether the documents path was sent, and the draft.
pub struct ProfileRun {
    store: Arc<dyn ProgressStore>,
    progress: Progress,
    folder: PathBuf,
    documents: Documents,
    today: NaiveDate,
    phase: ProfilePhase,
    /// Read my documents was pressed, and the run starts once the copy ends.
    starting: bool,
    /// Why the last start did not happen, once the copy ended.
    start_failure: Option<StartRefusal>,
    /// The record misses something held here: the draft, or more, when a save failed.
    unsaved: bool,
    trouble: Option<SaveTrouble>,
    /// When the draft was last tried, which paces the writes, failed ones included.
    last_draft_try: Option<Instant>,
    /// When the draft last reached the record.
    last_draft_save: Option<Instant>,
    /// The turn `poll` last saw, and when it began, known only when `poll` saw it working.
    turn: Option<TurnSeen>,
    /// The phase waits for a snapshot read after the turn ended, until this time at the latest.
    snapshot_due: Option<Instant>,
    /// The user pressed Finish setup, after a turn of the run said setup is complete.
    finished: bool,
    /// The recap opened and the record is gone, so nothing writes it again.
    cleared: bool,
    /// The draft was too long for the record, and waits for a change before it is tried again.
    held: bool,
    /// How many draft writes in a row could not be written, which spaces out the next try.
    failures: u32,
}

/// A turn of the run as `poll` first saw it. Kept in memory only.
struct TurnSeen {
    run: String,
    /// `Run::current_turn_start` of the turn.
    start: usize,
    began: Option<NaiveDateTime>,
    /// When `poll` first saw the turn not working, on the window's clock.
    ended: Option<Instant>,
}

impl ProfileRun {
    /// `progress` is the record as first-run setup holds it, and `folder` the job search folder it made. `repaint` wakes
    /// the window when a worker that reads or copies the documents has news.
    pub fn new(
        store: Arc<dyn ProgressStore>,
        progress: Progress,
        folder: PathBuf,
        copier: Arc<dyn DocumentCopier>,
        repaint: Arc<dyn Fn() + Send + Sync>,
        today: NaiveDate,
    ) -> Self {
        Self {
            store,
            progress,
            folder,
            documents: Documents::new(copier, repaint),
            today,
            phase: ProfilePhase::Documents,
            starting: false,
            start_failure: None,
            unsaved: false,
            trouble: None,
            last_draft_try: None,
            last_draft_save: None,
            turn: None,
            snapshot_due: None,
            finished: false,
            cleared: false,
            held: false,
            failures: 0,
        }
    }

    /// Whether a turn of this setup's run said setup is complete, in its last top-level message or any other. `/setup`
    /// may say so and then take a change the user asks for, and end that turn with no clean end of its own.
    pub fn complete_seen(&self, desk: &RunDesk) -> bool {
        self.run(desk).is_some_and(|run| run.activity.iter().any(|row| matches!(&row.kind, ActivityKind::Claude(text) if !row.nested && says_complete(text))))
    }

    /// The progress lines for the run as it stands, or for the copy before it.
    pub fn work_lines(&self, desk: &RunDesk) -> Vec<WorkLine> {
        work_lines(self.run(desk), &self.folder)
    }

    /// The rounds answered and the changes approved so far in this setup's run.
    pub fn so_far(&self, desk: &RunDesk) -> SoFar {
        so_far(self.run(desk))
    }

    /// Finish setup, which the user pressed on a sheet with no numbered questions, once a turn said setup is complete.
    /// Setup never ends this way on its own. Gives whether it ended.
    pub fn finish(&mut self, desk: &RunDesk) -> bool {
        if !matches!(self.phase, ProfilePhase::FallbackSheet { .. }) || !self.complete_seen(desk) {
            return false;
        }
        self.finished = true;
        self.phase = ProfilePhase::Done;
        true
    }

    /// The recap opened: the record is removed, and nothing writes it again, so the next open shows no setup. A record
    /// that cannot be removed costs only a welcome back, whose run then shows as done.
    pub fn clear_record(&mut self) {
        let _ = self.store.clear();
        self.cleared = true;
        self.unsaved = false;
        self.trouble = None;
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub fn documents(&self) -> &Documents {
        &self.documents
    }

    /// The files the user dropped or chose, looked at on a worker. Nothing is copied until Read my documents.
    pub fn add_documents(&mut self, sources: impl IntoIterator<Item = PathBuf>) {
        self.documents.add(sources);
    }

    pub fn remove_document(&mut self, at: usize) {
        self.documents.remove(at);
    }

    /// What a document not copied yet is, which decides the subfolder of `documents/` it goes to.
    pub fn set_document_kind(&mut self, at: usize, kind: DocumentKind) {
        self.documents.set_kind(at, kind);
    }

    /// The phase as the last `poll` found it.
    pub fn phase(&self) -> &ProfilePhase {
        &self.phase
    }

    /// Whether Read my documents was pressed and the run waits for the copy to end.
    pub fn is_starting(&self) -> bool {
        self.starting
    }

    /// Why the run did not start once the copy ended, until the next try.
    pub fn start_failure(&self) -> Option<&StartRefusal> {
        self.start_failure.as_ref()
    }

    /// This setup's run, found by the id the record keeps, so a desk restored from the run logs finds it again.
    pub fn run<'a>(&self, desk: &'a RunDesk) -> Option<&'a Run> {
        desk.run(self.progress.run.as_deref()?).filter(|run| run.line == SETUP)
    }

    /// Why Read my documents would not start the run now, if anything holds it.
    pub fn start_refusal(&self, desk: &RunDesk, rules: &StartRules) -> Option<StartRefusal> {
        if self.run(desk).is_some() || self.starting {
            return Some(StartRefusal::AlreadyStarted);
        }
        if self.documents.is_empty() {
            return Some(StartRefusal::NoDocuments);
        }
        if self.documents.is_looking() {
            return Some(StartRefusal::Looking);
        }
        gate(desk, rules)
    }

    /// Read my documents: the user starting the run, as Run does in the launcher. The documents are copied into
    /// `documents/` on a worker, and once every one is there `poll` starts `/setup` with the model and budget the
    /// launcher would start it with, checked again against the same rules.
    pub fn read_documents(&mut self, desk: &mut RunDesk, rules: &StartRules, wall: NaiveDateTime) -> Result<(), StartRefusal> {
        if let Some(refusal) = self.start_refusal(desk, rules) {
            return Err(refusal);
        }
        self.begin(desk, rules, wall)
    }

    /// Copies what is left to copy, or starts the run when nothing is.
    fn begin(&mut self, desk: &mut RunDesk, rules: &StartRules, wall: NaiveDateTime) -> Result<(), StartRefusal> {
        if let Some(refusal) = gate(desk, rules) {
            return Err(refusal);
        }
        spec_for_line(SETUP).ok_or(StartRefusal::NoCommand)?;
        self.start_failure = None;
        if self.documents.all_copied() {
            return self.start_now(desk, rules, wall);
        }
        self.documents.start_copy(&self.folder);
        self.starting = true;
        self.phase = ProfilePhase::Copying;
        Ok(())
    }

    /// Starts `/setup` and keeps the new run's id in the record, with the documents path not sent yet and no draft.
    fn start_now(&mut self, desk: &mut RunDesk, rules: &StartRules, wall: NaiveDateTime) -> Result<(), StartRefusal> {
        if let Some(refusal) = gate(desk, rules) {
            return Err(refusal);
        }
        let spec = spec_for_line(SETUP).ok_or(StartRefusal::NoCommand)?;
        let values = start_values(rules.defaults, spec);
        let options = StartOptions { model: values.model.alias().map(str::to_owned), budget_usd: values.budget.usd() };
        let wait = desk.folder_wait().unwrap_or(FolderWait::Updating);
        let id = desk.start(SETUP, &options, wall).map_err(|error| match error {
            StartError::AnotherRunIsActive => StartRefusal::Busy,
            StartError::TestRunning => StartRefusal::Testing,
            StartError::FolderUpdating => StartRefusal::FolderWait(wait),
        })?;
        self.progress.run = Some(id);
        self.progress.path_a_sent = false;
        self.progress.draft = None;
        let _ = self.save();
        self.phase = ProfilePhase::Reading;
        Ok(())
    }

    /// Call once per frame, after `RunDesk::poll`. It takes the documents' news, starts the run once the copy for it
    /// ended, sends the documents path when Claude asks for it, and finds the phase. A draft waiting to be written is
    /// written once `DRAFT_SAVE_EVERY` has passed since the last try.
    pub fn poll(&mut self, desk: &mut RunDesk, input: &PollInput) {
        self.today = input.wall.date();
        self.documents.poll();
        if self.starting && !self.documents.is_copying() {
            self.starting = false;
            let started = match self.documents.copy_failure() {
                Some(stop) => Err(StartRefusal::Copy(stop.clone())),
                None => self.start_now(desk, &input.rules, input.wall),
            };
            self.start_failure = started.err();
        }
        self.send_path_a(desk);
        let began = self.see_turn(desk, input.wall, input.now);
        let due = self.turn.as_ref().and_then(|seen| seen.ended).map(|ended| ended + SNAPSHOT_WAIT);
        let read_since = |ended: Instant| !input.rereads || input.read_at.is_some_and(|read| read >= ended);
        let settled = self.turn.as_ref().and_then(|seen| seen.ended).is_none_or(read_since) || due.is_some_and(|due| input.now >= due);
        let mut waits = false;
        self.phase = if self.finished {
            ProfilePhase::Done
        } else if self.starting {
            ProfilePhase::Copying
        } else {
            let read = phase_of(self.run(desk), self.progress.path_a_sent, &self.folder, input.cv, input.without_pdfs, began, settled);
            waits = read.waits;
            read.phase
        };
        self.snapshot_due = due.filter(|_| waits);
        self.save_draft_if_due(input.now);
    }

    /// How long the phase still waits for a snapshot read after the turn ended, so the window can look again then even
    /// with no input. `None` when it does not wait.
    pub fn snapshot_wait(&self, now: Instant) -> Option<Duration> {
        self.snapshot_due.map(|due| due.saturating_duration_since(now))
    }

    /// When the run's current turn began, as far as this window saw. A turn first seen while it works began now, to the
    /// second. One first seen after it ended, as after a restore, has no known start. The turn's end is noted at `now`
    /// the first time it is seen not working.
    fn see_turn(&mut self, desk: &RunDesk, wall: NaiveDateTime, now: Instant) -> Option<NaiveDateTime> {
        let run = self.run(desk)?;
        let (start, busy) = (run.current_turn_start(), run.is_busy());
        if !self.turn.as_ref().is_some_and(|seen| seen.run == run.id && seen.start == start) {
            let began = busy.then(|| wall.with_nanosecond(0).unwrap_or(wall));
            self.turn = Some(TurnSeen { run: run.id.clone(), start, began, ended: None });
        }
        let seen = self.turn.as_mut()?;
        if busy {
            seen.ended = None;
        } else if seen.ended.is_none() {
            seen.ended = Some(now);
        }
        seen.began
    }

    /// Jobdesk's one reply of its own. It goes when a turn that names Path A ended cleanly on its live session, with
    /// nothing else open, and never again once it went in this run, so a restart cannot send it twice.
    fn send_path_a(&mut self, desk: &mut RunDesk) {
        let path_a_sent = self.progress.path_a_sent;
        let Some(id) =
            self.run(desk).filter(|run| run.awaits_reply() && desk.has_session(&run.id) && waits_for_path(run, path_a_sent)).map(|run| run.id.clone())
        else {
            return;
        };
        if desk.reply(&id, PATH_A_REPLY).is_ok() {
            self.progress.path_a_sent = true;
            let _ = self.save();
        }
    }

    /// Send answers on a question sheet: one numbered reply in Claude's order, with "Skipped" for an empty field.
    pub fn send_answers(&mut self, desk: &mut RunDesk, answers: &[(u32, &str)]) -> Result<(), SendRefusal> {
        let ProfilePhase::Sheet { batch, .. } = &self.phase else { return Err(SendRefusal::NotNow) };
        let reply = joined_reply(&batch.numbers(), answers);
        self.send(desk, &reply)
    }

    /// Apply these changes on the review card, from whether each change is ticked, by Claude's number.
    pub fn send_review(&mut self, desk: &mut RunDesk, ticks: &[(u32, bool)]) -> Result<(), SendRefusal> {
        let ProfilePhase::Review { batch, .. } = &self.phase else { return Err(SendRefusal::NotNow) };
        let reply = review_reply(&batch.numbers(), ticks).ok_or(SendRefusal::NoReply)?;
        self.send(desk, &reply)
    }

    /// The user's own words: the answer on a sheet with no numbered questions, Change something on the review card, or
    /// Ask for a change on the CV check.
    pub fn send_text(&mut self, desk: &mut RunDesk, text: &str) -> Result<(), SendRefusal> {
        if !matches!(self.phase, ProfilePhase::FallbackSheet { .. } | ProfilePhase::Review { .. } | ProfilePhase::CvCheck { .. }) {
            return Err(SendRefusal::NotNow);
        }
        let text = text.trim();
        if text.is_empty() {
            return Err(SendRefusal::Empty);
        }
        self.send(desk, text)
    }

    /// Looks good, on the CV check.
    pub fn cv_looks_good(&mut self, desk: &mut RunDesk) -> Result<(), SendRefusal> {
        if !matches!(self.phase, ProfilePhase::CvCheck { .. }) {
            return Err(SendRefusal::NotNow);
        }
        self.send(desk, CV_LOOKS_GOOD)
    }

    /// Whether Pick up where it stopped goes on in the run's own conversation: the run reported a session. Without one,
    /// it starts `/setup` again from the documents.
    pub fn picks_up_in_session(&self, desk: &RunDesk) -> bool {
        self.run(desk).is_some_and(|run| run.session_id.is_some())
    }

    /// Pick up where it stopped. A run that never reported a session has nothing to resume, so `/setup` starts anew
    /// under the rules of Read my documents, and the record moves to the new run. Otherwise it is a reply on the run's
    /// own session: the documents path for a run stopped while Claude asked for it, else a plain pick up. A failed run's
    /// process is ended first, so the reply resumes the session in a new process with a budget of its own: the old one
    /// may have spent all of its budget.
    pub fn pick_up(&mut self, desk: &mut RunDesk, rules: &StartRules, wall: NaiveDateTime) -> Result<(), StartRefusal> {
        if !matches!(self.phase, ProfilePhase::Stopped { .. }) {
            return Err(StartRefusal::NotNow);
        }
        let path_a_sent = self.progress.path_a_sent;
        let Some((id, resumable, failed, path_a)) =
            self.run(desk).map(|run| (run.id.clone(), run.session_id.is_some(), matches!(run.status, RunStatus::Failed(_)), waits_for_path(run, path_a_sent)))
        else {
            return Err(StartRefusal::NotNow);
        };
        // Before anything is closed, so a refused pick up leaves the run as it was.
        if let Some(wait) = desk.folder_wait() {
            return Err(StartRefusal::FolderWait(wait));
        }
        if !resumable {
            if let Some(refusal) = gate(desk, rules) {
                return Err(refusal);
            }
            desk.close(&id);
            return self.begin(desk, rules, wall);
        }
        if failed && desk.has_session(&id) {
            desk.close(&id);
        }
        self.send(desk, if path_a { PATH_A_REPLY } else { PICK_UP_REPLY })?;
        if path_a {
            self.progress.path_a_sent = true;
            let _ = self.save();
        }
        Ok(())
    }

    /// Stop setup. While Claude works, the turn is stopped. While Claude waits on the user, the session's process is
    /// ended, so nothing stays open behind the app. Either way the run keeps its session and the draft, and the next
    /// reply, or Pick up where it stopped, resumes it.
    pub fn stop(&mut self, desk: &mut RunDesk) {
        let Some((id, busy)) = self.run(desk).map(|run| (run.id.clone(), run.is_busy())) else { return };
        if busy {
            desk.stop(&id);
        } else if desk.has_session(&id) {
            desk.close(&id);
        }
    }

    /// The user's answer to the open prompt, on the choice card. Only the user's choice gets here.
    pub fn decide(&mut self, desk: &mut RunDesk, request_id: &str, decision: Decision) {
        if let (ProfilePhase::Choice, Some(id)) = (&self.phase, self.run(desk).map(|run| run.id.clone())) {
            desk.decide(&id, request_id, decision);
        }
    }

    /// Sends `text` on the run's session, only while Claude waits on the user. Once it went, the draft is cleared.
    fn send(&mut self, desk: &mut RunDesk, text: &str) -> Result<(), SendRefusal> {
        let Some(id) = self.run(desk).filter(|run| !run.is_busy() && !run.needs_user()).map(|run| run.id.clone()) else {
            return Err(SendRefusal::NotNow);
        };
        let wait = desk.folder_wait().unwrap_or(FolderWait::Updating);
        desk.reply(&id, text).map_err(|error| match error {
            StartError::AnotherRunIsActive => SendRefusal::Busy,
            StartError::TestRunning => SendRefusal::Testing,
            StartError::FolderUpdating => SendRefusal::FolderWait(wait),
        })?;
        self.progress.draft = None;
        let _ = self.save();
        // Until the next poll, so nothing is sent twice in one frame.
        self.phase = ProfilePhase::Working;
        Ok(())
    }

    /// The draft of Claude's current turn, if the user has typed one.
    pub fn draft(&self, desk: &RunDesk) -> Option<&Draft> {
        let run = self.run(desk)?;
        self.progress.draft.as_ref().filter(|draft| draft.run == run.id && draft.turn == run.turn_number())
    }

    /// What the user has typed so far for Claude's current turn. It is written to the record at most once every
    /// `DRAFT_SAVE_EVERY`, and by `flush`. A draft emptied by the user is dropped.
    pub fn record_draft(&mut self, desk: &RunDesk, answers: Vec<(u32, String)>, text: String, now: Instant) {
        let Some(run) = self.run(desk) else { return };
        let draft = Draft { run: run.id.clone(), turn: run.turn_number(), answers, text };
        let draft = (!draft.is_empty()).then_some(draft);
        if self.progress.draft == draft {
            return;
        }
        self.progress.draft = draft;
        self.unsaved = true;
        // A draft too long for the record is tried again once it changes.
        self.held = false;
        self.save_draft_if_due(now);
    }

    /// How long until the draft that waits is tried again: `DRAFT_SAVE_EVERY` at first, then twice as long after each
    /// write that failed, up to `DRAFT_RETRY_MOST`. `None` when nothing waits, or when the draft is too long for the
    /// record and waits for a change.
    pub fn next_draft_try(&self, now: Instant) -> Option<Duration> {
        if !self.unsaved || self.held {
            return None;
        }
        let wait = DRAFT_SAVE_EVERY.saturating_mul(1 << self.failures.min(6)).min(DRAFT_RETRY_MOST);
        Some(self.last_draft_try.map_or(Duration::ZERO, |at| wait.saturating_sub(now.saturating_duration_since(at))))
    }

    /// Whether the record misses what is held here, the draft included: it waits for its next write, or the last write
    /// failed.
    pub fn is_unsaved(&self) -> bool {
        self.unsaved
    }

    /// Why the last write left something out of the record, until a write holds it all.
    pub fn save_trouble(&self) -> Option<SaveTrouble> {
        self.trouble
    }

    /// When the draft last reached the record, for the line that says so.
    pub fn last_draft_save(&self) -> Option<Instant> {
        self.last_draft_save
    }

    /// Writes what waits once `next_draft_try` says it is due. A draft too long for the record waits for a change, and a
    /// record that cannot be written is tried less and less often.
    pub fn save_draft_if_due(&mut self, now: Instant) {
        if self.next_draft_try(now).is_none_or(|left| !left.is_zero()) {
            return;
        }
        self.last_draft_try = Some(now);
        match self.save() {
            Ok(()) => {
                self.last_draft_save = Some(now);
                self.failures = 0;
            }
            Err(SaveTrouble::TooLong) => self.held = true,
            Err(SaveTrouble::CouldNotWrite) => self.failures = self.failures.saturating_add(1),
        }
    }

    /// Writes what waits now, as the window closes. A write that failed before is tried again.
    pub fn flush(&mut self) {
        if self.unsaved {
            let _ = self.save();
        }
    }

    /// Keeps the record. Only a write that holds everything, the draft included, clears `unsaved`. One that fails costs
    /// only the resume, so setup goes on with what it holds, and the trouble says what is missing.
    fn save(&mut self) -> Result<(), SaveTrouble> {
        if self.cleared {
            return Ok(());
        }
        self.progress.stopped = Some(self.today);
        let saved = match self.store.save(&self.progress) {
            Ok(Saved::Whole) => Ok(()),
            Ok(Saved::WithoutDraft) => Err(SaveTrouble::TooLong),
            Err(_) => Err(SaveTrouble::CouldNotWrite),
        };
        self.unsaved = saved.is_err();
        self.trouble = saved.err();
        saved
    }
}

/// What holds back a run that starts: another run, the folder update, a connection test or the monthly cap. The folder
/// update holds back the copy into `documents/` too, since the copy writes the job search folder.
fn gate(desk: &RunDesk, rules: &StartRules) -> Option<StartRefusal> {
    if desk.is_busy() {
        return Some(StartRefusal::Busy);
    }
    if let Some(wait) = desk.folder_wait() {
        return Some(StartRefusal::FolderWait(wait));
    }
    if rules.testing {
        return Some(StartRefusal::Testing);
    }
    match rules.cap {
        CapCheck::Reached(reached) => Some(StartRefusal::CapReached { reopens_on: reached.reopens_on }),
        CapCheck::Reading => Some(StartRefusal::CapReading),
        CapCheck::Open => None,
    }
}

/// The phase a poll found, and whether it shows Claude working only while it waits for a snapshot read after the turn
/// ended.
struct PhaseRead {
    phase: ProfilePhase,
    waits: bool,
}

impl From<ProfilePhase> for PhaseRead {
    fn from(phase: ProfilePhase) -> Self {
        Self { phase, waits: false }
    }
}

/// The phase `run` is in. `path_a_sent` says whether the documents path went, `root` is the job search folder, `cv`
/// the snapshot's CV documents, `without_pdfs` holds when no CV can be built, `turn_began` is when the current turn
/// began, when it was seen beginning, and `settled` says the snapshot was read after the turn ended, or need not be.
fn phase_of(
    run: Option<&Run>,
    path_a_sent: bool,
    root: &Path,
    cv: &CvShelf,
    without_pdfs: bool,
    turn_began: Option<NaiveDateTime>,
    settled: bool,
) -> PhaseRead {
    let Some(run) = run else { return ProfilePhase::Documents.into() };
    if run.needs_user() {
        return ProfilePhase::Choice.into();
    }
    if run.is_busy() {
        return working(run, path_a_sent).into();
    }
    match &run.status {
        RunStatus::Failed(reason) => return ProfilePhase::Stopped { reason: notice_text(reason).full }.into(),
        RunStatus::Stopped => return ProfilePhase::Stopped { reason: YOU_STOPPED.to_owned() }.into(),
        _ => {}
    }
    // A turn with no result in the run was cut off, by a quit or a crash. Whatever it said last, it never ended.
    if !run.has_turn_result() {
        return ProfilePhase::Stopped { reason: STOPPED_PARTWAY.to_owned() }.into();
    }
    let closed = run.status == RunStatus::Closed;
    if waits_for_path(run, path_a_sent) {
        // A live run is answered on this poll. One whose process ended waits for the user to pick it up.
        return if closed { ProfilePhase::Stopped { reason: STOPPED_BEFORE_READING.to_owned() } } else { ProfilePhase::Reading }.into();
    }
    match at_turn_end(run, root, cv, without_pdfs, turn_began, settled) {
        Some(phase) => phase.into(),
        None => PhaseRead { phase: working(run, path_a_sent), waits: true },
    }
}

/// What shows while Claude works: Reading on the turns Jobdesk started, Working on the user's.
fn working(run: &Run, path_a_sent: bool) -> ProfilePhase {
    if jobdesk_started_the_turn(run, path_a_sent) { ProfilePhase::Reading } else { ProfilePhase::Working }
}

/// The phase of a turn that ended cleanly with nothing open. A turn that asks numbered questions or proposes changes is
/// never the end, whatever else it says. The CV check shows only for a turn whose message has no numbered items, so "The CV looks good." never answers
/// questions or a proposal.
///
/// The CV check and the end of setup read the snapshot, which may predate the turn's last write. Unless `settled`, they
/// wait: `None` while the CV check is in question or setup would end.
fn at_turn_end(run: &Run, root: &Path, cv: &CvShelf, without_pdfs: bool, turn_began: Option<NaiveDateTime>, settled: bool) -> Option<ProfilePhase> {
    let message = last_message(run).unwrap_or_default();
    match read_message(&message) {
        Message::Proposal(batch) => return Some(ProfilePhase::Review { message, batch }),
        Message::Questions(batch) => return Some(ProfilePhase::Sheet { message, batch }),
        Message::Free => {}
    }
    let checks_cv = !without_pdfs && built_cv(run, root);
    let complete = setup_complete(run, &message);
    if !settled && (checks_cv || complete) {
        return None;
    }
    if checks_cv && let Some(pdf) = fresh_pdf(run, cv, turn_began) {
        return Some(ProfilePhase::CvCheck { message, pdf });
    }
    if complete {
        return Some(ProfilePhase::Done);
    }
    Some(ProfilePhase::FallbackSheet { message })
}

/// Whether the turn closes on a question, as Tasks reads one. A run read back from its log reads as its turn ended.
fn asks_back(run: &Run) -> bool {
    if run.awaits_reply() {
        return closing_question(run).is_some();
    }
    let mut ended = run.clone();
    ended.status = RunStatus::AwaitingReply;
    closing_question(&ended).is_some()
}

/// `text` as plain lower-case text, without hidden characters.
fn plain(text: &str) -> String {
    visible_chars(text).collect::<String>().to_lowercase()
}

/// Whether `text` says setup is complete, as whole words: "Setup complete!" counts, and "so setup completes" or "setup
/// completed" do not.
fn says_complete(text: &str) -> bool {
    let text = plain(text);
    text.match_indices(SETUP_COMPLETE).any(|(at, _)| {
        let before = text[..at].chars().next_back().is_some_and(char::is_alphanumeric);
        let after = text[at + SETUP_COMPLETE.len()..].chars().next().is_some_and(char::is_alphanumeric);
        !before && !after
    })
}

/// Whether `text` names Path A, as a word: "Path A:" counts, and "path about" does not.
fn names_path_a(text: &str) -> bool {
    let text = plain(text);
    text.match_indices(PATH_A).any(|(at, _)| !text[at + PATH_A.len()..].starts_with(char::is_alphanumeric))
}

/// Whether the turn ends setup: it closes on no question, and it says setup is complete, or it answers the user's "The
/// CV looks good." after an earlier turn of the run said so. That earlier turn may have shown the CV check, and a change
/// asked for there keeps it said. A turn that closes on a question waits on the user, whatever else it says. Read from
/// the run's own rows, so it holds after a restore.
fn setup_complete(run: &Run, message: &str) -> bool {
    if asks_back(run) {
        return false;
    }
    if says_complete(message) {
        return true;
    }
    let start = run.current_turn_start();
    let answers_check =
        start.checked_sub(1).and_then(|at| run.activity.get(at)).is_some_and(|row| matches!(&row.kind, ActivityKind::You(text) if text == CV_LOOKS_GOOD));
    answers_check && earlier_turns_said_complete(run, start)
}

/// Whether a turn before the row at `start` said setup is complete in its last top-level message.
fn earlier_turns_said_complete(run: &Run, start: usize) -> bool {
    let mut last: Option<&str> = None;
    let mut said = false;
    for activity in &run.activity[..start.min(run.activity.len())] {
        match &activity.kind {
            ActivityKind::You(_) => said |= last.take().is_some_and(says_complete),
            ActivityKind::Claude(text) if !activity.nested => last = Some(text),
            _ => {}
        }
    }
    said || last.is_some_and(says_complete)
}

/// Whether Claude's last turn asks for the documents path and Jobdesk has not sent it in this run.
fn waits_for_path(run: &Run, path_a_sent: bool) -> bool {
    let sent = path_a_sent || run.activity.iter().any(|row| matches!(&row.kind, ActivityKind::You(text) if text == PATH_A_REPLY));
    !sent && run.has_turn_result() && last_message(run).is_some_and(|message| names_path_a(&message))
}

/// Whether the turn under way is one Jobdesk started: the command line or the documents path, or one before the path
/// went.
fn jobdesk_started_the_turn(run: &Run, path_a_sent: bool) -> bool {
    let last_you = run.activity.iter().rev().find_map(|row| match &row.kind {
        ActivityKind::You(text) => Some(text.as_str()),
        _ => None,
    });
    !path_a_sent || run.turn_number() == 1 || last_you == Some(PATH_A_REPLY)
}

/// The last top-level message of the current turn.
fn last_message(run: &Run) -> Option<String> {
    run.activity.iter().skip(run.current_turn_start()).rev().find_map(|activity| match &activity.kind {
        ActivityKind::Claude(text) if !activity.nested => Some(text.clone()),
        _ => None,
    })
}

/// Whether the current turn built a CV PDF: it ran a LaTeX build, or wrote a `.pdf` under `cv/`. Only a build changes
/// the PDF, and each turn ends once, so a PDF the user answered never shows its check again, before or after a
/// restore: their answer starts a new turn, and that one shows the check only for a PDF it builds again.
fn built_cv(run: &Run, root: &Path) -> bool {
    let root = root.to_string_lossy();
    run.activity.iter().skip(run.current_turn_start()).any(|activity| match &activity.kind {
        ActivityKind::Tool { name, summary, .. } => {
            tool_line(&root, name, summary) == Some(MAKING_CV) && (name == "Bash" || summary.to_lowercase().ends_with(".pdf"))
        }
        _ => false,
    })
}

/// The newest CV PDF of the snapshot written since the run started. The run keeps its start to the minute, so a PDF
/// from that minute counts.
///
/// When `turn_began` knows when the current turn began, only a PDF written since then counts, so a build that failed or
/// built another file never shows a PDF the user already answered. A turn first seen after it ended, as after a
/// restore, has no known start and falls back to the run's.
fn fresh_pdf(run: &Run, cv: &CvShelf, turn_began: Option<NaiveDateTime>) -> Option<PdfDocument> {
    let started = match turn_began {
        Some(began) => began,
        None => NaiveDateTime::parse_from_str(&run.started_at, STARTED_AT).ok()?,
    };
    let CvShelf::Read { documents, .. } = cv else { return None };
    documents
        .iter()
        .filter(|document| document.pdf.as_ref().is_some_and(|pdf| pdf.at >= started))
        .filter_map(|document| document.pdf_document())
        .max_by_key(|pdf| pdf.modified)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Mutex;
    use std::thread;

    use super::*;
    use crate::onboarding::domain::{CopyFailure, PROGRESS_LINES, Refusal, by_name, shown_name};
    use crate::profile::application::{CvFiles, FileStamp, cv_shelf};
    use crate::runs::application::{
        AgentEvent, AgentGateway, AgentSession, CapReached, EventSink, Prompt, PromptKind, RunHistory, RunRequest, WORDS_BUDGET_USD,
    };
    use crate::settings::application::{CommandDefault, Dollars};
    use crate::shared_kernel::{LoadError, ModelChoice};

    #[derive(Default)]
    struct Script {
        requests: Vec<RunRequest>,
        sent: Vec<String>,
        interrupted: bool,
        sink: Option<EventSink>,
        /// The next process reports no session, as one whose key is gone.
        silent: bool,
    }

    /// A gateway that keeps the sink of the latest process, so a test can play Claude's side.
    struct ScriptedGateway(Rc<RefCell<Script>>);
    struct ScriptedSession(Rc<RefCell<Script>>);

    impl AgentGateway for ScriptedGateway {
        fn start(&self, request: &RunRequest, sink: EventSink) -> Box<dyn AgentSession> {
            let silent = self.0.borrow().silent;
            if !silent {
                sink(AgentEvent::SessionStarted { session_id: "s1".into(), model: "m".into(), api_key_source: "none".into() });
            }
            let mut script = self.0.borrow_mut();
            script.requests.push(request.clone());
            script.sink = Some(sink);
            Box::new(ScriptedSession(Rc::clone(&self.0)))
        }
    }

    impl AgentSession for ScriptedSession {
        fn send_message(&mut self, text: &str) {
            self.0.borrow_mut().sent.push(text.to_owned());
        }
        fn respond(&mut self, _: &Prompt, decision: &Decision) {
            self.0.borrow_mut().sent.push(format!("{decision:?}"));
        }
        fn interrupt(&mut self) {
            self.0.borrow_mut().interrupted = true;
        }
    }

    /// How the memory store answers a save.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Disk {
        Fine,
        Full,
        TooLong,
    }

    struct Memory {
        record: Mutex<Option<Progress>>,
        saves: Mutex<usize>,
        disk: Mutex<Disk>,
    }

    impl Default for Memory {
        fn default() -> Self {
            Self { record: Mutex::new(None), saves: Mutex::new(0), disk: Mutex::new(Disk::Fine) }
        }
    }

    impl ProgressStore for Memory {
        fn load(&self) -> Result<Option<Progress>, LoadError> {
            Ok(self.record.lock().unwrap().clone())
        }
        fn save(&self, progress: &Progress) -> Result<Saved, LoadError> {
            *self.saves.lock().unwrap() += 1;
            match *self.disk.lock().unwrap() {
                Disk::Full => Err(LoadError::new("onboarding.json", "disk full")),
                Disk::TooLong => {
                    *self.record.lock().unwrap() = Some(Progress { draft: None, ..progress.clone() });
                    Ok(Saved::WithoutDraft)
                }
                Disk::Fine => {
                    *self.record.lock().unwrap() = Some(progress.clone());
                    Ok(Saved::Whole)
                }
            }
        }
        fn clear(&self) -> Result<(), LoadError> {
            *self.record.lock().unwrap() = None;
            Ok(())
        }
    }

    /// Takes a PDF by its name and copies under that name, or panics on each copy once told to.
    #[derive(Default)]
    struct Shelf(Mutex<Vec<String>>, Mutex<bool>);

    impl DocumentCopier for Shelf {
        fn look(&self, source: &Path) -> Result<Option<usize>, Refusal> {
            by_name(&shown_name(source)).map(|()| Some(1))
        }
        fn copy(&self, source: &Path, _: &Path, _: DocumentKind) -> Result<String, CopyFailure> {
            if *self.1.lock().unwrap() {
                panic!("the test copier panics");
            }
            self.0.lock().unwrap().push(shown_name(source));
            Ok(shown_name(source))
        }
    }

    const ROOT: &str = "/Users/sam/Documents/Job search";
    const FIVE: &str = include_str!("../../../tests/fixtures/setup/five_questions.md");
    const SIXTEEN: &str = include_str!("../../../tests/fixtures/setup/sixteen_changes.md");
    const UNNUMBERED: &str = include_str!("../../../tests/fixtures/setup/unnumbered.md");
    const PATH_QUESTION: &str = "Welcome to the AI Job Search setup!\n\nThree ways to start:\n\n**Path A: Documents folder**\n\nWhich would you like?";
    const FORK_WARNING: &str =
        "> **Heads-up before we start:** your `origin` points at `sam/job-search`, which is a\n> public GitHub fork. Want to continue with the setup?";
    const COMPLETE: &str = "**Setup complete!** Here's what was generated:\n\n- `CLAUDE.md` - Your full candidate profile\n\n**Try it out:**\n- Run `/scrape` to search for matching jobs right now";
    const APPLY_ALL: &str = "## Proposed Additive Changes\n\n- [ ] New certification: Harbor design - extracted from LinkedIn\n\n> **Apply all additive changes?** These add new content without touching anything already in the files.\n> Reply **yes** to apply all, or list the numbers you want to skip.";

    struct Fixture {
        desk: RunDesk,
        script: Rc<RefCell<Script>>,
        store: Arc<Memory>,
        shelf: Arc<Shelf>,
        profile: ProfileRun,
        cv: CvShelf,
        without_pdfs: bool,
        rules_testing: bool,
        /// The local time each poll reads.
        wall: NaiveDateTime,
        /// When the snapshot on show was read. `None` reads it on each poll, as a watcher that never lags would.
        read_at: Option<Instant>,
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
    }

    fn now() -> NaiveDateTime {
        today().and_hms_opt(9, 30, 0).unwrap()
    }

    fn fixture_with(progress: Progress, history: Option<&dyn RunHistory>) -> Fixture {
        let script = Rc::new(RefCell::new(Script::default()));
        let mut desk = RunDesk::new(Box::new(ScriptedGateway(Rc::clone(&script))), Arc::new(|| {}));
        if let Some(history) = history {
            desk.restore(history);
        }
        let store = Arc::new(Memory::default());
        let shelf = Arc::new(Shelf::default());
        let copier = Arc::clone(&shelf) as Arc<dyn DocumentCopier>;
        let profile = ProfileRun::new(Arc::clone(&store) as Arc<dyn ProgressStore>, progress, PathBuf::from(ROOT), copier, Arc::new(|| {}), today());
        Fixture { desk, script, store, shelf, profile, cv: CvShelf::default(), without_pdfs: false, rules_testing: false, wall: now(), read_at: None }
    }

    fn fixture() -> Fixture {
        fixture_with(Progress { folder: Some(PathBuf::from(ROOT)), ..Progress::default() }, None)
    }

    fn open() -> CapCheck {
        CapCheck::Open
    }

    fn rules<'a>(defaults: &'a RunDefaults, cap: &'a CapCheck) -> StartRules<'a> {
        StartRules { defaults, cap, testing: false }
    }

    impl Fixture {
        fn emit(&self, event: AgentEvent) {
            (self.script.borrow().sink.as_ref().unwrap())(event);
        }

        fn says(&self, text: &str) {
            self.emit(AgentEvent::AssistantText { text: text.into(), nested: false });
        }

        fn tool(&self, id: &str, name: &str, summary: &str) {
            self.emit(AgentEvent::ToolStarted { id: id.into(), name: name.into(), summary: summary.into(), nested: false });
            self.emit(AgentEvent::ToolFinished { id: id.into(), outcome: Ok(()) });
        }

        fn ends_turn(&self) {
            self.emit(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.4, duration_ms: 90_000 });
        }

        fn poll_at(&mut self, at: Instant) {
            let (defaults, cap) = (RunDefaults::default(), CapCheck::Open);
            let rules = StartRules { defaults: &defaults, cap: &cap, testing: self.rules_testing };
            let input = PollInput {
                cv: &self.cv,
                read_at: Some(self.read_at.unwrap_or(at)),
                rereads: true,
                without_pdfs: self.without_pdfs,
                rules,
                now: at,
                wall: self.wall,
            };
            self.desk.poll();
            self.profile.poll(&mut self.desk, &input);
        }

        fn poll(&mut self) {
            self.poll_at(Instant::now());
        }

        /// Polls until the documents' workers have nothing left to say.
        fn settle(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            self.poll();
            while self.profile.documents().is_looking() || self.profile.is_starting() {
                assert!(Instant::now() < deadline, "the workers never finished");
                thread::sleep(Duration::from_millis(2));
                self.poll();
            }
        }

        fn add(&mut self, name: &str) {
            self.profile.add_documents([PathBuf::from("/Users/sam/Desktop").join(name)]);
            self.settle();
        }

        fn read(&mut self, defaults: &RunDefaults, cap: &CapCheck) -> Result<(), StartRefusal> {
            let read = self.profile.read_documents(&mut self.desk, &rules(defaults, cap), now());
            self.settle();
            read
        }

        fn start(&mut self) -> String {
            self.add("Sam Rivera CV.pdf");
            self.read(&RunDefaults::default(), &open()).unwrap();
            self.profile.progress().run.clone().unwrap()
        }

        /// Started, the path question answered, and Claude's next turn ended on `message`.
        fn at(&mut self, message: &str) -> String {
            let id = self.start();
            self.says(PATH_QUESTION);
            self.ends_turn();
            self.poll();
            self.says(message);
            self.ends_turn();
            self.poll();
            id
        }

        fn sent(&self) -> Vec<String> {
            self.script.borrow().sent.clone()
        }

        fn record(&self) -> Progress {
            self.store.record.lock().unwrap().clone().unwrap()
        }

        fn pick_up(&mut self) -> Result<(), StartRefusal> {
            let picked = self.profile.pick_up(&mut self.desk, &rules(&RunDefaults::default(), &open()), now());
            self.settle();
            picked
        }
    }

    #[test]
    fn read_my_documents_copies_on_a_worker_then_starts_setup_with_the_launchers_values() {
        let mut f = fixture();
        assert_eq!(f.profile.phase(), &ProfilePhase::Documents);
        f.profile.add_documents([PathBuf::from("/Users/sam/Desktop/Sam Rivera CV.pdf")]);
        assert_eq!(f.profile.start_refusal(&f.desk, &rules(&RunDefaults::default(), &open())), Some(StartRefusal::Looking));
        f.settle();
        f.profile.read_documents(&mut f.desk, &rules(&RunDefaults::default(), &open()), now()).unwrap();
        assert_eq!(f.profile.phase(), &ProfilePhase::Copying);
        assert!(f.script.borrow().requests.is_empty(), "not before the copy ends");
        f.settle();
        assert_eq!(*f.shelf.0.lock().unwrap(), ["Sam Rivera CV.pdf"]);
        let request = &f.script.borrow().requests[0];
        assert_eq!((request.line.as_str(), request.model.as_deref(), request.budget_usd), (SETUP, None, 25.0), "the catalog's suggestion");
        assert_eq!(f.record().run, f.profile.progress().run);
        assert_eq!(f.profile.phase(), &ProfilePhase::Reading);
    }

    /// A copier that panics still ends Read my documents, so the gates that wait on it open again.
    #[test]
    fn a_copier_that_panics_ends_the_start_with_the_copy_failure() {
        let mut f = fixture();
        f.add("Sam Rivera CV.pdf");
        *f.shelf.1.lock().unwrap() = true;
        f.read(&RunDefaults::default(), &open()).unwrap();
        assert!(!f.profile.is_starting());
        let Some(StartRefusal::Copy(stop)) = f.profile.start_failure() else { panic!("expected the copy failure") };
        assert_eq!(stop.line(), "The copy stopped at Sam Rivera CV.pdf. Nothing was saved.");
        assert!(f.script.borrow().requests.is_empty(), "no run starts");
        assert_eq!(f.profile.phase(), &ProfilePhase::Documents);
    }

    #[test]
    fn a_command_row_or_a_budget_for_every_task_beats_the_suggestion() {
        let row = CommandDefault { command: SETUP.into(), model: ModelChoice::Sonnet, budget: Dollars::whole(12) };
        let every = RunDefaults { model: ModelChoice::Opus, budget: Some(Dollars::whole(40)), commands: vec![] };
        let both = RunDefaults { commands: vec![row], ..every.clone() };
        for (defaults, model, budget) in [(every, "opus", 40.0), (both, "sonnet", 12.0)] {
            let mut f = fixture();
            f.add("cv.pdf");
            f.profile.read_documents(&mut f.desk, &rules(&defaults, &open()), now()).unwrap();
            let (deadline, cap, cv) = (Instant::now() + Duration::from_secs(5), open(), CvShelf::default());
            while f.profile.is_starting() {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(2));
                f.desk.poll();
                let input = PollInput {
                    cv: &cv,
                    read_at: Some(Instant::now()),
                    rereads: true,
                    without_pdfs: false,
                    rules: rules(&defaults, &cap),
                    now: Instant::now(),
                    wall: now(),
                };
                f.profile.poll(&mut f.desk, &input);
            }
            let request = &f.script.borrow().requests[0];
            assert_eq!((request.model.as_deref(), request.budget_usd), (Some(model), budget));
        }
        assert!(f64::from(WORDS_BUDGET_USD) < 25.0);
    }

    #[test]
    fn the_path_a_reply_goes_once_when_claude_asks_for_the_path() {
        let mut f = fixture();
        f.start();
        f.says(PATH_QUESTION);
        f.poll();
        assert!(f.sent().is_empty(), "not while the turn works");
        f.ends_turn();
        f.poll();
        assert_eq!(f.sent(), [PATH_A_REPLY]);
        assert!(f.record().path_a_sent);
        assert_eq!(f.profile.phase(), &ProfilePhase::Reading, "the user never sees the path question");
        f.says(PATH_QUESTION);
        f.ends_turn();
        f.poll();
        f.poll();
        assert_eq!(f.sent(), [PATH_A_REPLY], "never twice, whatever Claude asks next");
        assert!(matches!(f.profile.phase(), ProfilePhase::FallbackSheet { .. }));
    }

    #[test]
    fn a_fork_warning_is_the_users_to_answer_and_the_path_follows_once() {
        let mut f = fixture();
        f.start();
        f.says(FORK_WARNING);
        f.ends_turn();
        f.poll();
        assert!(f.sent().is_empty(), "nothing is answered for the user");
        assert_eq!(f.profile.phase(), &ProfilePhase::FallbackSheet { message: FORK_WARNING.into() });
        assert!(!f.record().path_a_sent);
        f.profile.send_text(&mut f.desk, "Yes, continue.").unwrap();
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Reading, "the user's own reply to the warning");
        f.says(PATH_QUESTION);
        f.ends_turn();
        f.poll();
        assert_eq!(f.sent(), ["Yes, continue.", PATH_A_REPLY]);
        assert!(f.record().path_a_sent);
    }

    #[test]
    fn path_a_is_named_as_a_word_without_hidden_characters() {
        assert!(names_path_a("**Path\u{200b} A: Read my documents folder**"));
        assert!(names_path_a("path a - documents"));
        assert!(names_path_a("Which path? Path A"));
        assert!(!names_path_a("The path about your documents"));
        assert!(!names_path_a("Want to continue with the setup?"));
    }

    #[test]
    fn the_path_a_reply_waits_for_a_pending_prompt() {
        let mut f = fixture();
        f.desk.set_asks_permission(true);
        f.start();
        let permission = PromptKind::Permission { tool: "Bash".into(), detail: "ls documents".into(), can_remember: false };
        f.emit(AgentEvent::PromptRaised(Prompt { request_id: "p".into(), kind: permission, input_json: "{}".into(), suggestions_json: None }));
        f.says(PATH_QUESTION);
        f.ends_turn();
        f.poll();
        assert!(f.sent().is_empty(), "the prompt reaches the user, and nothing is answered for them");
        assert_eq!(f.profile.phase(), &ProfilePhase::Choice);
        f.profile.decide(&mut f.desk, "p", Decision::Allow { remember: false });
        f.poll();
        assert_eq!(f.sent(), ["Allow { remember: false }", PATH_A_REPLY]);
    }

    struct Restored(Run);

    impl RunHistory for Restored {
        fn recent(&self, _: usize) -> Vec<Run> {
            vec![self.0.clone()]
        }
    }

    /// The run as the log reads it back after a restart.
    fn read_back(run: &Run) -> Run {
        let mut run = run.clone();
        run.settle_after_restore();
        run
    }

    #[test]
    fn the_path_a_reply_is_not_sent_again_after_a_restore() {
        let mut f = fixture();
        let id = f.start();
        f.says(PATH_QUESTION);
        f.ends_turn();
        f.poll();
        f.says(PATH_QUESTION);
        f.ends_turn();
        f.poll();
        let restored = Restored(read_back(f.desk.run(&id).unwrap()));
        let mut again = fixture_with(f.record(), Some(&restored));
        again.poll();
        assert!(again.script.borrow().requests.is_empty() && again.sent().is_empty(), "no process starts and nothing is sent");
        assert_eq!(again.profile.run(&again.desk).map(|run| run.id.as_str()), Some(id.as_str()), "the run is found by its id");
        // The record lost the flag, and the log still has the reply: the run holds it back.
        let mut lost = fixture_with(Progress { path_a_sent: false, ..f.record() }, Some(&restored));
        lost.poll();
        assert!(matches!(lost.profile.phase(), ProfilePhase::FallbackSheet { .. }));
        lost.profile.send_text(&mut lost.desk, "Path A, please.").unwrap();
        lost.says(PATH_QUESTION);
        lost.ends_turn();
        lost.poll();
        assert_eq!(lost.script.borrow().requests.len(), 1);
        assert!(lost.sent().is_empty(), "the reply in the log counts as sent");
    }

    #[test]
    fn a_run_restored_while_claude_asked_for_the_path_waits_and_pick_up_sends_it() {
        let mut f = fixture();
        let id = f.start();
        f.says(PATH_QUESTION);
        f.ends_turn();
        f.emit(AgentEvent::ProcessExited { failure: None });
        f.desk.poll();
        let restored = Restored(read_back(f.desk.run(&id).unwrap()));
        let mut again = fixture_with(Progress { path_a_sent: false, ..f.record() }, Some(&restored));
        again.poll();
        assert!(again.script.borrow().requests.is_empty(), "nothing starts on its own");
        assert_eq!(again.profile.phase(), &ProfilePhase::Stopped { reason: STOPPED_BEFORE_READING.into() });
        again.pick_up().unwrap();
        let requests = &again.script.borrow().requests;
        assert_eq!((requests.len(), requests[0].line.as_str(), requests[0].resume_session.as_deref()), (1, PATH_A_REPLY, Some("s1")));
        assert!(again.record().path_a_sent);
    }

    #[test]
    fn a_run_that_never_reported_a_session_starts_setup_anew_on_pick_up() {
        let mut f = fixture();
        f.script.borrow_mut().silent = true;
        let old = f.start();
        f.emit(AgentEvent::ProcessExited { failure: Some("The saved API key is gone.".into()) });
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Stopped { reason: "The saved API key is gone.".into() });
        f.script.borrow_mut().silent = false;
        f.rules_testing = true;
        assert_eq!(f.profile.pick_up(&mut f.desk, &StartRules { testing: true, ..rules(&RunDefaults::default(), &open()) }, now()), Err(StartRefusal::Testing));
        f.rules_testing = false;
        f.pick_up().unwrap();
        let requests = &f.script.borrow().requests;
        assert_eq!(requests.len(), 2);
        assert_eq!((requests[1].line.as_str(), requests[1].resume_session.as_deref()), (SETUP, None), "/setup itself, not a bare reply");
        let new = f.record().run.unwrap();
        assert_ne!(new, old);
        assert!(!f.record().path_a_sent);
        assert_eq!(*f.shelf.0.lock().unwrap(), ["Sam Rivera CV.pdf"], "copied once");
    }

    #[test]
    fn a_second_start_while_a_run_works_is_refused() {
        let mut f = fixture();
        f.start();
        assert_eq!(f.read(&RunDefaults::default(), &open()), Err(StartRefusal::AlreadyStarted));

        let mut other = fixture();
        other.desk.start("/rank", &StartOptions { model: None, budget_usd: 3.0 }, now()).unwrap();
        other.add("cv.pdf");
        assert_eq!(other.read(&RunDefaults::default(), &open()), Err(StartRefusal::Busy));
        assert!(other.shelf.0.lock().unwrap().is_empty(), "nothing is copied for a run that cannot start");
        assert_eq!(other.script.borrow().requests.len(), 1);

        let mut testing = fixture();
        testing.add("cv.pdf");
        let rules = StartRules { defaults: &RunDefaults::default(), cap: &CapCheck::Open, testing: true };
        assert_eq!(testing.profile.read_documents(&mut testing.desk, &rules, now()), Err(StartRefusal::Testing));
    }

    #[test]
    fn a_run_that_another_starts_meanwhile_is_held_back_after_the_copy() {
        let mut f = fixture();
        f.add("cv.pdf");
        f.profile.read_documents(&mut f.desk, &rules(&RunDefaults::default(), &open()), now()).unwrap();
        f.rules_testing = true;
        f.settle();
        assert_eq!(f.profile.start_failure(), Some(&StartRefusal::Testing));
        assert!(f.script.borrow().requests.is_empty());
        assert_eq!(f.profile.phase(), &ProfilePhase::Documents);
        f.rules_testing = false;
        assert_eq!(f.read(&RunDefaults::default(), &open()), Ok(()));
        assert_eq!(f.script.borrow().requests.len(), 1);
        assert_eq!(*f.shelf.0.lock().unwrap(), ["cv.pdf"], "copied once");
    }

    #[test]
    fn no_documents_no_start() {
        let mut f = fixture();
        assert_eq!(f.read(&RunDefaults::default(), &open()), Err(StartRefusal::NoDocuments));
        f.add("Reference.docx");
        assert_eq!(f.profile.start_refusal(&f.desk, &rules(&RunDefaults::default(), &open())), Some(StartRefusal::NoDocuments));
    }

    #[test]
    fn the_cap_is_refused_without_a_figure() {
        let reached = CapCheck::Reached(CapReached { spent_usd: 61.25, cap: Dollars::whole(60), reopens_on: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap() });
        for cap in [reached, CapCheck::Reading] {
            let mut f = fixture();
            f.add("cv.pdf");
            let refused = f.read(&RunDefaults::default(), &cap).unwrap_err();
            let line = refused.line();
            assert!(!line.contains('$') && !line.contains("60") && !line.contains("61"), "{line}");
            assert!(f.script.borrow().requests.is_empty() && f.shelf.0.lock().unwrap().is_empty());
        }
        assert_eq!(
            StartRefusal::CapReached { reopens_on: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap() }.line(),
            "This month's tasks reached the monthly cap you set in Settings. Setup can start on 1\u{a0}December."
        );
    }

    #[test]
    fn numbered_questions_open_a_sheet_and_send_one_joined_reply() {
        let mut f = fixture();
        f.at(FIVE);
        let ProfilePhase::Sheet { batch, message } = f.profile.phase().clone() else { panic!("{:?}", f.profile.phase()) };
        assert_eq!((batch.numbers(), message.as_str()), (vec![1, 2, 3, 4, 5], FIVE));
        f.profile.send_answers(&mut f.desk, &[(1, "Design Lead"), (3, "Lowmere, remote preferred")]).unwrap();
        assert_eq!(f.sent().last().unwrap(), "1. Design Lead\n2. Skipped\n3. Lowmere, remote preferred\n4. Skipped\n5. Skipped");
        assert_eq!(f.profile.send_answers(&mut f.desk, &[]), Err(SendRefusal::NotNow), "once");
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Working, "a turn the user started");
    }

    #[test]
    fn a_proposal_opens_the_review_and_sends_the_except_list() {
        let mut f = fixture();
        f.at(SIXTEEN);
        let ProfilePhase::Review { batch, .. } = f.profile.phase().clone() else { panic!("{:?}", f.profile.phase()) };
        let none: Vec<(u32, bool)> = batch.numbers().into_iter().map(|number| (number, false)).collect();
        assert_eq!(f.profile.send_review(&mut f.desk, &none), Err(SendRefusal::NoReply));
        let ticks: Vec<(u32, bool)> = batch.numbers().into_iter().map(|number| (number, number != 3 && number != 7)).collect();
        f.profile.send_review(&mut f.desk, &ticks).unwrap();
        assert_eq!(f.sent().last().unwrap(), "Apply all except 3 and 7.");
    }

    #[test]
    fn a_message_without_numbers_is_never_done_with_or_without_pdfs() {
        for without_pdfs in [false, true] {
            for message in [UNNUMBERED, APPLY_ALL, "No cross-reference issues found."] {
                let mut f = fixture();
                f.without_pdfs = without_pdfs;
                f.at(message);
                assert_eq!(f.profile.phase(), &ProfilePhase::FallbackSheet { message: message.into() }, "{without_pdfs} {message}");
            }
            let mut after_check = fixture();
            after_check.without_pdfs = without_pdfs;
            after_check.at(UNNUMBERED);
            after_check.profile.send_text(&mut after_check.desk, CV_LOOKS_GOOD).unwrap();
            after_check.says(UNNUMBERED);
            after_check.ends_turn();
            after_check.poll();
            assert!(matches!(after_check.profile.phase(), ProfilePhase::FallbackSheet { .. }), "not even after the CV looks good");
        }
        let mut f = fixture();
        f.at(UNNUMBERED);
        assert_eq!(f.profile.send_text(&mut f.desk, "  \n"), Err(SendRefusal::Empty));
        f.profile.send_text(&mut f.desk, "  Product design, in Lowmere.  ").unwrap();
        assert_eq!(f.sent().last().unwrap(), "Product design, in Lowmere.");
    }

    #[test]
    fn setup_complete_is_done_with_latex_and_no_pdf_built() {
        for without_pdfs in [false, true] {
            let mut f = fixture();
            f.without_pdfs = without_pdfs;
            f.at(FIVE);
            f.profile.send_answers(&mut f.desk, &[]).unwrap();
            f.tool("w", "Write", &format!("{ROOT}/cv/main_example.tex"));
            f.says("**Setup\u{200b} COMPLETE!** Here's what was generated.");
            f.ends_turn();
            f.poll();
            assert_eq!(f.profile.phase(), &ProfilePhase::Done, "no PDF changed, so no CV check: {without_pdfs}");
        }
    }

    fn cv_with_pdf(at: NaiveDateTime) -> CvShelf {
        let pdf = FileStamp { name: "main_example.pdf".into(), path: PathBuf::from("cv/main_example.pdf"), modified: 1_000, at, bytes: 80_000 };
        cv_shelf(vec![CvFiles { stem: "main_example".into(), tex: None, pdf: Some(pdf) }], false)
    }

    #[test]
    fn a_cv_built_this_run_shows_its_check_once_before_done() {
        let mut f = fixture();
        f.at(FIVE);
        f.profile.send_answers(&mut f.desk, &[]).unwrap();
        f.tool("w", "Write", &format!("{ROOT}/cv/main_example.tex"));
        f.tool("b", "Bash", "cd cv && lualatex main_example.tex");
        f.says(COMPLETE);
        f.ends_turn();
        f.cv = cv_with_pdf(now() - chrono::TimeDelta::days(1));
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Done, "a PDF from before the run is no CV check");
        f.cv = cv_with_pdf(now() + chrono::TimeDelta::minutes(30));
        f.without_pdfs = true;
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Done, "without PDFs there is no CV check");
        f.without_pdfs = false;
        f.poll();
        let ProfilePhase::CvCheck { pdf, .. } = f.profile.phase().clone() else { panic!("{:?}", f.profile.phase()) };
        assert_eq!(pdf.path, PathBuf::from("cv/main_example.pdf"));

        f.profile.cv_looks_good(&mut f.desk).unwrap();
        assert_eq!(f.sent().last().unwrap(), CV_LOOKS_GOOD);
        f.says("Glad it reads well.");
        f.ends_turn();
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Done, "the CV check answered the turn that said setup is complete");
    }

    /// At the end of a turn that built the CV and said setup is complete, before any poll saw it end. The snapshot on
    /// show was read at `read`, and holds no PDF.
    fn complete_after_a_build(f: &mut Fixture, read: Instant) {
        f.at(FIVE);
        f.profile.send_answers(&mut f.desk, &[]).unwrap();
        f.poll();
        f.read_at = Some(read);
        f.tool("b", "Bash", "cd cv && lualatex main_example.tex");
        f.says(COMPLETE);
        f.ends_turn();
    }

    /// The snapshot on show may predate the turn's last write, as when the watcher lags. Done, and the CV check that
    /// comes before it, wait for a snapshot read after the turn ended, or for `SNAPSHOT_WAIT` at most.
    #[test]
    fn the_end_waits_for_a_snapshot_read_after_the_turn_ended() {
        let base = Instant::now();
        let mut f = fixture();
        complete_after_a_build(&mut f, base);
        f.poll_at(base + Duration::from_millis(10));
        assert_eq!(f.profile.phase(), &ProfilePhase::Working, "the snapshot was read before the turn ended");
        assert_eq!(f.profile.snapshot_wait(base + Duration::from_millis(10)), Some(SNAPSHOT_WAIT));
        // The next snapshot, read after the turn ended, holds the PDF the turn built.
        f.cv = cv_with_pdf(now() + chrono::TimeDelta::minutes(30));
        f.read_at = Some(base + Duration::from_millis(20));
        f.poll_at(base + Duration::from_millis(30));
        assert!(matches!(f.profile.phase(), ProfilePhase::CvCheck { .. }), "{:?}", f.profile.phase());
        assert_eq!(f.profile.snapshot_wait(base + Duration::from_millis(30)), None);

        let mut late = fixture();
        complete_after_a_build(&mut late, base);
        late.poll_at(base + Duration::from_millis(10));
        late.poll_at(base + Duration::from_millis(10) + SNAPSHOT_WAIT - Duration::from_millis(1));
        assert_eq!(late.profile.phase(), &ProfilePhase::Working);
        late.poll_at(base + Duration::from_millis(10) + SNAPSHOT_WAIT);
        assert_eq!(late.profile.phase(), &ProfilePhase::Done, "never longer than SNAPSHOT_WAIT");
    }

    /// With no worker to read the folder again, the snapshot on show is the newest, and the end comes at once.
    #[test]
    fn with_no_worker_to_read_again_the_end_comes_at_once() {
        let base = Instant::now();
        let mut f = fixture();
        complete_after_a_build(&mut f, base);
        f.desk.poll();
        let (defaults, cap) = (RunDefaults::default(), CapCheck::Open);
        let rules = StartRules { defaults: &defaults, cap: &cap, testing: false };
        let at = base + Duration::from_millis(10);
        let input = PollInput { cv: &f.cv, read_at: Some(base), rereads: false, without_pdfs: false, rules, now: at, wall: f.wall };
        f.profile.poll(&mut f.desk, &input);
        assert_eq!(f.profile.phase(), &ProfilePhase::Done);
    }

    #[test]
    fn a_cv_check_before_the_end_leads_on_to_the_next_turn() {
        let mut f = fixture();
        f.at(FIVE);
        f.profile.send_answers(&mut f.desk, &[]).unwrap();
        f.tool("b", "Bash", "cd cv && lualatex main_example.tex");
        f.says("Your CV is built. Have a look at cv/main_example.pdf.");
        f.ends_turn();
        f.cv = cv_with_pdf(now() + chrono::TimeDelta::minutes(30));
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::CvCheck { .. }));
        f.profile.cv_looks_good(&mut f.desk).unwrap();
        f.says("Now the search terms. Which cities?");
        f.ends_turn();
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::FallbackSheet { .. }), "the end comes only when setup says so");
        f.profile.send_text(&mut f.desk, "Lowmere").unwrap();
        f.says(COMPLETE);
        f.ends_turn();
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Done);
    }

    /// "Setup complete" that closes on a question leaves the session waiting on the user, so it is no end: the fallback
    /// sheet shows, with Finish setup.
    #[test]
    fn setup_complete_that_closes_on_a_question_is_the_fallback_sheet() {
        for message in [
            "**Setup complete!** Here's what was generated:\n\n- `CLAUDE.md` - Your full candidate profile\n\nWant me to compile your CV to PDF now?",
            "Setup complete! Would you like to change your target roles?",
        ] {
            let mut f = fixture();
            f.at(message);
            assert_eq!(f.profile.phase(), &ProfilePhase::FallbackSheet { message: message.into() }, "{message}");
            assert!(f.profile.complete_seen(&f.desk), "Finish setup shows");
            let restored = Restored(read_back(f.desk.run(f.profile.progress().run.as_deref().unwrap()).unwrap()));
            let mut again = fixture_with(f.record(), Some(&restored));
            again.poll();
            assert!(matches!(again.profile.phase(), ProfilePhase::FallbackSheet { .. }), "nor after a restore: {message}");
        }
    }

    #[test]
    fn setup_complete_is_whole_words() {
        assert!(says_complete("**Setup complete!** Here's what was generated."));
        assert!(says_complete("Setup\u{200b} COMPLETE."));
        assert!(says_complete("setup complete"));
        for text in
            ["A few last questions so setup completes properly.", "Once setup completed, run /scrape.", "Setup completion takes a while.", "resetup complete"]
        {
            assert!(!says_complete(text), "{text}");
        }
    }

    #[test]
    fn numbered_questions_are_never_done_whatever_they_say() {
        for message in [
            "A few last questions so setup completes properly:\n\n1. Which city do you live in?\n2. What salary would you accept?",
            "Setup complete, almost. Two last checks:\n\n1. Is Lowmere right?\n2. Is the salary right?",
        ] {
            let mut f = fixture();
            f.at(message);
            assert!(matches!(f.profile.phase(), ProfilePhase::Sheet { .. }), "{message}: {:?}", f.profile.phase());
        }
        let mut proposal = fixture();
        proposal.at(&format!("Setup complete once these are in.\n\n{SIXTEEN}"));
        assert!(matches!(proposal.profile.phase(), ProfilePhase::Review { .. }));
    }

    /// At a turn that said setup is complete and built the CV, with its check showing.
    fn at_complete_with_a_cv(f: &mut Fixture) -> String {
        let id = f.at(FIVE);
        f.profile.send_answers(&mut f.desk, &[]).unwrap();
        f.tool("b", "Bash", "cd cv && lualatex main_example.tex");
        f.says(COMPLETE);
        f.ends_turn();
        f.cv = cv_with_pdf(now() + chrono::TimeDelta::minutes(30));
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::CvCheck { .. }));
        id
    }

    #[test]
    fn a_change_asked_at_the_cv_check_keeps_setup_complete_for_the_looks_good_after_it() {
        let mut f = fixture();
        at_complete_with_a_cv(&mut f);
        f.profile.send_text(&mut f.desk, "Put my phone number on the first line.").unwrap();
        f.tool("w", "Edit", &format!("{ROOT}/cv/main_example.tex"));
        f.tool("b2", "Bash", "cd cv && lualatex main_example.tex");
        f.says("I moved the phone number. Have another look.");
        f.ends_turn();
        f.cv = cv_with_pdf(now() + chrono::TimeDelta::minutes(40));
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::CvCheck { .. }), "the rebuilt PDF");
        f.profile.cv_looks_good(&mut f.desk).unwrap();
        f.says("Glad it reads well.");
        f.ends_turn();
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Done);
    }

    #[test]
    fn the_cv_check_never_shows_twice_for_the_same_pdf_also_after_a_restore() {
        let mut f = fixture();
        let id = at_complete_with_a_cv(&mut f);
        let unanswered = Restored(read_back(f.desk.run(&id).unwrap()));
        let mut again = fixture_with(f.record(), Some(&unanswered));
        again.cv = f.cv.clone();
        again.poll();
        assert!(matches!(again.profile.phase(), ProfilePhase::CvCheck { .. }), "a check not answered yet shows again");

        f.profile.send_text(&mut f.desk, "Shorter, please.").unwrap();
        f.tool("w", "Edit", &format!("{ROOT}/cv/main_example.tex"));
        f.says("I shortened the summary in the source. Shall I go on?");
        f.ends_turn();
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::FallbackSheet { .. }), "no build, so the same PDF: no second check");
        let restored = Restored(read_back(f.desk.run(&id).unwrap()));
        let mut after = fixture_with(f.record(), Some(&restored));
        after.cv = f.cv.clone();
        after.poll();
        assert!(matches!(after.profile.phase(), ProfilePhase::FallbackSheet { .. }), "nor after a restore");
    }

    #[test]
    fn a_question_after_the_cv_looks_good_is_not_the_end() {
        let mut f = fixture();
        at_complete_with_a_cv(&mut f);
        f.profile.cv_looks_good(&mut f.desk).unwrap();
        let question = "Glad you like it. Would you also like me to tailor the cover letter template to it?";
        f.says(question);
        f.ends_turn();
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::FallbackSheet { message: question.into() });
        let restored = Restored(read_back(f.desk.run(f.profile.progress().run.as_deref().unwrap()).unwrap()));
        let mut again = fixture_with(f.record(), Some(&restored));
        again.poll();
        assert!(matches!(again.profile.phase(), ProfilePhase::FallbackSheet { .. }), "nor after a restore");
        f.profile.send_text(&mut f.desk, "No, thank you.").unwrap();
        f.says("Then setup is done. Run Find new jobs next.");
        f.ends_turn();
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::FallbackSheet { .. }), "only the turn that answers the CV check can end it");
    }

    #[test]
    fn numbered_questions_in_a_turn_that_built_the_cv_open_the_sheet() {
        let mut f = fixture();
        f.at(FIVE);
        f.profile.send_answers(&mut f.desk, &[]).unwrap();
        f.tool("b", "Bash", "cd cv && lualatex main_example.tex");
        f.says(FIVE);
        f.ends_turn();
        f.cv = cv_with_pdf(now() + chrono::TimeDelta::minutes(30));
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::Sheet { .. }), "{:?}", f.profile.phase());
        let mut proposal = fixture();
        proposal.at(FIVE);
        proposal.profile.send_answers(&mut proposal.desk, &[]).unwrap();
        proposal.tool("b", "Bash", "cd cv && lualatex main_example.tex");
        proposal.says(SIXTEEN);
        proposal.ends_turn();
        proposal.cv = cv_with_pdf(now() + chrono::TimeDelta::minutes(30));
        proposal.poll();
        assert!(matches!(proposal.profile.phase(), ProfilePhase::Review { .. }));
    }

    #[test]
    fn a_failed_rebuild_shows_no_second_check_on_the_pdf_already_answered() {
        let mut f = fixture();
        at_complete_with_a_cv(&mut f);
        f.wall = now() + chrono::TimeDelta::minutes(35);
        f.profile.send_text(&mut f.desk, "Put my phone number on the first line.").unwrap();
        f.poll();
        f.emit(AgentEvent::ToolStarted { id: "b2".into(), name: "Bash".into(), summary: "cd cv && lualatex main_example.tex".into(), nested: false });
        f.emit(AgentEvent::ToolFinished { id: "b2".into(), outcome: Err(Some("! LaTeX Error".into())) });
        f.says("The build failed. Shall I try another way?");
        f.ends_turn();
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::FallbackSheet { .. }), "the PDF is the one answered: {:?}", f.profile.phase());
        f.profile.send_text(&mut f.desk, "Yes.").unwrap();
        f.poll();
        f.tool("b3", "Bash", "cd cv && xelatex main_example.tex");
        f.says("Built with XeLaTeX. Have a look.");
        f.ends_turn();
        f.cv = cv_with_pdf(now() + chrono::TimeDelta::minutes(40));
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::CvCheck { .. }), "a PDF built in this turn");
    }

    #[test]
    fn a_turn_that_wrote_no_cv_is_no_cv_check_and_the_making_line_is_the_handoffs() {
        assert!(PROGRESS_LINES.iter().any(|line| line.text == MAKING_CV));
        let mut f = fixture();
        f.cv = cv_with_pdf(now() + chrono::TimeDelta::minutes(30));
        f.at("Your CV is built.");
        assert!(matches!(f.profile.phase(), ProfilePhase::FallbackSheet { .. }));
    }

    #[test]
    fn a_failure_or_a_stop_shows_its_reason_as_plain_text_and_pick_up_replies() {
        let mut f = fixture();
        let id = f.at(FIVE);
        f.profile.send_answers(&mut f.desk, &[]).unwrap();
        f.emit(AgentEvent::TurnEnded { failed: true, message: Some("Budget cap reached.\u{202e}".into()), cost_usd: 25.0, duration_ms: 1 });
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Stopped { reason: "Budget cap reached.".into() });
        f.pick_up().unwrap();
        let requests = &f.script.borrow().requests;
        assert_eq!((requests.len(), requests[1].line.as_str(), requests[1].resume_session.as_deref()), (2, PICK_UP_REPLY, Some("s1")), "a new process");
        assert!(f.desk.has_session(&id));

        let mut stopped = fixture();
        stopped.at(FIVE);
        stopped.profile.send_answers(&mut stopped.desk, &[]).unwrap();
        stopped.poll();
        stopped.profile.stop(&mut stopped.desk);
        assert!(stopped.script.borrow().interrupted);
        stopped.ends_turn();
        stopped.poll();
        assert_eq!(stopped.profile.phase(), &ProfilePhase::Stopped { reason: YOU_STOPPED.into() });
        stopped.pick_up().unwrap();
        assert_eq!(stopped.sent().last().unwrap(), PICK_UP_REPLY, "on the same session");
    }

    #[test]
    fn a_question_from_claude_is_the_choice_and_waits_for_the_user() {
        let mut f = fixture();
        f.at(FIVE);
        let question = PromptKind::Questions(vec![]);
        f.emit(AgentEvent::PromptRaised(Prompt { request_id: "q".into(), kind: question, input_json: "{}".into(), suggestions_json: None }));
        f.poll();
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Choice);
        assert_eq!(f.sent(), [PATH_A_REPLY], "nothing answers it but the user");
        assert_eq!(f.profile.send_answers(&mut f.desk, &[]), Err(SendRefusal::NotNow));
    }

    #[test]
    fn a_sheet_open_at_quit_opens_again_after_a_restore() {
        let mut f = fixture();
        let id = f.at(FIVE);
        let restored = Restored(read_back(f.desk.run(&id).unwrap()));
        let mut again = fixture_with(f.record(), Some(&restored));
        again.poll();
        assert!(matches!(again.profile.phase(), ProfilePhase::Sheet { .. }));
        again.profile.send_answers(&mut again.desk, &[(1, "Design Lead")]).unwrap();
        let request = &again.script.borrow().requests[0];
        assert_eq!((request.resume_session.as_deref(), request.line.starts_with("1. Design Lead")), (Some("s1"), true), "the same session");
    }

    #[test]
    fn a_turn_cut_off_is_stopped_whatever_it_said_last() {
        for (without_pdfs, last) in [(false, Some("Let me update the skill files.")), (false, None), (true, Some(COMPLETE)), (false, Some(FIVE))] {
            let mut f = fixture();
            f.without_pdfs = without_pdfs;
            let id = f.at(FIVE);
            f.profile.send_answers(&mut f.desk, &[]).unwrap();
            if let Some(last) = last {
                f.says(last);
            }
            f.emit(AgentEvent::ProcessExited { failure: None });
            f.poll();
            assert_eq!(f.profile.phase(), &ProfilePhase::Stopped { reason: STOPPED_PARTWAY.into() }, "live: {last:?}");
            let restored = Restored(read_back(f.desk.run(&id).unwrap()));
            let mut again = fixture_with(f.record(), Some(&restored));
            again.without_pdfs = without_pdfs;
            again.poll();
            assert_eq!(again.profile.phase(), &ProfilePhase::Stopped { reason: STOPPED_PARTWAY.into() }, "restored: {last:?}");
            again.pick_up().unwrap();
            assert_eq!(again.script.borrow().requests[0].line, PICK_UP_REPLY);
        }
    }

    #[test]
    fn a_draft_is_keyed_by_run_and_turn_saved_at_most_once_a_second_and_cleared_by_send() {
        let mut f = fixture();
        let id = f.at(FIVE);
        let saves = |f: &Fixture| *f.store.saves.lock().unwrap();
        let base = Instant::now();
        let before = saves(&f);
        f.profile.record_draft(&f.desk, vec![(1, "Design".into())], String::new(), base);
        assert_eq!(saves(&f), before + 1, "the first change is written at once");
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into())], String::new(), base + Duration::from_millis(400));
        assert_eq!(saves(&f), before + 1, "not again within the second");
        assert!(f.profile.is_unsaved());
        f.profile.save_draft_if_due(base + Duration::from_millis(900));
        assert_eq!(saves(&f), before + 1);
        f.profile.save_draft_if_due(base + Duration::from_millis(1_400));
        assert_eq!(saves(&f), before + 2);
        assert_eq!(f.profile.last_draft_save(), Some(base + Duration::from_millis(1_400)));
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into()), (2, "Mine".into())], String::new(), base + Duration::from_millis(1_500));
        f.profile.flush();
        assert_eq!(saves(&f), before + 3, "a quit writes what waits");
        assert!(!f.profile.is_unsaved());
        let kept = f.record().draft.unwrap();
        assert_eq!((kept.run.as_str(), kept.turn, kept.answers.len()), (id.as_str(), 2, 2));
        assert_eq!(f.profile.draft(&f.desk), Some(&kept));

        f.profile.send_answers(&mut f.desk, &[(1, "Design Lead"), (2, "Mine")]).unwrap();
        assert_eq!(f.record().draft, None, "cleared once Send succeeded");
        assert_eq!(f.profile.draft(&f.desk), None);
    }

    #[test]
    fn a_draft_the_record_could_not_take_stays_unsaved_and_says_why() {
        for (disk, trouble) in [(Disk::Full, SaveTrouble::CouldNotWrite), (Disk::TooLong, SaveTrouble::TooLong)] {
            let mut f = fixture();
            f.at(FIVE);
            let base = Instant::now();
            *f.store.disk.lock().unwrap() = disk;
            f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into())], String::new(), base);
            assert!(f.profile.is_unsaved());
            assert_eq!(f.profile.save_trouble(), Some(trouble));
            assert_eq!(f.profile.last_draft_save(), None, "the draft never reached the record");
            let tries = *f.store.saves.lock().unwrap();
            f.profile.flush();
            assert_eq!(*f.store.saves.lock().unwrap(), tries + 1, "a quit tries again");
            assert!(f.profile.is_unsaved());
            *f.store.disk.lock().unwrap() = Disk::Fine;
            f.profile.record_draft(&f.desk, vec![(1, "Design Lead, systems".into())], String::new(), base + Duration::from_secs(70));
            assert!(!f.profile.is_unsaved());
            assert_eq!(f.profile.save_trouble(), None);
            assert_eq!(f.profile.last_draft_save(), Some(base + Duration::from_secs(70)));
            assert!(f.record().draft.is_some());
        }
    }

    /// A draft too long for the record is not written again until it changes: the same draft would fail the same way.
    #[test]
    fn a_draft_too_long_waits_for_a_change_before_it_is_tried_again() {
        let mut f = fixture();
        f.at(FIVE);
        let base = Instant::now();
        *f.store.disk.lock().unwrap() = Disk::TooLong;
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into())], String::new(), base);
        let tries = *f.store.saves.lock().unwrap();
        for seconds in [2, 10, 120] {
            f.poll_at(base + Duration::from_secs(seconds));
        }
        assert_eq!(*f.store.saves.lock().unwrap(), tries, "nothing is written while the draft stays the same");
        assert_eq!(f.profile.next_draft_try(base + Duration::from_secs(120)), None);
        f.profile.record_draft(&f.desk, vec![(1, "Design".into())], String::new(), base + Duration::from_secs(121));
        assert_eq!(*f.store.saves.lock().unwrap(), tries + 1, "a change is tried");
    }

    /// A record that cannot be written is tried again with a growing wait, doubling up to a minute.
    #[test]
    fn a_record_that_cannot_be_written_is_tried_again_less_and_less_often() {
        let mut f = fixture();
        f.at(FIVE);
        let base = Instant::now();
        *f.store.disk.lock().unwrap() = Disk::Full;
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into())], String::new(), base);
        let saves = |f: &Fixture| *f.store.saves.lock().unwrap();
        let first = saves(&f);
        f.poll_at(base + Duration::from_millis(1_500));
        assert_eq!(saves(&f), first, "not after one second: the wait doubled to two");
        f.poll_at(base + Duration::from_millis(2_100));
        assert_eq!(saves(&f), first + 1);
        f.poll_at(base + Duration::from_millis(5_000));
        assert_eq!(saves(&f), first + 1, "then four seconds");
        f.poll_at(base + Duration::from_millis(6_200));
        assert_eq!(saves(&f), first + 2);
        let mut at = base + Duration::from_millis(6_200);
        for _ in 0..10 {
            at += Duration::from_secs(61);
            f.poll_at(at);
        }
        assert_eq!(saves(&f), first + 12, "never more than a minute apart");
        assert_eq!(f.profile.next_draft_try(at), Some(Duration::from_secs(60)), "the wait stops at a minute");
    }

    #[test]
    fn a_draft_of_another_turn_is_not_shown() {
        let mut f = fixture();
        let id = f.at(FIVE);
        let stale = Draft { run: id.clone(), turn: 1, answers: vec![(1, "old".into())], text: String::new() };
        let mut again = fixture_with(Progress { draft: Some(stale), ..f.record() }, Some(&Restored(read_back(f.desk.run(&id).unwrap()))));
        again.poll();
        assert_eq!(again.profile.draft(&again.desk), None);
        f.profile.record_draft(&f.desk, vec![(1, " ".into())], "\n".into(), Instant::now());
        assert_eq!(f.record().draft, None, "an emptied draft is dropped");
    }

    /// A change asked at the CV check that Claude answers without a build leaves no clean end. Finish setup ends it then,
    /// only when the user presses it, and only once a turn said setup is complete.
    #[test]
    fn finish_setup_ends_only_on_the_users_press_after_setup_said_complete() {
        let mut f = fixture();
        f.at(UNNUMBERED);
        assert!(!f.profile.complete_seen(&f.desk));
        assert!(!f.profile.finish(&f.desk), "nothing said setup is complete");
        assert!(matches!(f.profile.phase(), ProfilePhase::FallbackSheet { .. }));

        let mut f = fixture();
        at_complete_with_a_cv(&mut f);
        assert!(!f.profile.finish(&f.desk), "not from the CV check");
        f.profile.send_text(&mut f.desk, "Put my phone number on the first line.").unwrap();
        f.tool("w", "Edit", &format!("{ROOT}/cv/main_example.tex"));
        f.says("I moved it in the source. Anything else?");
        f.ends_turn();
        f.poll();
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::FallbackSheet { .. }), "setup never ends on its own");
        assert!(f.profile.complete_seen(&f.desk));
        assert!(f.profile.finish(&f.desk));
        f.poll();
        assert_eq!(f.profile.phase(), &ProfilePhase::Done, "and it stays done");
        assert_eq!(f.sent().last().unwrap(), "Put my phone number on the first line.", "Finish setup sends nothing");
    }

    /// Stop setup while Claude waits ends the session's process, keeps the draft, and the next send resumes the session.
    #[test]
    fn stop_setup_on_a_waiting_sheet_ends_the_process_and_keeps_the_draft() {
        let mut f = fixture();
        let id = f.at(FIVE);
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into())], String::new(), Instant::now());
        f.profile.stop(&mut f.desk);
        assert!(!f.desk.has_session(&id), "nothing stays open");
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::Sheet { .. }), "{:?}", f.profile.phase());
        assert!(f.record().draft.is_some(), "the draft is kept");
        assert!(f.profile.draft(&f.desk).is_some());
        f.profile.send_answers(&mut f.desk, &[(1, "Design Lead")]).unwrap();
        let requests = &f.script.borrow().requests;
        assert_eq!((requests.len(), requests[1].resume_session.as_deref()), (2, Some("s1")), "the same session, in a new process");
    }

    #[test]
    fn the_recap_clears_the_record_and_nothing_writes_it_again() {
        let mut f = fixture();
        f.at(FIVE);
        f.profile.clear_record();
        assert!(f.store.record.lock().unwrap().is_none());
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into())], String::new(), Instant::now() + Duration::from_secs(5));
        f.profile.flush();
        f.poll();
        assert!(f.store.record.lock().unwrap().is_none(), "the record stays gone");
    }

    #[test]
    fn a_refused_send_keeps_the_draft() {
        let mut f = fixture();
        f.at(FIVE);
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into())], String::new(), Instant::now());
        f.desk.close(&f.profile.progress().run.clone().unwrap());
        f.poll();
        f.desk.set_testing(true);
        assert_eq!(f.profile.send_answers(&mut f.desk, &[(1, "Design Lead")]), Err(SendRefusal::Testing));
        assert!(f.record().draft.is_some());
    }

    /// While Jobdesk updates the folder, or cannot lock it, Read my documents copies nothing and starts nothing, and a
    /// copy that ends meanwhile starts no run. The same press goes once the wait has ended.
    #[test]
    fn read_my_documents_waits_while_the_folder_updates() {
        let mut f = fixture();
        f.add("cv.pdf");
        f.desk.set_folder_wait(Some(FolderWait::Updating));
        let refused = f.read(&RunDefaults::default(), &open());
        assert_eq!(refused, Err(StartRefusal::FolderWait(FolderWait::Updating)));
        assert_eq!(refused.unwrap_err().line(), "Jobdesk is updating your job search folder. Setup can start once it ends.");
        assert!(f.shelf.0.lock().unwrap().is_empty() && f.script.borrow().requests.is_empty(), "nothing is copied, nothing starts");
        f.desk.set_folder_wait(Some(FolderWait::Unlockable));
        assert_eq!(f.profile.start_refusal(&f.desk, &rules(&RunDefaults::default(), &open())).map(|refusal| refusal.line()), Some(UNLOCKABLE.to_owned()));

        f.desk.set_folder_wait(None);
        f.profile.read_documents(&mut f.desk, &rules(&RunDefaults::default(), &open()), now()).unwrap();
        // Another Jobdesk takes the folder while the copy works.
        f.desk.set_folder_wait(Some(FolderWait::Updating));
        f.settle();
        assert_eq!(f.profile.start_failure(), Some(&StartRefusal::FolderWait(FolderWait::Updating)));
        assert!(f.script.borrow().requests.is_empty(), "no run starts after the copy");

        f.desk.set_folder_wait(None);
        assert_eq!(f.read(&RunDefaults::default(), &open()), Ok(()));
        assert_eq!(f.script.borrow().requests.len(), 1);
        assert_eq!(*f.shelf.0.lock().unwrap(), ["cv.pdf"], "copied once");
    }

    /// While the folder updates, no reply reaches Claude: not Jobdesk's own Path A reply, not the user's, and not Pick
    /// up where it stopped, which leaves the run as it was. Each goes once the wait has ended.
    #[test]
    fn no_reply_and_no_pick_up_while_the_folder_updates() {
        let mut f = fixture();
        f.start();
        f.desk.set_folder_wait(Some(FolderWait::Updating));
        f.says(PATH_QUESTION);
        f.ends_turn();
        f.poll();
        assert!(f.sent().is_empty() && !f.record().path_a_sent, "the path waits");
        f.desk.set_folder_wait(None);
        f.poll();
        assert_eq!(f.sent(), [PATH_A_REPLY], "and goes once the update has ended");

        f.says(FIVE);
        f.ends_turn();
        f.poll();
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into())], String::new(), Instant::now());
        f.desk.set_folder_wait(Some(FolderWait::Updating));
        let refused = f.profile.send_answers(&mut f.desk, &[(1, "Design Lead")]);
        assert_eq!(refused, Err(SendRefusal::FolderWait(FolderWait::Updating)));
        assert_eq!(refused.unwrap_err().line(), "Jobdesk is updating your job search folder. Send once it ends.");
        assert_eq!(SendRefusal::FolderWait(FolderWait::Unlockable).line(), UNLOCKABLE);
        assert!(f.record().draft.is_some(), "the draft stays");
        assert_eq!(f.sent(), [PATH_A_REPLY]);

        f.desk.set_folder_wait(None);
        f.profile.send_answers(&mut f.desk, &[(1, "Design Lead")]).unwrap();
        f.emit(AgentEvent::TurnEnded { failed: true, message: Some("Budget cap reached.".into()), cost_usd: 25.0, duration_ms: 1 });
        f.poll();
        assert!(matches!(f.profile.phase(), ProfilePhase::Stopped { .. }));
        let id = f.profile.progress().run.clone().unwrap();
        f.desk.set_folder_wait(Some(FolderWait::Updating));
        assert_eq!(f.pick_up(), Err(StartRefusal::FolderWait(FolderWait::Updating)));
        assert!(f.desk.has_session(&id), "the run is left as it was");
        assert_eq!(f.script.borrow().requests.len(), 1);
        f.desk.set_folder_wait(None);
        f.pick_up().unwrap();
        assert_eq!(f.script.borrow().requests.len(), 2, "picked up once the update has ended");
    }
}
