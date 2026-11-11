//! The Profile chapter's `/setup` run: Read my documents copies the documents and starts it, Jobdesk answers its opening
//! question once with the documents path, and every other reply goes only when the user sends it. The phase the screen
//! shows comes from the run and the parse of Claude's last message, never from the disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{NaiveDate, NaiveDateTime};

use crate::onboarding::application::{Batch, Documents, Message, read_message};
use crate::onboarding::domain::{
    CV_LOOKS_GOOD, CopyFailure, DocumentCopier, Draft, PATH_A_REPLY, PICK_UP_REPLY, Progress, ProgressStore, joined_reply, review_reply, tool_line,
};
use crate::profile::application::{CvShelf, PdfDocument};
use crate::runs::application::{
    ActivityKind, CapCheck, Decision, Run, RunDefaults, RunDesk, RunStatus, StartError, StartOptions, closing_question, notice_text, spec_for_line,
    start_values, stop_point,
};

/// The command the chapter runs, unchanged and with no arguments.
pub const SETUP: &str = "/setup";

/// A draft is written to the record at most this often while the user types.
pub const DRAFT_SAVE_EVERY: Duration = Duration::from_secs(1);

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
    /// Claude works on the turns Jobdesk started: the command line and the documents path.
    Reading,
    /// Claude works on a turn the user started.
    Working,
    /// Claude's message asks numbered questions: a field under each.
    Sheet { message: String, batch: Batch },
    /// Claude's message has no numbered questions Jobdesk can count: the message beside one field.
    FallbackSheet { message: String },
    /// Claude proposes numbered changes and asks to confirm: a checkbox each.
    Review { message: String, batch: Batch },
    /// Claude built the CV and waits: its pages, Looks good and Ask for a change.
    CvCheck { message: String, pdf: PdfDocument },
    /// A question from `AskUserQuestion`, or a permission prompt, waits on the user. Jobdesk never answers it.
    Choice,
    /// The run failed, ran out of budget or was stopped. The reason is plain text, without hidden characters.
    Stopped { reason: String },
    /// Claude ended a turn with nothing to answer, after the CV check or without PDFs: the profile is built.
    Done,
}

/// What Read my documents is checked against, as the launcher checks Run.
pub struct StartRules<'a> {
    pub defaults: &'a RunDefaults,
    pub cap: &'a CapCheck,
    /// Settings' Test connection is running.
    pub testing: bool,
}

/// Why Read my documents does not start the run. No line names a sum of money.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartRefusal {
    NoDocuments,
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
}

impl StartRefusal {
    pub fn line(&self) -> String {
        match self {
            Self::NoDocuments => "Add a document first.".to_owned(),
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

/// The Profile chapter over the job search folder: its documents, its `/setup` run on the shell's `RunDesk`, and the
/// progress record, which keeps the run's id, whether the documents path was sent, and the draft.
pub struct ProfileRun {
    store: Arc<dyn ProgressStore>,
    progress: Progress,
    folder: PathBuf,
    documents: Documents,
    today: NaiveDate,
    phase: ProfilePhase,
    /// The draft changed since the record was last written.
    unsaved: bool,
    last_draft_save: Option<Instant>,
}

impl ProfileRun {
    /// `progress` is the record as first-run setup holds it, and `folder` the job search folder it made.
    pub fn new(store: Arc<dyn ProgressStore>, progress: Progress, folder: PathBuf, copier: Arc<dyn DocumentCopier>, today: NaiveDate) -> Self {
        Self { store, progress, folder, documents: Documents::new(copier), today, phase: ProfilePhase::Documents, unsaved: false, last_draft_save: None }
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

    /// The files the user dropped or chose. Nothing is copied until Read my documents.
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

    /// This setup's run, found by the id the record keeps, so a desk restored from the run logs finds it again.
    pub fn run<'a>(&self, desk: &'a RunDesk) -> Option<&'a Run> {
        desk.run(self.progress.run.as_deref()?).filter(|run| run.line == SETUP)
    }

    /// Why Read my documents would not start the run now, if anything holds it.
    pub fn start_refusal(&self, desk: &RunDesk, rules: &StartRules) -> Option<StartRefusal> {
        if self.run(desk).is_some() {
            return Some(StartRefusal::AlreadyStarted);
        }
        if self.documents.is_empty() {
            return Some(StartRefusal::NoDocuments);
        }
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

    /// Read my documents: the user starting the run, as Run does in the launcher. It copies the documents into
    /// `documents/`, then starts `/setup` with the model and budget the launcher would start it with, and keeps the run's
    /// id in the record. Nothing is copied when the run could not start.
    pub fn read_documents(&mut self, desk: &mut RunDesk, rules: &StartRules, now: NaiveDateTime) -> Result<String, StartRefusal> {
        if let Some(refusal) = self.start_refusal(desk, rules) {
            return Err(refusal);
        }
        let spec = spec_for_line(SETUP).ok_or(StartRefusal::NoCommand)?;
        self.documents.copy_into(&self.folder).map_err(StartRefusal::Copy)?;
        let values = start_values(rules.defaults, spec);
        let options = StartOptions { model: values.model.alias().map(str::to_owned), budget_usd: values.budget.usd() };
        let id = desk.start(SETUP, &options, now).map_err(|error| match error {
            StartError::AnotherRunIsActive => StartRefusal::Busy,
            StartError::TestRunning => StartRefusal::Testing,
        })?;
        self.progress.run = Some(id.clone());
        self.progress.path_a_sent = false;
        self.progress.draft = None;
        self.save();
        self.phase = ProfilePhase::Reading;
        Ok(id)
    }

    /// Call once per frame, after `RunDesk::poll`. It sends the documents path once, when the first turn has ended, and
    /// finds the phase. `cv` is the snapshot's CV documents, and `without_pdfs` holds when the user went on without
    /// LaTeX, so no CV check can come. A draft waiting to be written is written once `DRAFT_SAVE_EVERY` has passed.
    pub fn poll(&mut self, desk: &mut RunDesk, cv: &CvShelf, without_pdfs: bool, now: Instant, today: NaiveDate) {
        self.today = today;
        self.send_path_a(desk);
        self.phase = phase_of(self.run(desk), self.progress.path_a_sent, &self.folder, cv, without_pdfs);
        self.save_draft_if_due(now);
    }

    /// Jobdesk's one reply of its own. It goes when the first turn ended on its live session, with nothing else open and
    /// no reply yet, and never again once the record says it went, so a restart cannot send it twice.
    fn send_path_a(&mut self, desk: &mut RunDesk) {
        if self.progress.path_a_sent {
            return;
        }
        let Some(id) = self.run(desk).filter(|run| run.turn_number() == 1 && run.awaits_reply() && desk.has_session(&run.id)).map(|run| run.id.clone()) else {
            return;
        };
        if desk.reply(&id, PATH_A_REPLY).is_ok() {
            self.progress.path_a_sent = true;
            self.save();
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

    /// Pick up where it stopped: a reply on the run's own session. A run that stopped before the documents path went
    /// gets that path, as the user starting it again. A failed run's process is ended first, so the reply resumes the
    /// session in a new process with a budget of its own: the old one may have spent all of its budget.
    pub fn pick_up(&mut self, desk: &mut RunDesk) -> Result<(), SendRefusal> {
        if !matches!(self.phase, ProfilePhase::Stopped { .. }) {
            return Err(SendRefusal::NotNow);
        }
        let Some((id, turn, failed)) = self.run(desk).map(|run| (run.id.clone(), run.turn_number(), matches!(run.status, RunStatus::Failed(_)))) else {
            return Err(SendRefusal::NotNow);
        };
        let path_a = turn == 1 && !self.progress.path_a_sent;
        if failed && desk.has_session(&id) {
            desk.close(&id);
        }
        self.send(desk, if path_a { PATH_A_REPLY } else { PICK_UP_REPLY })?;
        if path_a {
            self.progress.path_a_sent = true;
            self.save();
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
        self.save();
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

    /// Whether a draft waits to be written to the record.
    pub fn has_unsaved_draft(&self) -> bool {
        self.unsaved
    }

    /// When the draft was last written to the record, for the line that says so.
    pub fn last_draft_save(&self) -> Option<Instant> {
        self.last_draft_save
    }

    /// Writes a waiting draft, unless one was written less than `DRAFT_SAVE_EVERY` ago.
    pub fn save_draft_if_due(&mut self, now: Instant) {
        if self.unsaved && self.last_draft_save.is_none_or(|at| now.saturating_duration_since(at) >= DRAFT_SAVE_EVERY) {
            self.save();
            self.last_draft_save = Some(now);
        }
    }

    /// Writes a waiting draft now, as the window closes.
    pub fn flush(&mut self) {
        if self.unsaved {
            self.save();
        }
    }

    /// Keeps the record. One that cannot be written costs only the resume, so setup goes on with what it holds.
    fn save(&mut self) {
        self.progress.stopped = Some(self.today);
        let _ = self.store.save(&self.progress);
        self.unsaved = false;
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
        return if run.turn_number() <= 2 { ProfilePhase::Reading } else { ProfilePhase::Working };
    }
    match &run.status {
        RunStatus::Failed(reason) => return ProfilePhase::Stopped { reason: notice_text(reason).full },
        RunStatus::Stopped => return ProfilePhase::Stopped { reason: YOU_STOPPED.to_owned() },
        _ => {}
    }
    let closed = run.status == RunStatus::Closed;
    if run.turn_number() == 1 && !path_a_sent {
        // A live run is answered on this poll. One whose process ended waits for the user to pick it up.
        return if closed { ProfilePhase::Stopped { reason: STOPPED_BEFORE_READING.to_owned() } } else { ProfilePhase::Reading };
    }
    // A run read back from its log, or whose process ended, reads as its last turn ended: a sheet open at quit opens
    // again, and a reply resumes the session.
    let ended;
    let run = if closed {
        let mut awaiting = run.clone();
        awaiting.status = RunStatus::AwaitingReply;
        ended = awaiting;
        &ended
    } else {
        run
    };
    at_turn_end(run, closed, root, cv, without_pdfs)
}

/// The phase of a turn that ended with nothing open.
fn at_turn_end(run: &Run, closed: bool, root: &Path, cv: &CvShelf, without_pdfs: bool) -> ProfilePhase {
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
    match read {
        Message::Questions(batch) => ProfilePhase::Sheet { message, batch },
        _ if closing_question(run).is_some() || stop_point(run).is_some() || !(without_pdfs || cv_checked(run)) => ProfilePhase::FallbackSheet { message },
        // The turn may have been cut short, so it is never taken for the end.
        _ if closed => ProfilePhase::Stopped { reason: STOPPED_PARTWAY.to_owned() },
        _ => ProfilePhase::Done,
    }
}

/// The last top-level message of the current turn.
fn last_message(run: &Run) -> Option<String> {
    run.activity.iter().skip(run.current_turn_start()).rev().find_map(|activity| match &activity.kind {
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

/// Whether the user said the CV looks good, earlier in the run.
fn cv_checked(run: &Run) -> bool {
    run.activity.iter().any(|activity| matches!(&activity.kind, ActivityKind::You(text) if text == CV_LOOKS_GOOD))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Mutex;

    use super::*;
    use crate::onboarding::domain::{PROGRESS_LINES, Refusal, by_name, shown_name};
    use crate::profile::application::{CvFiles, FileStamp, cv_shelf};
    use crate::runs::application::{
        AgentEvent, AgentGateway, AgentSession, CapReached, EventSink, Prompt, PromptKind, RunHistory, RunRequest, WORDS_BUDGET_USD,
    };
    use crate::settings::application::{CommandDefault, Dollars};
    use crate::shared_kernel::ModelChoice;

    #[derive(Default)]
    struct Script {
        requests: Vec<RunRequest>,
        sent: Vec<String>,
        interrupted: bool,
        sink: Option<EventSink>,
    }

    /// A gateway that keeps the sink of the latest process, so a test can play Claude's side.
    struct ScriptedGateway(Rc<RefCell<Script>>);
    struct ScriptedSession(Rc<RefCell<Script>>);

    impl AgentGateway for ScriptedGateway {
        fn start(&self, request: &RunRequest, sink: EventSink) -> Box<dyn AgentSession> {
            sink(AgentEvent::SessionStarted { session_id: "s1".into(), model: "m".into(), api_key_source: "none".into() });
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

    #[derive(Default)]
    struct Memory {
        record: Mutex<Option<Progress>>,
        saves: Mutex<usize>,
    }

    impl ProgressStore for Memory {
        fn load(&self) -> Result<Option<Progress>, crate::shared_kernel::LoadError> {
            Ok(self.record.lock().unwrap().clone())
        }
        fn save(&self, progress: &Progress) -> Result<(), crate::shared_kernel::LoadError> {
            *self.record.lock().unwrap() = Some(progress.clone());
            *self.saves.lock().unwrap() += 1;
            Ok(())
        }
        fn clear(&self) -> Result<(), crate::shared_kernel::LoadError> {
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

    struct Fixture {
        desk: RunDesk,
        script: Rc<RefCell<Script>>,
        store: Arc<Memory>,
        shelf: Arc<Shelf>,
        profile: ProfileRun,
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
        let profile = ProfileRun::new(Arc::clone(&store) as Arc<dyn ProgressStore>, progress, PathBuf::from(ROOT), copier, today());
        Fixture { desk, script, store, shelf, profile }
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

        fn poll_with(&mut self, cv: &CvShelf, without_pdfs: bool) {
            self.desk.poll();
            self.profile.poll(&mut self.desk, cv, without_pdfs, Instant::now(), today());
        }

        fn poll(&mut self) {
            self.poll_with(&CvShelf::default(), false);
        }

        fn start(&mut self) -> String {
            self.profile.add_documents([PathBuf::from("/Users/sam/Desktop/Sam Rivera CV.pdf")]);
            let id = self.profile.read_documents(&mut self.desk, &rules(&RunDefaults::default(), &open()), now()).unwrap();
            self.poll();
            id
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
    }

    #[test]
    fn read_my_documents_copies_then_starts_setup_with_the_launchers_values() {
        let mut f = fixture();
        assert_eq!(f.profile.phase(), &ProfilePhase::Documents);
        let id = f.start();
        assert_eq!(*f.shelf.0.lock().unwrap(), ["Sam Rivera CV.pdf"]);
        let request = &f.script.borrow().requests[0];
        assert_eq!((request.line.as_str(), request.model.as_deref(), request.budget_usd), (SETUP, None, 25.0), "the catalog's suggestion");
        assert_eq!(f.record().run.as_deref(), Some(id.as_str()));
        assert_eq!(f.profile.phase(), &ProfilePhase::Reading);
    }

    #[test]
    fn a_command_row_or_a_budget_for_every_task_beats_the_suggestion() {
        let row = CommandDefault { command: SETUP.into(), model: ModelChoice::Sonnet, budget: Dollars::whole(12) };
        let every = RunDefaults { model: ModelChoice::Opus, budget: Some(Dollars::whole(40)), commands: vec![] };
        let both = RunDefaults { commands: vec![row], ..every.clone() };
        for (defaults, model, budget) in [(every, "opus", 40.0), (both, "sonnet", 12.0)] {
            let mut f = fixture();
            f.profile.add_documents([PathBuf::from("/Users/sam/Desktop/cv.pdf")]);
            f.profile.read_documents(&mut f.desk, &rules(&defaults, &open()), now()).unwrap();
            let request = &f.script.borrow().requests[0];
            assert_eq!((request.model.as_deref(), request.budget_usd), (Some(model), budget));
        }
        assert!(f64::from(WORDS_BUDGET_USD) < 25.0);
    }

    #[test]
    fn the_path_a_reply_goes_once_when_the_first_turn_ends() {
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
        let restored = Restored(read_back(f.desk.run(&id).unwrap()));
        let mut again = fixture_with(f.record(), Some(&restored));
        again.poll();
        assert!(again.script.borrow().requests.is_empty() && again.sent().is_empty(), "no process starts and nothing is sent");
        assert_eq!(again.profile.run(&again.desk).map(|run| run.id.as_str()), Some(id.as_str()), "the run is found by its id");
        // The record lost the flag, and the log still has the reply: the turn count holds it back.
        let mut lost = fixture_with(Progress { path_a_sent: false, ..f.record() }, Some(&restored));
        lost.poll();
        assert!(lost.script.borrow().requests.is_empty() && lost.sent().is_empty());
    }

    #[test]
    fn a_run_restored_before_its_path_went_waits_for_the_user_and_pick_up_sends_it() {
        let mut f = fixture();
        let id = f.start();
        f.says(PATH_QUESTION);
        f.emit(AgentEvent::ProcessExited { failure: None });
        f.poll();
        let restored = Restored(read_back(f.desk.run(&id).unwrap()));
        let mut again = fixture_with(Progress { path_a_sent: false, ..f.record() }, Some(&restored));
        again.poll();
        assert!(again.script.borrow().requests.is_empty(), "nothing starts on its own");
        assert_eq!(again.profile.phase(), &ProfilePhase::Stopped { reason: STOPPED_BEFORE_READING.into() });
        again.profile.pick_up(&mut again.desk).unwrap();
        let requests = &again.script.borrow().requests;
        assert_eq!((requests.len(), requests[0].line.as_str(), requests[0].resume_session.as_deref()), (1, PATH_A_REPLY, Some("s1")));
        assert!(again.record().path_a_sent);
    }

    #[test]
    fn a_second_start_while_a_run_works_is_refused() {
        let mut f = fixture();
        f.start();
        assert_eq!(f.profile.read_documents(&mut f.desk, &rules(&RunDefaults::default(), &open()), now()), Err(StartRefusal::AlreadyStarted));

        let mut other = fixture();
        other.desk.start("/rank", &StartOptions { model: None, budget_usd: 3.0 }, now()).unwrap();
        other.profile.add_documents([PathBuf::from("/Users/sam/Desktop/cv.pdf")]);
        let refused = other.profile.read_documents(&mut other.desk, &rules(&RunDefaults::default(), &open()), now());
        assert_eq!(refused, Err(StartRefusal::Busy));
        assert!(other.shelf.0.lock().unwrap().is_empty(), "nothing is copied for a run that cannot start");
        assert_eq!(other.script.borrow().requests.len(), 1);

        let mut testing = fixture();
        testing.profile.add_documents([PathBuf::from("/Users/sam/Desktop/cv.pdf")]);
        let rules = StartRules { defaults: &RunDefaults::default(), cap: &CapCheck::Open, testing: true };
        assert_eq!(testing.profile.read_documents(&mut testing.desk, &rules, now()), Err(StartRefusal::Testing));
    }

    #[test]
    fn no_documents_no_start() {
        let mut f = fixture();
        assert_eq!(f.profile.read_documents(&mut f.desk, &rules(&RunDefaults::default(), &open()), now()), Err(StartRefusal::NoDocuments));
        f.profile.add_documents([PathBuf::from("/Users/sam/Desktop/Reference.docx")]);
        assert_eq!(f.profile.start_refusal(&f.desk, &rules(&RunDefaults::default(), &open())), Some(StartRefusal::NoDocuments));
    }

    #[test]
    fn the_cap_is_refused_without_a_figure() {
        let reached = CapCheck::Reached(CapReached { spent_usd: 61.25, cap: Dollars::whole(60), reopens_on: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap() });
        for cap in [reached, CapCheck::Reading] {
            let mut f = fixture();
            f.profile.add_documents([PathBuf::from("/Users/sam/Desktop/cv.pdf")]);
            let refused = f.profile.read_documents(&mut f.desk, &rules(&RunDefaults::default(), &cap), now()).unwrap_err();
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
    fn a_message_without_numbers_opens_the_fallback_sheet_until_the_cv_check() {
        let mut f = fixture();
        f.at(UNNUMBERED);
        assert_eq!(f.profile.phase(), &ProfilePhase::FallbackSheet { message: UNNUMBERED.into() });
        assert_eq!(f.profile.send_text(&mut f.desk, "  \n"), Err(SendRefusal::Empty));
        f.profile.send_text(&mut f.desk, "  Product design, in Lowmere.  ").unwrap();
        assert_eq!(f.sent().last().unwrap(), "Product design, in Lowmere.");
    }

    fn cv_with_pdf(at: NaiveDateTime) -> CvShelf {
        let pdf = FileStamp { name: "main_example.pdf".into(), path: PathBuf::from("cv/main_example.pdf"), modified: 1_000, at, bytes: 80_000 };
        cv_shelf(vec![CvFiles { stem: "main_example".into(), tex: None, pdf: Some(pdf) }], false)
    }

    #[test]
    fn a_cv_built_this_run_opens_the_cv_check_and_then_setup_is_done() {
        let mut f = fixture();
        f.at(FIVE);
        f.profile.send_answers(&mut f.desk, &[]).unwrap();
        f.tool("w", "Write", &format!("{ROOT}/cv/main_example.tex"));
        f.tool("b", "Bash", "cd cv && lualatex main_example.tex");
        f.says("Your CV is built. Have a look at cv/main_example.pdf.");
        f.ends_turn();
        let built = cv_with_pdf(now() + chrono::TimeDelta::minutes(30));
        f.poll_with(&cv_with_pdf(now() - chrono::TimeDelta::days(1)), false);
        assert!(matches!(f.profile.phase(), ProfilePhase::FallbackSheet { .. }), "a PDF from before the run is no CV check");
        f.poll_with(&built, true);
        assert!(matches!(f.profile.phase(), ProfilePhase::Done), "without PDFs there is no CV check");
        f.poll_with(&built, false);
        let ProfilePhase::CvCheck { pdf, .. } = f.profile.phase().clone() else { panic!("{:?}", f.profile.phase()) };
        assert_eq!(pdf.path, PathBuf::from("cv/main_example.pdf"));

        f.profile.cv_looks_good(&mut f.desk).unwrap();
        assert_eq!(f.sent().last().unwrap(), CV_LOOKS_GOOD);
        f.says("Setup complete! Run /scrape to search for matching jobs.");
        f.ends_turn();
        f.poll_with(&built, false);
        assert_eq!(f.profile.phase(), &ProfilePhase::Done, "the turn wrote nothing under cv/");
    }

    #[test]
    fn a_turn_that_wrote_no_cv_is_no_cv_check_and_the_making_line_is_the_handoffs() {
        assert!(PROGRESS_LINES.iter().any(|line| line.text == MAKING_CV));
        let mut f = fixture();
        f.at("Your CV is built.");
        f.poll_with(&cv_with_pdf(now() + chrono::TimeDelta::minutes(30)), false);
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
        f.profile.pick_up(&mut f.desk).unwrap();
        let requests = &f.script.borrow().requests;
        assert_eq!((requests.len(), requests[1].line.as_str()), (2, PICK_UP_REPLY), "a new process, with a budget of its own");
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
        stopped.profile.pick_up(&mut stopped.desk).unwrap();
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
    fn a_restored_run_with_nothing_to_answer_is_never_taken_for_done() {
        let mut f = fixture();
        let id = f.at(FIVE);
        f.profile.send_answers(&mut f.desk, &[]).unwrap();
        f.says("Writing your profile now.");
        f.emit(AgentEvent::ProcessExited { failure: None });
        f.poll_with(&CvShelf::default(), true);
        assert_eq!(f.profile.phase(), &ProfilePhase::Stopped { reason: STOPPED_PARTWAY.into() });
        let restored = Restored(read_back(f.desk.run(&id).unwrap()));
        let mut again = fixture_with(f.record(), Some(&restored));
        again.poll_with(&CvShelf::default(), true);
        assert_eq!(again.profile.phase(), &ProfilePhase::Stopped { reason: STOPPED_PARTWAY.into() });
    }

    #[test]
    fn a_draft_is_keyed_by_run_and_turn_saved_at_most_once_a_second_and_cleared_by_send() {
        let mut f = fixture();
        let id = f.at(FIVE);
        let saves = || *f.store.saves.lock().unwrap();
        let base = Instant::now();
        let before = saves();
        f.profile.record_draft(&f.desk, vec![(1, "Design".into())], String::new(), base);
        assert_eq!(saves(), before + 1, "the first change is written at once");
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into())], String::new(), base + Duration::from_millis(400));
        assert_eq!(saves(), before + 1, "not again within the second");
        assert!(f.profile.has_unsaved_draft());
        f.profile.save_draft_if_due(base + Duration::from_millis(900));
        assert_eq!(saves(), before + 1);
        f.profile.save_draft_if_due(base + Duration::from_millis(1_400));
        assert_eq!(saves(), before + 2);
        f.profile.record_draft(&f.desk, vec![(1, "Design Lead".into()), (2, "Mine".into())], String::new(), base + Duration::from_millis(1_500));
        f.profile.flush();
        assert_eq!(saves(), before + 3, "a quit writes what waits");
        let kept = f.record().draft.unwrap();
        assert_eq!((kept.run.as_str(), kept.turn, kept.answers.len()), (id.as_str(), 2, 2));
        assert_eq!(f.profile.draft(&f.desk), Some(&kept));

        f.profile.send_answers(&mut f.desk, &[(1, "Design Lead"), (2, "Mine")]).unwrap();
        assert_eq!(f.record().draft, None, "cleared once Send succeeded");
        assert_eq!(f.profile.draft(&f.desk), None);
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
