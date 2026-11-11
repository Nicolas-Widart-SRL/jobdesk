//! The Profile chapter's screens, in the window grown to the app's size: the documents, Claude working, its questions,
//! its proposed changes, the CV check, a prompt, and a stop. They read `ProfileRun` and the run on the desk, and each
//! reply goes only from a button the user pressed, or Cmd Enter. Claude's words and file names are untrusted: Claude's
//! Markdown renders by `claude_markdown`'s rules, and every other line of it as plain text without hidden characters.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{NaiveDateTime, TimeDelta};
use egui::{ColorImage, Key, Modifiers, OpenUrl, TextureFilter, TextureHandle, TextureOptions, Ui};
use egui_commonmark::CommonMarkCache;

use crate::onboarding::application::{
    Batch, CV_LOOKS_GOOD, DRAFT_SAVE_EVERY, Decision, DocumentKind, Draft, LineState, MessageBlock, PageState, PageStore, PdfDocument, ProfilePhase,
    ProfileRun, PromptKind, Question as AskedQuestion, Quiet, Run, RunDesk, RunStatus, StartRefusal, StartRules, WorkLine, elapsed_text, quiet, review_reply,
};
use crate::onboarding::ui::{STEPS, get_help};
use crate::shared_kernel::visible_text::visible_chars;
use crate::ui_kit::tokens::{DOC_MEASURE, ITEM_GAP};
use crate::ui_kit::{
    AddedFile, AnswersSlip, Change, DropForm, Ink, KnotState, LineEnd, NOT_SAVED, PageSet, ProposalPart, Question, SetupLine, SheetPart, SubItem, SweepForm,
    Tone, Type, WizardStep, alert_slip, answers_slip, button, choice_card, claude_markdown, claude_wrote_popover, command_chord, copyable, cv_check, cv_slip,
    danger_button, desk_label, drop_zone, enabled_when, file_list, link, paper, path_label, primary_button, primary_shortcut_button, prompt_box_tone,
    question_fallback, question_sheet, reply_slip, review_changes, review_slip, setting_check, setup_line, side_slip, slip_text, solid_tag, split_card,
    text_field, wide_card,
};

/// The user guide's part on the documents to add, with how to save a LinkedIn profile as a PDF.
const LINKEDIN_HELP: &str = "https://github.com/Nicolas-Widart-SRL/jobdesk/blob/main/docs/guide.md#your-documents";
/// The step the chapter stands at, counting from 0.
const AT: usize = 2;
const DOCS_TITLE: &str = "Tell Jobdesk about you";
const DOCS_LEAD: &str = "Add your CV and anything else that says what you have done. Claude reads it all, then asks about what is missing.";
const DROP_TITLE: &str = "Drop your documents here";
const DROP_SENTENCE: &str = "Your CV, and any of these: a LinkedIn profile saved as a PDF, diplomas, reference letters. PDF files only.";
const DROP_MORE: &str = "Drop more here: diplomas, reference letters.";
const ADD_CV_FIRST: &str = "Add at least your CV first.";
const STOP_SETUP: &str = "Stop setup";
const SHOW_ALL: &str = "Show all Claude wrote";
const ASK_FOR_CHANGE: &str = "Ask for a change";
const CHANGE_SOMETHING: &str = "Change something";
/// How long a change the user asks for may be, in characters.
const CHANGE_LIMIT: usize = 4000;
/// How many grey pages show while the CV renders.
const LOADING_PAGES: usize = 2;
/// How often the screen asks for a frame while the sweep moves, about thirty a second.
const SWEEP_FRAME: Duration = Duration::from_millis(33);

/// What the chapter asks of the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChapterRequest {
    /// Choose files…: the system's file picker, whose files are then added.
    ChooseFiles,
    /// Open PDF on the CV check, by the name `cv/` lists the file under.
    OpenPdf(String),
    /// Finish later, and Stop setup while Claude waits on the user: the app opens with what is done, and the next open
    /// offers to pick up.
    FinishLater,
}

/// What the chapter reads besides the run: the desk it runs on, the rules a start is checked against, the CV's page
/// renderer, the clocks, and the job search folder as the screens show it.
pub struct Chapter<'a> {
    pub desk: &'a mut RunDesk,
    pub rules: StartRules<'a>,
    pub pages: Option<&'a mut PageStore>,
    /// The local time, for a start and when the draft last saved.
    pub wall: NaiveDateTime,
    /// The window's clock, for the draft's pace.
    pub now: Instant,
    /// The job search folder as the screens show it, such as `Documents/Job search`.
    pub folder: &'a str,
}

/// What the chapter keeps between frames: the fields of Claude's current turn, the ticks of a review, the picks of a
/// prompt, and the CV's pages. A new turn starts them afresh, from the draft the record holds for it.
#[derive(Default)]
pub struct ProfileChapter {
    cache: CommonMarkCache,
    /// The run and turn the fields belong to.
    turn: Option<(String, usize)>,
    /// The draft of this turn, not yet in the fields.
    pending: Option<Draft>,
    answers: Vec<String>,
    text: String,
    ticks: Vec<bool>,
    /// Change something, or Ask for a change, opened its field.
    changing: bool,
    show_all: bool,
    picks: Vec<BTreeSet<usize>>,
    picks_for: Option<String>,
    /// Why the last press did not go, until the next turn.
    refusal: Option<String>,
    textures: Option<((PathBuf, u64), Vec<TextureHandle>)>,
}

/// Claude's message as the screens take it apart: each block in order, and its numbered items.
struct Laid {
    parts: Vec<LaidPart>,
    items: Vec<LaidItem>,
}

/// A block of Claude's message: its own Markdown, a bold line alone as plain text, or the item at an index of
/// `Laid::items`.
enum LaidPart {
    Prose(String),
    Group(String),
    Item(usize),
}

struct LaidItem {
    number: u32,
    text: String,
    subs: Vec<(char, String)>,
    optional: bool,
}

impl ProfileChapter {
    /// The screen for the chapter's phase. The recap, once the run is done, is the shell's.
    pub fn show(&mut self, ui: &mut Ui, run: &mut ProfileRun, cx: &mut Chapter) -> Option<ChapterRequest> {
        self.follow_turn(run, cx.desk);
        let phase = run.phase().clone();
        // Taken before any field, so Cmd Enter sends and no field types it.
        let send_keys = matches!(phase, ProfilePhase::Sheet { .. } | ProfilePhase::FallbackSheet { .. })
            && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter));
        let asked = match &phase {
            ProfilePhase::Documents => self.documents(ui, run, cx),
            ProfilePhase::Copying | ProfilePhase::Reading | ProfilePhase::Working => {
                working(ui, run, cx.desk, &phase);
                None
            }
            ProfilePhase::Sheet { batch, .. } => self.sheet(ui, run, cx, batch, send_keys),
            ProfilePhase::FallbackSheet { message } => self.fallback(ui, run, cx, message, send_keys),
            ProfilePhase::Review { batch, .. } => self.review(ui, run, cx, batch),
            ProfilePhase::CvCheck { message, pdf } => self.cv(ui, run, cx, message, pdf),
            ProfilePhase::Choice => {
                self.choice(ui, run, cx);
                None
            }
            ProfilePhase::Stopped { reason } => self.stopped(ui, run, cx, reason),
            ProfilePhase::Done => None,
        };
        let message = match &phase {
            ProfilePhase::Sheet { message, .. }
            | ProfilePhase::FallbackSheet { message }
            | ProfilePhase::Review { message, .. }
            | ProfilePhase::CvCheck { message, .. } => Some(message.as_str()),
            _ => None,
        };
        match message {
            Some(message) => claude_wrote_popover(ui.ctx(), &mut self.show_all, &mut self.cache, message),
            None => self.show_all = false,
        }
        // A draft that waits for its next write gets a frame of its own, so the last words save without more typing.
        if run.is_unsaved() {
            ui.ctx().request_repaint_after(DRAFT_SAVE_EVERY);
        }
        asked
    }

    /// Starts the fields afresh when Claude's turn changes, keeping the record's draft of the new turn for them.
    fn follow_turn(&mut self, run: &ProfileRun, desk: &RunDesk) {
        let turn = run.run(desk).map(|shown| (shown.id.clone(), shown.turn_number()));
        if turn == self.turn {
            return;
        }
        self.turn = turn;
        self.pending = run.draft(desk).cloned();
        (self.answers, self.text, self.ticks, self.changing, self.show_all, self.refusal) = (Vec::new(), String::new(), Vec::new(), false, false, None);
        if let Some(draft) = &self.pending {
            self.text = draft.text.clone();
            self.changing = !draft.text.trim().is_empty();
        }
    }

    /// The foot's muted line: why the last press did not go, else the run's Elapsed.
    fn why(&self, run: &ProfileRun, desk: &RunDesk) -> String {
        self.refusal.clone().unwrap_or_else(|| elapsed(run.run(desk), desk))
    }

    /// Keeps what the user typed in the record, at the run's pace.
    fn record(&self, run: &mut ProfileRun, cx: &Chapter, numbers: &[u32]) {
        let answers = numbers.iter().copied().zip(self.answers.iter().cloned()).collect();
        run.record_draft(cx.desk, answers, self.text.clone(), cx.now);
    }

    /// The documents to add, then Read my documents.
    fn documents(&mut self, ui: &mut Ui, run: &mut ProfileRun, cx: &mut Chapter) -> Option<ChapterRequest> {
        let added: Vec<AddedFile> = run
            .documents()
            .added()
            .iter()
            .map(|added| AddedFile {
                name: &added.name,
                pages: added.pages,
                kind: DocumentKind::ALL.iter().position(|kind| *kind == added.kind).unwrap_or(0),
                copied: added.copied_as.is_some(),
            })
            .collect();
        let refused: Vec<(String, &str)> = run.documents().refused().iter().map(|refused| (plain(&refused.name), refused.line())).collect();
        let failed = run.start_failure().map(|failure| plain(&failure.line()));
        let refusal = run.start_refusal(cx.desk, &cx.rules);
        let count = added.len();
        let why = match &refusal {
            None if count == 1 => "1 document added".to_owned(),
            None => format!("{count} documents added"),
            Some(StartRefusal::NoDocuments) => ADD_CV_FIRST.to_owned(),
            Some(other) => other.line(),
        };
        let why = self.refusal.clone().unwrap_or(why);
        let kinds: Vec<&str> = DocumentKind::ALL.iter().map(|kind| kind.label()).collect();
        let (dir, name) = split_path(cx.folder);
        let read = Cell::new(false);
        let mut zone = None;
        let mut listed = None;
        split_card(
            ui,
            &WizardStep { steps: &STEPS, at: AT, title: DOCS_TITLE, lead: DOCS_LEAD },
            |ui| {
                if let Some(line) = &failed {
                    alert_slip(ui, "Setup did not start.", line);
                }
                if added.is_empty() {
                    zone = Some(drop_zone(ui, DropForm::Full { title: DROP_TITLE, sentence: DROP_SENTENCE }));
                } else {
                    listed = Some(file_list(ui, &added, &kinds));
                    zone = Some(drop_zone(ui, DropForm::Slim { sentence: DROP_MORE }));
                }
                for (name, line) in &refused {
                    alert_slip(ui, &format!("{name} was not added."), line);
                }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    desk_label(ui, "Jobdesk copies them into the documents folder inside ", Type::NOTE, Ink::Muted);
                    path_label(ui, &dir, &name);
                    desk_label(ui, ". The originals stay where they are.", Type::NOTE, Ink::Muted);
                });
                if added.is_empty() && link(ui, "How to save your LinkedIn profile as a PDF").clicked() {
                    ui.ctx().open_url(OpenUrl::new_tab(LINKEDIN_HELP));
                }
            },
            |ui| {
                side_slip(ui, "What comes next", "", |ui| {
                    slip_text(
                        ui,
                        "",
                        "Claude reads your documents, then writes to you in a few rounds of questions. Answer them on your own time, in any order.",
                    );
                    slip_text(ui, "", "Setup took one tester about 40 minutes, most of it writing answers. Your answers are saved as you type.");
                });
            },
            |ui| {
                read.set(enabled_when(ui, refusal.is_none(), &why, |ui| primary_button(ui, "Read my documents")).clicked());
            },
            &why,
        );
        let mut asked = None;
        if let Some(zone) = zone {
            if zone.choose {
                asked = Some(ChapterRequest::ChooseFiles);
            }
            if !zone.dropped.is_empty() {
                run.add_documents(zone.dropped);
            }
        }
        if let Some(listed) = listed {
            if let Some((at, kind)) = listed.kind {
                run.set_document_kind(at, DocumentKind::ALL[kind]);
            }
            if let Some(at) = listed.removed {
                run.remove_document(at);
            }
        }
        if read.get() {
            self.refusal = run.read_documents(cx.desk, &cx.rules, cx.wall).err().map(|refusal| refusal.line());
        }
        // The documents are looked at on a worker, which wakes the window with its news.
        asked
    }

    /// Claude's numbered questions, a field under each, then Send answers.
    fn sheet(&mut self, ui: &mut Ui, run: &mut ProfileRun, cx: &mut Chapter, batch: &Batch, send_keys: bool) -> Option<ChapterRequest> {
        let numbers = batch.numbers();
        if let Some(draft) = self.pending.take() {
            self.answers =
                numbers.iter().map(|number| draft.answers.iter().find(|(at, _)| at == number).map(|(_, text)| text.clone()).unwrap_or_default()).collect();
        }
        let laid = lay_out(batch, false);
        let subs: Vec<Vec<SubItem>> =
            laid.items.iter().map(|item| item.subs.iter().map(|(letter, text)| SubItem { letter: *letter, text }).collect()).collect();
        let parts: Vec<SheetPart> = laid
            .parts
            .iter()
            .map(|part| match part {
                LaidPart::Prose(text) => SheetPart::Prose(text),
                LaidPart::Group(title) => SheetPart::Group(title),
                LaidPart::Item(at) => {
                    let item = &laid.items[*at];
                    SheetPart::Question(Question { number: item.number, text: &item.text, sub_items: &subs[*at], optional: item.optional })
                }
            })
            .collect();
        let count = numbers.len();
        let short = count <= 3;
        let rounds = run.so_far(cx.desk).rounds.len();
        let (title, lead) = if short {
            (format!("{} quick checks", capitalized(count)), "Claude wants to check a few things before it goes on.".to_owned())
        } else {
            (format!("Claude has {count} questions"), format!("Writing your profile. Your {} round of answers.", ordinal(rounds + 1)))
        };
        let saved_at = saved_at(run, cx);
        let unsaved = run.save_trouble().is_some();
        let why = self.why(run, cx.desk);
        let (send, finish_later, changed) = (Cell::new(send_keys), Cell::new(false), Cell::new(false));
        let mut show_all = false;
        let answers = RefCell::new(std::mem::take(&mut self.answers));
        let cache = &mut self.cache;
        split_card(
            ui,
            &WizardStep { steps: &STEPS, at: AT, title: &title, lead: &lead },
            |ui| changed.set(question_sheet(ui, cache, &parts, &mut answers.borrow_mut())),
            |ui| {
                let answered: Vec<bool> = answers.borrow().iter().map(|answer| !answer.trim().is_empty()).collect();
                show_all = answers_slip(ui, &AnswersSlip { answered: &answered, saved_at: saved_at.as_deref(), short, unsaved });
                if short {
                    show_all |= show_all_slip(ui);
                }
            },
            |ui| {
                send.set(send.get() | primary_shortcut_button(ui, "Send answers", &command_chord("Enter")).clicked());
                finish_later.set(button(ui, STOP_SETUP).clicked());
            },
            &why,
        );
        self.answers = answers.into_inner();
        self.show_all |= show_all;
        if changed.get() {
            self.record(run, cx, &numbers);
        }
        if send.get() {
            let replies: Vec<(u32, &str)> = numbers.iter().copied().zip(self.answers.iter().map(String::as_str)).collect();
            self.refusal = run.send_answers(cx.desk, &replies).err().map(|refusal| refusal.line().to_owned());
        }
        stop_setup(run, cx, finish_later.get())
    }

    /// Claude's message beside one large field, when it asks nothing Jobdesk can count. Once a turn said setup is
    /// complete, Finish setup ends it, as only the user can.
    fn fallback(&mut self, ui: &mut Ui, run: &mut ProfileRun, cx: &mut Chapter, message: &str, send_keys: bool) -> Option<ChapterRequest> {
        self.pending = None;
        let complete = run.complete_seen(cx.desk);
        let rounds = run.so_far(cx.desk).rounds.len();
        let lead = if complete {
            "Claude says your profile is ready. Answer it, or press Finish setup.".to_owned()
        } else {
            format!("Writing your profile. Your {} round of answers.", ordinal(rounds + 1))
        };
        let draft = if run.save_trouble().is_some() {
            NOT_SAVED.to_owned()
        } else {
            match saved_at(run, cx) {
                Some(at) => format!("Draft saved as you type, last at {at}. You can close Jobdesk and come back."),
                None => "Draft saved as you type. You can close Jobdesk and come back.".to_owned(),
            }
        };
        let help = format!("Claude asked in its own words this time, so your answer is one reply. Answer in any order; short answers are fine.\n{draft}");
        let why = self.why(run, cx.desk);
        let (send, finish_later, finish, changed) = (Cell::new(send_keys), Cell::new(false), Cell::new(false), Cell::new(false));
        let show_all = Cell::new(false);
        let text = &mut self.text;
        let cache = &mut self.cache;
        wide_card(
            ui,
            &WizardStep { steps: &STEPS, at: AT, title: "Claude has a few questions", lead: &lead },
            |ui| changed.set(question_fallback(ui, cache, message, text, &help)),
            |ui| {
                send.set(send.get() | primary_shortcut_button(ui, "Send answer", &command_chord("Enter")).clicked());
                finish_later.set(button(ui, STOP_SETUP).clicked());
                if complete {
                    finish.set(link(ui, "Finish setup").clicked());
                }
                show_all.set(link(ui, SHOW_ALL).clicked());
            },
            &why,
        );
        self.show_all |= show_all.get();
        if changed.get() {
            self.record(run, cx, &[]);
        }
        if finish.get() {
            run.finish(cx.desk);
        } else if send.get() {
            self.refusal = run.send_text(cx.desk, &self.text).err().map(|refusal| refusal.line().to_owned());
        }
        stop_setup(run, cx, finish_later.get())
    }

    /// Claude's proposed changes, a checkbox each, then the reply the ticks build.
    fn review(&mut self, ui: &mut Ui, run: &mut ProfileRun, cx: &mut Chapter, batch: &Batch) -> Option<ChapterRequest> {
        self.pending = None;
        let numbers = batch.numbers();
        let laid = lay_out(batch, true);
        let parts: Vec<ProposalPart> = laid
            .parts
            .iter()
            .map(|part| match part {
                LaidPart::Prose(text) => ProposalPart::Prose(text),
                LaidPart::Group(title) => ProposalPart::Group(title),
                LaidPart::Item(at) => ProposalPart::Change(Change { number: laid.items[*at].number, text: &laid.items[*at].text }),
            })
            .collect();
        if self.ticks.len() != numbers.len() {
            self.ticks = vec![true; numbers.len()];
        }
        let ticked: Vec<(u32, bool)> = numbers.iter().copied().zip(self.ticks.iter().copied()).collect();
        let reply = review_reply(&numbers, &ticked);
        let changing = self.changing;
        let why = match (&self.refusal, &reply) {
            (Some(refusal), _) => refusal.clone(),
            (None, None) if !changing => "Tick a change, or use Change something.".to_owned(),
            _ => elapsed(run.run(cx.desk), cx.desk),
        };
        let (apply, change, send, cancel, changed) = (Cell::new(false), Cell::new(false), Cell::new(false), Cell::new(false), Cell::new(false));
        let show_all = Cell::new(false);
        let ticks = &mut self.ticks;
        let text = &mut self.text;
        let cache = &mut self.cache;
        split_card(
            ui,
            &WizardStep {
                steps: &STEPS,
                at: AT,
                title: "Claude proposes changes to your profile",
                lead: "From your documents and your answers. Nothing changes until you say so.",
            },
            |ui| {
                review_changes(ui, cache, &parts, ticks);
                if changing {
                    ui.add_space(ITEM_GAP);
                    desk_label(ui, "What should Claude change? Your words are sent as they are, without the ticks.", Type::NOTE, Ink::Muted);
                    let width = ui.available_width().min(DOC_MEASURE);
                    changed.set(text_field(ui, text, width, "Say what to change", CHANGE_LIMIT).changed());
                }
            },
            |ui| {
                review_slip(ui, &ticked, reply.as_deref().unwrap_or("Nothing to apply."));
                side_slip(ui, "", "", |ui| slip_text(ui, "", "Something to fix? Change something opens a field. Say what to change in your own words."));
                show_all.set(show_all_slip(ui));
            },
            |ui| {
                if changing {
                    send.set(primary_button(ui, "Send").clicked());
                    cancel.set(button(ui, "Cancel").clicked());
                } else {
                    apply.set(enabled_when(ui, reply.is_some(), &why, |ui| primary_button(ui, "Apply these changes")).clicked());
                    change.set(button(ui, CHANGE_SOMETHING).clicked());
                }
            },
            &why,
        );
        self.show_all |= show_all.get();
        self.change_presses(run, cx, changed.get(), change.get(), cancel.get(), send.get());
        if apply.get() {
            let ticked: Vec<(u32, bool)> = numbers.iter().copied().zip(self.ticks.iter().copied()).collect();
            self.refusal = run.send_review(cx.desk, &ticked).err().map(|refusal| refusal.line().to_owned());
        }
        None
    }

    /// The presses of a field that asks for a change in the user's own words: open it, type, cancel or send.
    fn change_presses(&mut self, run: &mut ProfileRun, cx: &mut Chapter, changed: bool, open: bool, cancel: bool, send: bool) {
        if open {
            self.changing = true;
        }
        if changed {
            self.record(run, cx, &[]);
        }
        if cancel {
            self.changing = false;
            self.text.clear();
            self.record(run, cx, &[]);
        }
        if send {
            self.refusal = run.send_text(cx.desk, &self.text).err().map(|refusal| refusal.line().to_owned());
        }
    }

    /// The CV's pages, Looks good and Ask for a change.
    fn cv(&mut self, ui: &mut Ui, run: &mut ProfileRun, cx: &mut Chapter, message: &str, pdf: &PdfDocument) -> Option<ChapterRequest> {
        self.pending = None;
        let shown = self.pages(ui, pdf, cx.pages.as_deref_mut());
        let file = pdf.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let label = plain(&pdf.label);
        let sentence = match &shown {
            Shown::Ready(total) if *total == 1 => "1 page. Saved in the cv folder of your job search folder.".to_owned(),
            Shown::Ready(total) => format!("{total} pages. Saved in the cv folder of your job search folder."),
            _ => "Saved in the cv folder of your job search folder.".to_owned(),
        };
        let changing = self.changing;
        let why = self.why(run, cx.desk);
        let (open, good, change, send, cancel, changed) =
            (Cell::new(false), Cell::new(false), Cell::new(false), Cell::new(false), Cell::new(false), Cell::new(false));
        let textures = self.textures.as_ref().map_or(&[][..], |(_, textures)| textures.as_slice());
        let text = &mut self.text;
        let cache = &mut self.cache;
        let show_all = Cell::new(false);
        split_card(
            ui,
            &WizardStep { steps: &STEPS, at: AT, title: "Check your CV", lead: "Claude made it from your profile. Have a look before setup finishes." },
            |ui| {
                // What Claude said with the CV, as it renders everywhere in the chapter.
                ui.scope(|ui| {
                    ui.set_max_width(DOC_MEASURE);
                    claude_markdown(ui, cache, message);
                });
                ui.add_space(ITEM_GAP);
                if changing {
                    desk_label(ui, "What should Claude change on your CV? Say it in your own words.", Type::NOTE, Ink::Muted);
                    let width = ui.available_width().min(DOC_MEASURE);
                    changed.set(text_field(ui, text, width, "Say what to change", CHANGE_LIMIT).changed());
                    ui.add_space(ITEM_GAP);
                }
                match &shown {
                    Shown::Loading => cv_check(ui, PageSet::Loading(LOADING_PAGES)),
                    Shown::Ready(total) => cv_check(ui, PageSet::Ready { pages: textures, of: *total, alt: &label }),
                    Shown::Failed(reason) => {
                        alert_slip(ui, "This PDF could not be shown here.", reason);
                    }
                }
            },
            |ui| {
                open.set(cv_slip(ui, &label, &sentence).clicked());
                side_slip(ui, "", "", |ui| {
                    slip_text(ui, "", "Spotted something? Ask for a change opens a field. Say it in your own words, such as \"Put the design system first\".")
                });
                reply_slip(ui, "Looks good sends Claude this reply:", CV_LOOKS_GOOD);
                show_all.set(show_all_slip(ui));
            },
            |ui| {
                if changing {
                    send.set(primary_button(ui, "Send").clicked());
                    cancel.set(button(ui, "Cancel").clicked());
                } else {
                    good.set(primary_button(ui, "Looks good").clicked());
                    change.set(button(ui, ASK_FOR_CHANGE).clicked());
                }
            },
            &why,
        );
        self.show_all |= show_all.get();
        self.change_presses(run, cx, changed.get(), change.get(), cancel.get(), send.get());
        if good.get() {
            self.refusal = run.cv_looks_good(cx.desk).err().map(|refusal| refusal.line().to_owned());
        }
        open.get().then_some(ChapterRequest::OpenPdf(file))
    }

    /// Asks `pages` for the CV's pages, which starts their render the first time, and keeps them as textures.
    fn pages(&mut self, ui: &Ui, pdf: &PdfDocument, pages: Option<&mut PageStore>) -> Shown {
        let key = (pdf.path.clone(), pdf.modified);
        let Some(store) = pages else { return Shown::Loading };
        match store.pages(pdf) {
            PageState::Loading => Shown::Loading,
            PageState::Ready(rendered) => {
                if self.textures.as_ref().is_none_or(|(shown, _)| *shown != key) {
                    let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
                    let textures = rendered
                        .pages
                        .iter()
                        .enumerate()
                        .map(|(index, image)| {
                            let color = ColorImage::from_rgba_unmultiplied([image.width, image.height], &image.rgba);
                            ui.ctx().load_texture(format!("setup_cv_page_{index}"), color, options)
                        })
                        .collect();
                    self.textures = Some((key, textures));
                }
                Shown::Ready(rendered.total.max(rendered.pages.len()))
            }
            PageState::Failed(reason) => Shown::Failed(plain(&reason)),
        }
    }

    /// The question or permission Claude raised, in the stamp ink, as Tasks shows it. Only the user's press answers it.
    fn choice(&mut self, ui: &mut Ui, run: &mut ProfileRun, cx: &mut Chapter) {
        let Some(prompt) = run.run(cx.desk).and_then(Run::current_prompt).cloned() else { return };
        if self.picks_for.as_deref() != Some(prompt.request_id.as_str()) {
            self.picks_for = Some(prompt.request_id.clone());
            self.picks.clear();
        }
        let why = elapsed(run.run(cx.desk), cx.desk);
        let decided: Cell<Option<Decision>> = Cell::new(None);
        match &prompt.kind {
            PromptKind::Questions(questions) => {
                self.picks.resize(questions.len(), BTreeSet::new());
                let picks = &mut self.picks;
                let complete = picks.iter().all(|picked| !picked.is_empty());
                let answers = answers_of(questions, picks);
                let several = questions.iter().any(|question| question.multi_select);
                let lead = if several { "Pick the answers that fit, then send them." } else { "Pick one, then send your answer." };
                split_card(
                    ui,
                    &WizardStep { steps: &STEPS, at: AT, title: "Claude has a question", lead },
                    |ui| questions_card(ui, questions, picks),
                    |ui| {
                        side_slip(ui, "", "", |ui| {
                            slip_text(ui, "", "This card shows when Claude asks with set choices. Most of setup's questions come as a list instead.")
                        });
                    },
                    |ui| {
                        if enabled_when(ui, complete, "Pick an answer for each question.", |ui| primary_button(ui, "Send answer")).clicked() {
                            decided.set(Some(Decision::Answer(answers.clone())));
                        }
                        if button(ui, "Decline").clicked() {
                            decided.set(Some(Decision::Deny));
                        }
                    },
                    &why,
                );
            }
            PromptKind::Permission { tool, detail, can_remember } => {
                let lead = if *can_remember {
                    "Settings › Permissions asks before each tool. Allow it once, for the rest of this run, or deny it."
                } else {
                    "Settings › Permissions asks before each tool. Allow it once, or deny it."
                };
                split_card(
                    ui,
                    &WizardStep { steps: &STEPS, at: AT, title: "Claude asks to use a tool", lead },
                    |ui| permission_card(ui, tool, detail),
                    |ui| {
                        side_slip(ui, "", "", |ui| {
                            slip_text(ui, "", "Turn the prompts off in Settings › Permissions, and tools are allowed and noted in the task.")
                        });
                    },
                    |ui| {
                        // As Tasks offers them.
                        if primary_button(ui, "Allow once").clicked() {
                            decided.set(Some(Decision::Allow { remember: false }));
                        }
                        if *can_remember && button(ui, "Allow for this run").clicked() {
                            decided.set(Some(Decision::Allow { remember: true }));
                        }
                        if danger_button(ui, "Deny").clicked() {
                            decided.set(Some(Decision::Deny));
                        }
                    },
                    &why,
                );
            }
        }
        if let Some(decision) = decided.take() {
            run.decide(cx.desk, &prompt.request_id, decision);
            self.picks.clear();
        }
    }

    /// Setup stopped partway: why, what is kept, Pick up where it stopped and Finish later.
    fn stopped(&mut self, ui: &mut Ui, run: &mut ProfileRun, cx: &mut Chapter, reason: &str) -> Option<ChapterRequest> {
        let lines = run.work_lines(cx.desk);
        let files = run.documents().added().len();
        let why = self.refusal.clone().unwrap_or_default();
        let reason = plain(reason);
        let (pick_up, later) = (Cell::new(false), Cell::new(false));
        split_card(
            ui,
            &WizardStep { steps: &STEPS, at: AT, title: "Setup stopped partway", lead: "Nothing is lost. Claude picks up where it stopped." },
            |ui| {
                alert_slip(ui, "Claude stopped before setup finished.", &reason);
                work_paper(ui, &lines, files, None);
            },
            |ui| {
                side_slip(ui, "What is kept", "", |ui| {
                    slip_text(ui, "", "Your documents, your answers and every change Claude already wrote are in your folder.");
                    slip_text(ui, "", "Finish later opens Jobdesk. The next time you open it, setup offers to pick up here.");
                });
            },
            |ui| {
                pick_up.set(primary_button(ui, "Pick up where it stopped").clicked());
                later.set(button(ui, "Finish later").clicked());
                get_help(ui);
            },
            &why,
        );
        if pick_up.get() {
            self.refusal = run.pick_up(cx.desk, &cx.rules, cx.wall).err().map(|refusal| refusal.line());
        }
        later.get().then_some(ChapterRequest::FinishLater)
    }
}

/// What the CV check's pages area shows.
enum Shown {
    Loading,
    /// How many pages the PDF holds.
    Ready(usize),
    /// The renderer's reason, as plain text.
    Failed(String),
}

/// Claude working: the progress lines, what to do meanwhile, and Stop setup.
fn working(ui: &mut Ui, run: &mut ProfileRun, desk: &mut RunDesk, phase: &ProfilePhase) {
    let lines = run.work_lines(desk);
    let so_far = run.so_far(desk);
    let files = run.documents().added().len();
    let shown = run.run(desk);
    let live = shown.map(|shown| live_line(shown, desk));
    let why = match phase {
        ProfilePhase::Copying => "Claude starts once your documents are in your folder.".to_owned(),
        _ => elapsed(shown, desk),
    };
    let (title, lead) = match phase {
        ProfilePhase::Copying => ("Copying your documents", "Into your job search folder. Then Claude reads them."),
        ProfilePhase::Reading => ("Claude is reading your documents", "Then it writes to you with its first questions. This can take a few minutes."),
        _ => ("Claude is working on your profile", "With the answers you sent. This can take a few minutes."),
    };
    let moving = Cell::new(false);
    let stop = Cell::new(false);
    let copying = *phase == ProfilePhase::Copying;
    split_card(
        ui,
        &WizardStep { steps: &STEPS, at: AT, title, lead },
        |ui| moving.set(work_paper(ui, &lines, files, live.as_ref())),
        |ui| match phase {
            ProfilePhase::Working => {
                let rounds = &so_far.rounds;
                let has = !rounds.is_empty() || so_far.changes > 0;
                if has {
                    side_slip(ui, "So far", "", |ui| {
                        if !rounds.is_empty() {
                            let counts: Vec<String> = rounds.iter().map(|count| count.to_string()).collect();
                            let questions =
                                if counts.len() == 1 { format!("{} questions.", counts[0]) } else { format!("{} questions.", counts.join(", then ")) };
                            let answered = if rounds.len() == 1 { "1 round answered:".to_owned() } else { format!("{} rounds answered:", rounds.len()) };
                            slip_text(ui, &answered, &questions);
                        }
                        if so_far.changes > 0 {
                            let changes = if so_far.changes == 1 { "1 change".to_owned() } else { format!("{} changes", so_far.changes) };
                            slip_text(ui, &changes, "applied to your profile.");
                        }
                    });
                }
                side_slip(ui, "", "", |ui| slip_text(ui, "", "You can step away. Claude's next message shows here as soon as it comes."));
            }
            _ => {
                side_slip(ui, "While you wait", "", |ui| {
                    slip_text(ui, "", "You can leave this window open and do something else. Claude's questions show here as soon as they come.");
                });
            }
        },
        |ui| {
            if !copying {
                stop.set(button(ui, STOP_SETUP).clicked());
            }
        },
        &why,
    );
    if stop.get() {
        run.stop(desk);
    }
    // Elapsed and the last update count the seconds, and the sweep moves.
    let after = if moving.get() { SWEEP_FRAME } else { Duration::from_millis(500) };
    ui.ctx().request_repaint_after(after);
}

/// What the working line says under its title, and its sweep.
struct Live {
    under: String,
    form: SweepForm,
    seconds: f64,
}

fn live_line(run: &Run, desk: &RunDesk) -> Live {
    let now = desk.now();
    let seconds = run.elapsed.total(now).as_secs_f64();
    if run.status == RunStatus::Stopping {
        return Live { under: "Stopping…".to_owned(), form: SweepForm::Stopping, seconds };
    }
    let Some(heard) = run.last_heard else { return Live { under: "Starting…".to_owned(), form: SweepForm::Working, seconds } };
    match quiet(now.saturating_duration_since(heard)) {
        Quiet::Fresh(since) => Live { under: format!("Last update {}s ago", since.as_secs()), form: SweepForm::Working, seconds },
        Quiet::Quiet(since) => Live { under: format!("No update for {}. Long writes can be quiet.", short_time(since)), form: SweepForm::Quiet, seconds },
        Quiet::Long(since) => Live { under: format!("No update for {}. Long writes can be quiet.", short_time(since)), form: SweepForm::LongQuiet, seconds },
    }
}

/// "48s", or "3 min" past a minute.
fn short_time(since: Duration) -> String {
    let seconds = since.as_secs();
    if seconds < 60 { format!("{seconds}s") } else { format!("{} min", seconds / 60) }
}

/// The progress lines on paper. `files` counts the documents copied, and `live` says how the working line stands.
/// Returns whether its sweep moves.
fn work_paper(ui: &mut Ui, lines: &[WorkLine], files: usize, live: Option<&Live>) -> bool {
    let files = match files {
        0 => String::new(),
        1 => "1 file".to_owned(),
        count => format!("{count} files"),
    };
    paper(ui, false, |ui| {
        let mut moving = false;
        for (index, line) in lines.iter().enumerate() {
            let knot = (index + 1).to_string();
            let first_line = index == 0;
            let (state, end, under, sweep) = match line.state {
                LineState::Done => (KnotState::Done, LineEnd::Tag("Done", Tone::Hired), if first_line { files.as_str() } else { "" }, None),
                LineState::Working => {
                    let under = live.map_or("", |live| live.under.as_str());
                    (KnotState::Working, LineEnd::Nothing, under, live.map(|live| (live.form, live.seconds)))
                }
                LineState::Stopped => (KnotState::Failed, LineEnd::Tag("Stopped", Tone::Closed), "Stopped partway", None),
                LineState::Ahead => (KnotState::Ahead, LineEnd::Nothing, "", None),
            };
            moving |= setup_line(ui, &SetupLine { knot: &knot, state, title: line.text, under, end, bar: None, live: sweep }, first_line);
        }
        moving
    })
}

/// The questions of a prompt, each with its choices as pickable cards, in the stamp ink.
fn questions_card(ui: &mut Ui, questions: &[AskedQuestion], picks: &mut [BTreeSet<usize>]) {
    let several = questions.iter().any(|question| question.multi_select);
    prompt_box_tone(ui, Tone::Turn, |ui| {
        ui.horizontal(|ui| {
            solid_tag(ui, "Claude asks", Tone::Turn);
            let how = if several { "Pick the answers that fit, or decline." } else { "Pick one, or decline." };
            desk_label(ui, how, Type::NOTE, Ink::Muted);
        });
        for (index, question) in questions.iter().enumerate() {
            ui.add_space(ITEM_GAP);
            copyable(ui, |ui| desk_label(ui, plain(&question.text), Type::SLIP_TITLE, Ink::Text));
            if question.multi_select {
                desk_label(ui, "Pick as many as fit.", Type::NOTE, Ink::Muted);
            }
            let Some(picked) = picks.get_mut(index) else { continue };
            for (at, option) in question.options.iter().enumerate() {
                let on = picked.contains(&at);
                let (label, description) = (plain(&option.label), plain(&option.description));
                // Several answers are checkboxes, as Tasks shows them, and one answer a radio card.
                let card = ui
                    .push_id(("setup_choice", index, at), |ui| {
                        if question.multi_select {
                            let mut ticked = on;
                            setting_check(ui, &mut ticked, &label, &description, "")
                        } else {
                            choice_card(ui, on, &label, None, &description)
                        }
                    })
                    .inner;
                if card.clicked() {
                    if question.multi_select {
                        if !picked.remove(&at) {
                            picked.insert(at);
                        }
                    } else {
                        picked.clear();
                        picked.insert(at);
                    }
                }
            }
        }
    });
}

/// The answers a prompt's picks give: each question's text with its picked labels.
fn answers_of(questions: &[AskedQuestion], picks: &[BTreeSet<usize>]) -> Vec<(String, String)> {
    questions
        .iter()
        .zip(picks)
        .filter(|(_, picked)| !picked.is_empty())
        .map(|(question, picked)| {
            let labels: Vec<&str> = picked.iter().filter_map(|at| question.options.get(*at)).map(|option| option.label.as_str()).collect();
            (question.text.clone(), labels.join(", "))
        })
        .collect()
}

/// A permission prompt: the tool and what it would run, as plain text.
fn permission_card(ui: &mut Ui, tool: &str, detail: &str) {
    prompt_box_tone(ui, Tone::Turn, |ui| {
        ui.horizontal(|ui| {
            solid_tag(ui, "Claude asks", Tone::Turn);
            desk_label(ui, format!("Claude wants to use {}", plain(tool)), Type::STRONG, Ink::Text);
        });
        desk_label(ui, "Not on the allow list in .claude/settings.json.", Type::NOTE, Ink::Muted);
        copyable(ui, |ui| desk_label(ui, plain(detail), Type::MONO, Ink::Text));
    });
}

/// Show all Claude wrote, on a slip of its own. Gives whether it was clicked.
fn show_all_slip(ui: &mut Ui) -> bool {
    side_slip(ui, "", "", |ui| link(ui, SHOW_ALL).clicked())
}

/// Stop setup on a sheet where Claude waits: the run's process ends, the draft stays, and the app opens as Finish
/// later opens it. Pick up, or the next reply, resumes the same session.
fn stop_setup(run: &mut ProfileRun, cx: &mut Chapter, pressed: bool) -> Option<ChapterRequest> {
    if !pressed {
        return None;
    }
    run.stop(cx.desk);
    Some(ChapterRequest::FinishLater)
}

/// The run's Elapsed, as Tasks counts it, or nothing before the run.
fn elapsed(run: Option<&Run>, desk: &RunDesk) -> String {
    run.map(|run| format!("Elapsed {}", elapsed_text(run.elapsed.total(desk.now()).as_millis() as u64))).unwrap_or_default()
}

/// When the draft last reached the record, on the wall clock, such as "14:32".
fn saved_at(run: &ProfileRun, cx: &Chapter) -> Option<String> {
    if run.is_unsaved() {
        return None;
    }
    let saved = run.last_draft_save()?;
    let ago = TimeDelta::from_std(cx.now.saturating_duration_since(saved)).unwrap_or_default();
    Some((cx.wall - ago).format("%H:%M").to_string())
}

/// `text` as plain text, without hidden characters.
fn plain(text: &str) -> String {
    visible_chars(text).collect()
}

/// A path as the screens show it, split into its folder, with its slash, and its last part.
fn split_path(shown: &str) -> (String, String) {
    match shown.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_owned()),
        None => (String::new(), shown.to_owned()),
    }
}

/// "Two" for 2, as a title starts.
fn capitalized(count: usize) -> String {
    match count {
        1 => "One".to_owned(),
        2 => "Two".to_owned(),
        3 => "Three".to_owned(),
        count => count.to_string(),
    }
}

/// "second" for 2.
fn ordinal(round: usize) -> &'static str {
    match round {
        1 => "first",
        2 => "second",
        3 => "third",
        4 => "fourth",
        5 => "fifth",
        _ => "next",
    }
}

/// Claude's message for a sheet or a review, from the blocks the parser read: its prose as Markdown, its titles as
/// plain words, and each numbered item in its place, its words with its lettered sub-items apart. The review card
/// leaves out a proposal's ask, which invites typing numbers; Show all Claude wrote still holds it.
fn lay_out(batch: &Batch, review: bool) -> Laid {
    let items: Vec<LaidItem> = batch
        .items()
        .map(|item| {
            let (mut words, mut subs) = (Vec::new(), Vec::new());
            for line in item.body.lines() {
                match lettered(line) {
                    Some(sub) if !words.is_empty() => subs.push(sub),
                    _ => words.push(line.trim()),
                }
            }
            LaidItem { number: item.number, text: words.join(" ").trim().to_owned(), subs, optional: item.optional }
        })
        .collect();
    let parts = batch
        .blocks
        .iter()
        .filter_map(|block| match block {
            MessageBlock::Prose(source) => Some(LaidPart::Prose(source.clone())),
            MessageBlock::Title(words) => Some(LaidPart::Group(plain(words))),
            MessageBlock::Item(number) => items.iter().position(|item| item.number == *number).map(LaidPart::Item),
            MessageBlock::Ask(source) => (!review).then(|| LaidPart::Prose(source.clone())),
        })
        .collect();
    Laid { parts, items }
}

/// A lettered sub-item, such as "a. A former manager", with or without a list mark before it.
fn lettered(line: &str) -> Option<(char, String)> {
    let line = line.trim_start();
    let line = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).unwrap_or(line);
    let mut chars = line.chars();
    let letter = chars.next().filter(char::is_ascii_lowercase)?;
    let rest = chars.as_str();
    let rest = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") "))?;
    Some((letter, rest.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onboarding::application::{Message, read_message};

    const THIRTEEN: &str = include_str!("../../../tests/fixtures/setup/thirteen_questions.md");
    const SIXTEEN: &str = include_str!("../../../tests/fixtures/setup/sixteen_changes.md");

    /// The prose blocks of `laid`, in order.
    fn prose(laid: &Laid) -> Vec<&str> {
        laid.parts
            .iter()
            .filter_map(|part| match part {
                LaidPart::Prose(text) | LaidPart::Group(text) => Some(text.as_str()),
                LaidPart::Item(_) => None,
            })
            .collect()
    }

    /// The items of `laid`, in the order they show.
    fn order(laid: &Laid) -> Vec<u32> {
        laid.parts
            .iter()
            .filter_map(|part| match part {
                LaidPart::Item(at) => Some(laid.items[*at].number),
                LaidPart::Prose(_) | LaidPart::Group(_) => None,
            })
            .collect()
    }

    #[test]
    fn every_block_of_a_batch_shows_in_order_with_the_items_in_place() {
        let Message::Questions(batch) = read_message(THIRTEEN) else { panic!("questions") };
        let laid = lay_out(&batch, false);
        let blocks = prose(&laid);
        assert!(blocks[0].starts_with("Thank you."), "the lead");
        assert!(matches!(&laid.parts[1], LaidPart::Group(title) if title == "About the roles you want"), "a bold line alone is a group's title");
        assert_eq!(blocks.len(), 6, "the lead and each group's title: {blocks:?}");
        assert_eq!(order(&laid), (1..=13).collect::<Vec<_>>());
        let second = &laid.items[1];
        assert_eq!(second.text, "Which of these fit you best?");
        assert_eq!(second.subs, [('a', "Individual contributor with deep craft".to_owned()), ('b', "Lead of a small team".to_owned())]);

        let Message::Proposal(proposal) = read_message(SIXTEEN) else { panic!("proposal") };
        let laid = lay_out(&proposal, true);
        let blocks = prose(&laid);
        assert_eq!(blocks[..2], ["I have what I need. Here is what I will write. Nothing changes until you say so.", "CLAUDE.md"]);
        assert!(!blocks.iter().any(|block| block.contains("list the numbers")), "the review leaves out the ask: {blocks:?}");
        assert_eq!(order(&laid).len(), 16);
    }

    /// A zero-width space before an item, odd line breaks or a numbered line in code never move a field or show a
    /// question twice: each field sits directly under its own question.
    #[test]
    fn each_question_shows_once_with_its_field_under_it() {
        for message in [
            "Intro.\n\n\u{200b}1. Which city?\n2. Which salary?\n\nThanks.",
            "Intro.\u{2029}\u{2029}1. Which city?\u{2028}2. Which salary?\u{2028}\u{2028}Thanks.",
            "Intro:\n\n```\n1. Which city?\n```\n\n1. Which city?\n2. Which salary?\n\nThanks.",
        ] {
            let Message::Questions(batch) = read_message(message) else { panic!("questions: {message:?}") };
            let laid = lay_out(&batch, false);
            assert_eq!(order(&laid), [1, 2], "{message:?}");
            assert!(matches!(laid.parts.last(), Some(LaidPart::Prose(text)) if text == "Thanks."), "{message:?}");
            assert!(prose(&laid).iter().all(|block| !block.contains("Which salary")), "no question shows twice: {message:?}");
            let at = laid.parts.iter().position(|part| matches!(part, LaidPart::Item(0))).unwrap();
            assert!(matches!(laid.parts[at + 1], LaidPart::Item(1)), "each field under its question: {message:?}");
        }
    }

    #[test]
    fn small_words_read_as_words() {
        assert_eq!(capitalized(2), "Two");
        assert_eq!(ordinal(2), "second");
        assert_eq!(short_time(Duration::from_secs(48)), "48s");
        assert_eq!(short_time(Duration::from_secs(190)), "3 min");
        assert_eq!(split_path("Documents/Job search"), ("Documents/".to_owned(), "Job search".to_owned()));
        assert!(lettered("   a. A former manager").is_some());
        assert!(lettered("Answer in any order.").is_none());
    }
}
