//! The Profile chapter's `/setup` run: Read my documents copies the documents and starts it, Jobdesk answers its path
//! question once with the documents path, and every other reply goes only when the user sends it. The phase the screen
//! shows comes from the run and the parse of Claude's last message, never from the disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{NaiveDate, NaiveDateTime};

use crate::onboarding::application::{Batch, Documents, Message, read_message};
use crate::onboarding::domain::{
    CV_LOOKS_GOOD, CopyFailure, DocumentCopier, Draft, PATH_A_REPLY, PICK_UP_REPLY, Progress, ProgressStore, Saved, joined_reply, review_reply, tool_line,
};
use crate::profile::application::{CvShelf, PdfDocument};
use crate::runs::application::{
    ActivityKind, CapCheck, Decision, Run, RunDefaults, RunDesk, RunStatus, StartError, StartOptions, notice_text, spec_for_line, start_values,
};
use crate::shared_kernel::visible_text::visible_chars;

/// The command the chapter runs, unchanged and with no arguments.
pub const SETUP: &str = "/setup";

/// A draft is written to the record at most this often while the user types.
pub const DRAFT_SAVE_EVERY: Duration = Duration::from_secs(1);

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
    CapReached {
        reopens_on: NaiveDate,
    },
    /// A cap is in force and this month's spend is not read yet.
    CapReading,
    Copy(CopyFailure),
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
            // A no-break space holds the day to its month.
            Self::CapReached { reopens_on } => {
                format!("This month's tasks reached the monthly cap you set in Settings. Setup can start on {}.", reopens_on.format("%-d\u{a0}%B"))
            }
            Self::CapReading => "Setup waits while this month's tasks are counted.".to_owned(),
            Self::Copy(failure) => failure.line().to_owned(),
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
}

impl SendRefusal {
    pub fn line(self) -> &'static str {
        match self {
            Self::NotNow => "Claude is not waiting on this right now.",
            Self::Empty => "Write something first.",
            Self::NoReply => "Tick a change, or use Change something.",
            Self::Busy => "Another task is running. Send once it ends.",
            Self::Testing => "Settings is testing the connection. Send once the test ends.",
        }
    }
}

impl From<SendRefusal> for StartRefusal {
    fn from(refusal: SendRefusal) -> Self {
        match refusal {
            SendRefusal::Busy => Self::Busy,
            SendRefusal::Testing => Self::Testing,
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
        }
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
        let id = desk.start(SETUP, &options, wall).map_err(|error| match error {
            StartError::AnotherRunIsActive => StartRefusal::Busy,
            StartError::TestRunning => StartRefusal::Testing,
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
                Some(failure) => Err(StartRefusal::Copy(failure)),
                None => self.start_now(desk, &input.rules, input.wall),
            };
            self.start_failure = started.err();
        }
        self.send_path_a(desk);
        self.phase =
            if self.starting { ProfilePhase::Copying } else { phase_of(self.run(desk), self.progress.path_a_sent, &self.folder, input.cv, input.without_pdfs) };
        self.save_draft_if_due(input.now);
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

    /// Stop setup, while Claude works. The run keeps its session, and Pick up where it stopped resumes it.
    pub fn stop(&mut self, desk: &mut RunDesk) {
        if let Some(id) = self.run(desk).filter(|run| run.is_busy()).map(|run| run.id.clone()) {
            desk.stop(&id);
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
        desk.reply(&id, text).map_err(|error| match error {
            StartError::AnotherRunIsActive => SendRefusal::Busy,
            StartError::TestRunning => SendRefusal::Testing,
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
        self.save_draft_if_due(now);
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

    /// Writes what waits, unless a write was tried less than `DRAFT_SAVE_EVERY` ago.
    pub fn save_draft_if_due(&mut self, now: Instant) {
        if !self.unsaved || self.last_draft_try.is_some_and(|at| now.saturating_duration_since(at) < DRAFT_SAVE_EVERY) {
            return;
        }
        self.last_draft_try = Some(now);
        if self.save().is_ok() {
            self.last_draft_save = Some(now);
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

/// What holds back a run that starts: another run, a connection test or the monthly cap.
fn gate(desk: &RunDesk, rules: &StartRules) -> Option<StartRefusal> {
    if desk.is_busy() {
        return Some(StartRefusal::Busy);
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

/// The phase `run` is in. `path_a_sent` says whether the documents path went, `root` is the job search folder, `cv`
/// the snapshot's CV documents, and `without_pdfs` holds when no CV can be built.
pub fn phase_of(run: Option<&Run>, path_a_sent: bool, root: &Path, cv: &CvShelf, without_pdfs: bool) -> ProfilePhase {
    let Some(run) = run else { return ProfilePhase::Documents };
    if run.needs_user() {
        return ProfilePhase::Choice;
    }
    if run.is_busy() {
        return if jobdesk_started_the_turn(run, path_a_sent) { ProfilePhase::Reading } else { ProfilePhase::Working };
    }
    match &run.status {
        RunStatus::Failed(reason) => return ProfilePhase::Stopped { reason: notice_text(reason).full },
        RunStatus::Stopped => return ProfilePhase::Stopped { reason: YOU_STOPPED.to_owned() },
        _ => {}
    }
    // A turn with no result in the run was cut off, by a quit or a crash. Whatever it said last, it never ended.
    if !run.has_turn_result() {
        return ProfilePhase::Stopped { reason: STOPPED_PARTWAY.to_owned() };
    }
    let closed = run.status == RunStatus::Closed;
    if waits_for_path(run, path_a_sent) {
        // A live run is answered on this poll. One whose process ended waits for the user to pick it up.
        return if closed { ProfilePhase::Stopped { reason: STOPPED_BEFORE_READING.to_owned() } } else { ProfilePhase::Reading };
    }
    at_turn_end(run, root, cv, without_pdfs)
}

/// The phase of a turn that ended cleanly with nothing open.
fn at_turn_end(run: &Run, root: &Path, cv: &CvShelf, without_pdfs: bool) -> ProfilePhase {
    let message = last_message(run).unwrap_or_default();
    let read = read_message(&message);
    if let Message::Proposal(batch) = read {
        return ProfilePhase::Review { message, batch };
    }
    if !without_pdfs
        && wrote_cv(run, root)
        && let Some(pdf) = fresh_pdf(run, cv)
    {
        return ProfilePhase::CvCheck { message, pdf };
    }
    if setup_complete(run, &message) {
        return ProfilePhase::Done;
    }
    match read {
        Message::Questions(batch) => ProfilePhase::Sheet { message, batch },
        _ => ProfilePhase::FallbackSheet { message },
    }
}

/// `text` as plain lower-case text, without hidden characters.
fn plain(text: &str) -> String {
    visible_chars(text).collect::<String>().to_lowercase()
}

/// Whether `text` says setup is complete.
fn says_complete(text: &str) -> bool {
    plain(text).contains(SETUP_COMPLETE)
}

/// Whether `text` names Path A, as a word: "Path A:" counts, and "path about" does not.
fn names_path_a(text: &str) -> bool {
    let text = plain(text);
    text.match_indices(PATH_A).any(|(at, _)| !text[at + PATH_A.len()..].starts_with(char::is_alphanumeric))
}

/// Whether the turn says setup is complete, or answers the CV check of a turn that said so: the CV check shows before
/// the end once, and the end follows the user's Looks good.
fn setup_complete(run: &Run, message: &str) -> bool {
    if says_complete(message) {
        return true;
    }
    let start = run.current_turn_start();
    let answered_check =
        start.checked_sub(1).and_then(|at| run.activity.get(at)).is_some_and(|row| matches!(&row.kind, ActivityKind::You(text) if text == CV_LOOKS_GOOD));
    answered_check && previous_turn_message(run, start - 1).is_some_and(|text| says_complete(&text))
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

/// The last top-level message of the turn before the user's message at `you`.
fn previous_turn_message(run: &Run, you: usize) -> Option<String> {
    run.activity[..you].iter().rev().take_while(|activity| !matches!(activity.kind, ActivityKind::You(_))).find_map(|activity| match &activity.kind {
        ActivityKind::Claude(text) if !activity.nested => Some(text.clone()),
        _ => None,
    })
}

/// Whether the current turn wrote under `cv/` or built a PDF there.
fn wrote_cv(run: &Run, root: &Path) -> bool {
    let root = root.to_string_lossy();
    run.activity.iter().skip(run.current_turn_start()).any(|activity| match &activity.kind {
        ActivityKind::Tool { name, summary, .. } => tool_line(&root, name, summary) == Some(MAKING_CV),
        _ => false,
    })
}

/// The newest CV PDF of the snapshot written since the run started. The run keeps its start to the minute, so a PDF
/// from that minute counts.
fn fresh_pdf(run: &Run, cv: &CvShelf) -> Option<PdfDocument> {
    let started = NaiveDateTime::parse_from_str(&run.started_at, STARTED_AT).ok()?;
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
    use crate::onboarding::domain::{PROGRESS_LINES, Refusal, by_name, shown_name};
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

    /// Takes a PDF by its name and copies under that name.
    #[derive(Default)]
    struct Shelf(Mutex<Vec<String>>);

    impl DocumentCopier for Shelf {
        fn look(&self, source: &Path) -> Result<Option<usize>, Refusal> {
            by_name(&shown_name(source)).map(|()| Some(1))
        }
        fn copy(&self, source: &Path, _: &Path) -> Result<String, CopyFailure> {
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
        Fixture { desk, script, store, shelf, profile, cv: CvShelf::default(), without_pdfs: false, rules_testing: false }
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
            let input = PollInput { cv: &self.cv, without_pdfs: self.without_pdfs, rules, now: at, wall: now() };
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
                let input = PollInput { cv: &cv, without_pdfs: false, rules: rules(&defaults, &cap), now: Instant::now(), wall: now() };
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
            f.poll_at(base + Duration::from_millis(300));
            assert_eq!(*f.store.saves.lock().unwrap(), tries, "a failed write is retried at the same pace");
            f.profile.flush();
            assert_eq!(*f.store.saves.lock().unwrap(), tries + 1, "a quit tries again");
            assert!(f.profile.is_unsaved());
            *f.store.disk.lock().unwrap() = Disk::Fine;
            f.poll_at(base + Duration::from_millis(1_100));
            assert!(!f.profile.is_unsaved());
            assert_eq!(f.profile.save_trouble(), None);
            assert_eq!(f.profile.last_draft_save(), Some(base + Duration::from_millis(1_100)));
            assert!(f.record().draft.is_some());
        }
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
}
