//! First-run setup's screens in the small first window: the welcome, or the welcome back, on the desk, then the Folder
//! and Computer steps, one wizard card each. They read the setup's stage and record, and say what the user asked for.

use std::cell::Cell;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use egui::{Align, Frame, Label, Layout, Margin, OpenUrl, Rect, Response, Sense, Ui, UiBuilder, pos2, vec2};

use crate::onboarding::application::{Arch, Failure, HELPERS, Helper, HelperLine, InstallPhase, Onboarding, Phase, Place, Progress, Stage, Step, Work};
use crate::ui_kit::tokens::{BUTTON_GAP, BUTTON_HEIGHT, GUTTER, HEAD_GAP, INSET, ITEM_GAP, ROW_GAP, ROW_PADDING, SCREEN_PADDING};
use crate::ui_kit::{
    Ink, KnotState, LineEnd, SetupLine, Tone, Type, WizardStep, alert_slip, button, choice_card, day_stamp, desk_label, enabled_when, link, paper, path_label,
    primary_button, rich, setup_line, setup_stamp, tag, wizard_card, wrapped_height,
};

/// The user guide's part on first run, which Get help opens.
pub const GET_HELP: &str = "https://github.com/Nicolas-Widart-SRL/jobdesk/blob/main/docs/guide.md#first-run";

/// The steps the dots and the stamp count.
const STEPS: [&str; 4] = ["Folder", "Computer", "Profile", "Done"];
/// The widest a step's card gets in the first window.
const CARD_WIDTH: f32 = 640.0;

const WELCOME_TITLE: &str = "Welcome to Jobdesk";
const WELCOME_LEAD: &str = "Your job search as a short list of decisions. Claude does the legwork, and nothing runs until you say so.";
const WELCOME_WHY: &str = "Each step says what it does before it does it.\nQuit any time: setup picks up where you stopped.";
const FROM_SCRATCH: &str = "Set up from scratch";
const FROM_SCRATCH_SAYS: &str = "Jobdesk makes your job search folder, gets this computer ready and builds your profile from your CV. About 40 minutes, most of it answering Claude's questions about you.";
const HAVE_FOLDER: &str = "I already have an ai-job-search folder";
const HAVE_FOLDER_SAYS: &str = "Choose it, and Jobdesk opens it as it is.";
const PLACE_TITLE: &str = "Where should your job search live?";
const FOLDER_TITLE: &str = "Getting your folder ready";
const COMPUTER_TITLE: &str = "Get this computer ready";
const SAFE_HELPERS: &str = "They install in your own user folder, only for you. Nothing else on this computer changes.";
const WITHOUT_LATEX: &str =
    "Without LaTeX, Jobdesk still finds and scores jobs, but cannot make PDFs of your CV and cover letters. Get help shows how to add it later.";
const NEEDS_BOTH: &str = "Jobdesk needs Python and Bun for its tasks. Check that this computer is online, then try again.";

/// What the user asked for on a first-run screen. The shell follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRunRequest {
    /// Start setup with Set up from scratch picked.
    StartFromScratch,
    /// The folder picker for an ai-job-search folder the user already has, which then opens as today.
    HaveFolder,
    /// Continue setup, on the welcome back.
    Resume,
    Back,
    /// Change…: the folder picker for where the job search lives.
    ChangeFolder,
    UseSuggestion,
    /// Save here, and Try again after the download failed.
    SaveHere,
    Cancel,
    /// Continue, once the folder is ready.
    GoOn,
    /// Install what is missing, and Try again after a helper failed.
    Install,
    /// Continue, once every helper works.
    Finish,
    WithoutPdfs,
}

/// The two ways the welcome offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Way {
    #[default]
    FromScratch,
    HaveFolder,
}

/// The request a screen's parts take, from whichever of them was pressed.
type Asked = Cell<Option<FirstRunRequest>>;

/// Takes `request` when `response` was clicked.
fn press(response: Response, request: FirstRunRequest, asked: &Asked) {
    if response.clicked() {
        asked.set(Some(request));
    }
}

/// The first-run screens' own state: the way picked on the welcome, and the home folder paths are shown from.
pub struct FirstRunScreen {
    way: Way,
    home: PathBuf,
}

impl FirstRunScreen {
    /// Paths under `home` show from it, such as `Documents/Job search`.
    pub fn new(home: PathBuf) -> Self {
        Self { way: Way::default(), home }
    }

    /// The screen for where `onboarding` stands. `refused` says the folder picked as an ai-job-search folder was not one.
    pub fn show(&mut self, ui: &mut Ui, onboarding: &Onboarding, today: NaiveDate, refused: bool) -> Option<FirstRunRequest> {
        let asked = Asked::default();
        match onboarding.stage() {
            Stage::Welcome if onboarding.progress().is_done(Step::Folder) => self.welcome_back(ui, onboarding.progress(), &asked),
            Stage::Welcome => self.welcome(ui, onboarding.can_download(), today, refused, &asked),
            Stage::Place { folder, place } => self.place(ui, folder, place, &asked),
            Stage::Download { folder, version, phase, cancelling } => {
                let lines = FolderLines { shown: self.shown(folder), version: version.as_deref(), at: phase_line(*phase), failure: None };
                download(ui, &lines, *cancelling, &asked);
            }
            Stage::Done { folder, version } => {
                download(ui, &FolderLines { shown: self.shown(folder), version: Some(version), at: 4, failure: None }, false, &asked)
            }
            Stage::Failed { folder, failure, version, at } => {
                let lines = FolderLines { shown: self.shown(folder), version: version.as_deref(), at: failed_line(*failure, *at), failure: Some(*failure) };
                failed(ui, &lines, *failure, &asked);
            }
            Stage::Computer { lines, work, .. } => computer(ui, lines, *work, &asked),
            Stage::Finished { .. } => {}
        }
        asked.get()
    }

    /// `path` as the screens show it: from the home folder when it is in it, `/` between its parts.
    fn shown(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(inside) if !inside.as_os_str().is_empty() => inside.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>().join("/"),
            _ => path.display().to_string(),
        }
    }

    fn welcome(&mut self, ui: &mut Ui, can_download: bool, today: NaiveDate, refused: bool, asked: &Asked) {
        // A build that can vouch for no release offers only the folder the user already has.
        let way = Cell::new(if can_download { self.way } else { Way::HaveFolder });
        on_desk(
            ui,
            |ui| {
                let date = |pattern| today.format(pattern).to_string();
                desk_head(ui, |ui| day_stamp(ui, &date("%a"), &date("%-d"), &date("%b"), 0, 0, &date("%A %-d %B")), WELCOME_TITLE, WELCOME_LEAD);
                ui.spacing_mut().item_spacing.y = ROW_GAP;
                if can_download && choice_card(ui, way.get() == Way::FromScratch, FROM_SCRATCH, Some(("Recommended", Tone::Plain)), FROM_SCRATCH_SAYS).clicked()
                {
                    way.set(Way::FromScratch);
                }
                if choice_card(ui, way.get() == Way::HaveFolder, HAVE_FOLDER, None, HAVE_FOLDER_SAYS).clicked() {
                    way.set(Way::HaveFolder);
                }
                if refused {
                    ui.add_space(ITEM_GAP);
                    alert_slip(ui, "That is not an ai-job-search folder.", "Choose the folder ai-job-search is in, or set up from scratch.");
                }
            },
            |ui| {
                let (label, request) = match way.get() {
                    Way::FromScratch => ("Start setup", FirstRunRequest::StartFromScratch),
                    Way::HaveFolder => ("Choose folder…", FirstRunRequest::HaveFolder),
                };
                press(primary_button(ui, label), request, asked);
            },
            WELCOME_WHY,
        );
        if can_download {
            self.way = way.get();
        }
    }

    /// The welcome back, with the Folder step done. The record is cleared once the Computer step is, so setup always
    /// picks up there.
    fn welcome_back(&self, ui: &mut Ui, progress: &Progress, asked: &Asked) {
        let at = 1;
        let lead = "Your folder is ready. Let's get this computer ready.";
        let folder = progress.folder.as_deref().map(|folder| self.shown(folder)).unwrap_or_default();
        let tools = "Jobdesk checks Python, Bun and LaTeX, and installs what is missing.";
        let unders = [folder.as_str(), tools, "Claude builds it from your CV and your answers.", ""];
        let why = progress.stopped.map(|day| format!("You stopped on {}.", day.format("%A %-d %B"))).unwrap_or_default();
        on_desk(
            ui,
            |ui| {
                desk_head(ui, |ui| setup_stamp(ui, at, STEPS.len()), "Welcome back", lead);
                paper(ui, false, |ui| {
                    for (index, (name, under)) in STEPS.into_iter().zip(unders).enumerate() {
                        let (state, end, under) = match index.cmp(&at) {
                            Ordering::Less => (KnotState::Done, LineEnd::Tag("Done", Tone::Hired), under),
                            Ordering::Equal => (KnotState::Next, LineEnd::Nothing, under),
                            Ordering::Greater => (KnotState::Ahead, LineEnd::Nothing, ""),
                        };
                        let knot = (index + 1).to_string();
                        setup_line(ui, &SetupLine { knot: &knot, state, title: name, under, end, bar: None, live: None }, index == 0);
                    }
                });
            },
            |ui| press(primary_button(ui, "Continue setup"), FirstRunRequest::Resume, asked),
            &why,
        );
    }

    fn place(&self, ui: &mut Ui, folder: &Path, place: &Place, asked: &Asked) {
        let shown = self.shown(folder);
        match place {
            Place::New | Place::Empty => {
                let says = match place {
                    Place::Empty => "An empty folder. Jobdesk sets up in it as it is.",
                    _ if folder.parent().is_some_and(|parent| parent == self.home.join("Documents")) => "A new folder in your Documents folder.",
                    _ => "A new folder, made when you press Save here.",
                };
                let lead = "Jobdesk makes one folder for it. Your CV, the jobs it finds and your applications all go there.";
                card(
                    ui,
                    &WizardStep { steps: &STEPS, at: 0, title: PLACE_TITLE, lead },
                    |ui| {
                        press(folder_row(ui, &shown, says, None), FirstRunRequest::ChangeFolder, asked);
                        desk_label(
                            ui,
                            "Next, Jobdesk downloads ai-job-search into it: the free job search toolkit that Jobdesk runs on.",
                            Type::BODY,
                            Ink::Text,
                        );
                    },
                    |ui| {
                        press(primary_button(ui, "Save here"), FirstRunRequest::SaveHere, asked);
                        press(button(ui, "Back"), FirstRunRequest::Back, asked);
                    },
                    "Your files stay on this computer. Claude reads them only while a task runs.",
                );
            }
            Place::NotEmpty { suggest } => {
                let suggested = self.shown(suggest);
                card(
                    ui,
                    &WizardStep { steps: &STEPS, at: 0, title: PLACE_TITLE, lead: "That folder already has files in it." },
                    |ui| {
                        let detail = format!("{shown} holds other files, and setting up there could overwrite them.");
                        alert_slip(ui, "Jobdesk sets up only in a new or empty folder.", &detail);
                        folder_row(ui, &suggested, "A new folder inside the one you chose.", Some("Suggested"));
                    },
                    |ui| {
                        press(primary_button(ui, "Use this new folder"), FirstRunRequest::UseSuggestion, asked);
                        press(button(ui, "Change…"), FirstRunRequest::ChangeFolder, asked);
                    },
                    "An ai-job-search folder? Open it from the welcome.",
                );
            }
            Place::AlreadyAiJobSearch => {
                card(
                    ui,
                    &WizardStep { steps: &STEPS, at: 0, title: PLACE_TITLE, lead: "That folder already holds ai-job-search." },
                    |ui| {
                        let detail = format!("{shown} is an ai-job-search folder already, so Jobdesk leaves it as it is.");
                        alert_slip(ui, "Jobdesk sets up only in a new or empty folder.", &detail);
                    },
                    |ui| {
                        press(primary_button(ui, "Change…"), FirstRunRequest::ChangeFolder, asked);
                        press(button(ui, "Back"), FirstRunRequest::Back, asked);
                    },
                    "To open it as it is, go back and choose I already have an ai-job-search folder.",
                );
            }
        }
    }
}

/// Runs a step's card in the room left.
fn card(ui: &mut Ui, step: &WizardStep, body: impl FnOnce(&mut Ui), actions: impl FnOnce(&mut Ui), why: &str) {
    wizard_card(ui, step, CARD_WIDTH, body, actions, why);
}

/// Get help, as a link set apart from the buttons: the user guide's part on first run.
fn get_help(ui: &mut Ui) {
    ui.add_space(f32::from(GUTTER) - BUTTON_GAP);
    if link(ui, "Get help").clicked() {
        ui.ctx().open_url(OpenUrl::new_tab(GET_HELP));
    }
}

/// A screen straight on the desk, with no card: `body` from the top, and the foot pinned at the bottom with the one
/// primary at the left and `why` in `muted` at the right.
fn on_desk(ui: &mut Ui, body: impl FnOnce(&mut Ui), primary: impl FnOnce(&mut Ui), why: &str) {
    let full = ui.available_rect_before_wrap();
    let room = Rect::from_min_max(
        pos2(full.left() + SCREEN_PADDING.0, full.top()),
        pos2(full.right() - SCREEN_PADDING.0, full.bottom() - f32::from(GUTTER) - ITEM_GAP),
    );
    let foot_height = BUTTON_HEIGHT.max(wrapped_height(ui, why, Type::NOTE, room.width() / 2.0));
    let foot = Rect::from_min_max(pos2(room.left(), room.bottom() - foot_height), room.max);
    let top = Rect::from_min_max(room.min, pos2(room.right(), foot.top() - f32::from(GUTTER)));
    ui.scope_builder(UiBuilder::new().id_salt("first_run_body").max_rect(top).layout(Layout::top_down(Align::Min)), |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
    });
    ui.scope_builder(UiBuilder::new().id_salt("first_run_foot").max_rect(foot).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        primary(ui);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(Label::new(rich(ui, why, Type::NOTE, Ink::Muted)).halign(Align::Max).wrap());
        });
    });
    ui.allocate_rect(full, Sense::hover());
}

/// The head of a screen on the desk: its stamp beside the title over the lead, which wraps.
fn desk_head(ui: &mut Ui, stamp: impl FnOnce(&mut Ui) -> Response, title: &str, lead: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = HEAD_GAP.0;
        stamp(ui);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
            desk_label(ui, title, Type::CLEAR, Ink::Text);
            desk_label(ui, lead, Type::TAKE, Ink::Muted);
        });
    });
    ui.add_space(HEAD_GAP.1);
}

/// The folder on paper: its path over a sentence, then Change… at the right, or a tag. Gives Change…'s response, or the
/// tag's.
fn folder_row(ui: &mut Ui, shown: &str, says: &str, tagged: Option<&str>) -> Response {
    let (dir, name) = match shown.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_owned()),
        None => (String::new(), shown.to_owned()),
    };
    paper(ui, false, |ui| {
        Frame::new()
            .inner_margin(Margin::symmetric(INSET, ROW_PADDING + 2))
            .show(ui, |ui| {
                // As tall as Change…, with the words centred on it.
                ui.allocate_ui_with_layout(vec2(ui.available_width(), BUTTON_HEIGHT + 4.0), Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = f32::from(GUTTER);
                    let end = match tagged {
                        Some(text) => tag(ui, text, Tone::Plain),
                        None => button(ui, "Change…"),
                    };
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            path_label(ui, &dir, &name);
                            desk_label(ui, says, Type::NOTE, Ink::Muted);
                        });
                    });
                    end
                })
                .inner
            })
            .inner
    })
}

/// What the Folder step's lines show: the line working or failed, counting from 0, or 4 once all are done.
struct FolderLines<'a> {
    shown: String,
    version: Option<&'a str>,
    at: usize,
    failure: Option<Failure>,
}

/// The line a download phase works on.
fn phase_line(phase: Phase) -> usize {
    match phase {
        Phase::Made | Phase::Downloading { .. } => 1,
        Phase::Checking => 2,
        Phase::Unpacking => 3,
    }
}

/// The line a failure stopped: by what failed, or by where the download was for a failure any line could have.
fn failed_line(failure: Failure, at: Phase) -> usize {
    match failure {
        Failure::NotEmpty => 0,
        Failure::Offline | Failure::Refused(_) | Failure::NoPin => 1,
        Failure::Mismatch => 2,
        Failure::BadArchive => 3,
        Failure::Disk if at == Phase::Made => 0,
        _ => phase_line(at),
    }
}

/// The folder made and filled, working or done.
fn download(ui: &mut Ui, lines: &FolderLines, cancelling: bool, asked: &Asked) {
    let done = lines.at == 4;
    let lead = if done { "Your folder is ready." } else { "This takes under a minute." };
    let why = match (done, cancelling) {
        (true, _) => "Next, Jobdesk checks this computer.",
        (false, true) => "Stopping the download…",
        (false, false) => "Continue opens when the folder is ready.",
    };
    card(
        ui,
        &WizardStep { steps: &STEPS, at: 0, title: FOLDER_TITLE, lead },
        |ui| {
            folder_lines(ui, lines);
            let record = lines.version.map_or_else(|| "its own record of it".to_owned(), |version| format!("its own record of version {version}"));
            desk_label(ui, format!("Jobdesk compares the download with {record} before it unpacks anything."), Type::NOTE, Ink::Muted);
        },
        |ui| {
            press(enabled_when(ui, done, why, |ui| primary_button(ui, "Continue")), FirstRunRequest::GoOn, asked);
            if !done {
                press(enabled_when(ui, !cancelling, why, |ui| button(ui, "Cancel")), FirstRunRequest::Cancel, asked);
            }
        },
        why,
    );
}

/// The download stopped: what happened, what is kept, Try again, Back and Get help.
fn failed(ui: &mut Ui, lines: &FolderLines, failure: Failure, asked: &Asked) {
    let (title, detail) = download_failure(failure, lines.version);
    let lead = match lines.at {
        0 | 1 => "The download did not finish.",
        2 => "Jobdesk stopped before unpacking.",
        _ => "Jobdesk stopped while unpacking.",
    };
    card(
        ui,
        &WizardStep { steps: &STEPS, at: 0, title: FOLDER_TITLE, lead },
        |ui| {
            // The alert and the lines say one thing, so they sit close, and the line that failed fits the first window.
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            alert_slip(ui, title, &detail);
            folder_lines(ui, lines);
        },
        |ui| {
            press(primary_button(ui, "Try again"), FirstRunRequest::SaveHere, asked);
            press(button(ui, "Back"), FirstRunRequest::Back, asked);
            get_help(ui);
        },
        "",
    );
}

/// The four lines of the Folder step, or once it failed, those up to the line that failed.
fn folder_lines(ui: &mut Ui, lines: &FolderLines) {
    let last = if lines.failure.is_some() { lines.at } else { 3 };
    paper(ui, false, |ui| {
        for index in 0..=last {
            let failed = lines.failure.filter(|_| index == lines.at);
            let state = match index.cmp(&lines.at) {
                Ordering::Less => KnotState::Done,
                Ordering::Equal if failed.is_some() => KnotState::Failed,
                Ordering::Equal => KnotState::Working,
                Ordering::Greater => KnotState::Ahead,
            };
            let (title, under, working) = match index {
                0 => ("Folder made", lines.shown.clone(), "Making…"),
                1 => {
                    let under = match (state, lines.version) {
                        (KnotState::Done, Some(version)) => format!("Version {version}"),
                        (KnotState::Working, Some(version)) => format!("The latest version, {version}"),
                        (KnotState::Working, None) => "Finding the latest version".to_owned(),
                        _ => String::new(),
                    };
                    ("Downloading ai-job-search", under, "Downloading…")
                }
                2 => ("Checking the download", if state == KnotState::Done { "It matches Jobdesk's record".to_owned() } else { String::new() }, "Checking…"),
                _ => ("Unpacking it into your folder", String::new(), "Unpacking…"),
            };
            let under = failed.map_or(under, |failure| failed_under(failure).to_owned());
            let end = match (state, failed) {
                (_, Some(Failure::Mismatch)) => LineEnd::Tag("No match", Tone::Closed),
                (_, Some(_)) => LineEnd::Tag("Stopped", Tone::Closed),
                (KnotState::Done, _) => LineEnd::Tag("Done", Tone::Hired),
                (KnotState::Working, _) => LineEnd::Tag(working, Tone::Active),
                _ => LineEnd::Nothing,
            };
            let knot = (index + 1).to_string();
            setup_line(ui, &SetupLine { knot: &knot, state, title, under: &under, end, bar: None, live: None }, index == 0);
        }
    });
}

/// What a failed Folder line says under its title.
fn failed_under(failure: Failure) -> &'static str {
    match failure {
        Failure::Offline => "Stopped: no connection",
        Failure::Refused(_) => "Stopped: the download site turned Jobdesk away for now",
        Failure::NoPin => "Stopped: no version Jobdesk can check",
        Failure::TooLarge => "It is larger than ai-job-search should be",
        Failure::Mismatch => "It does not match Jobdesk's record",
        Failure::BadArchive => "It holds a file that would land outside your folder",
        Failure::Disk => "This computer could not write it",
        Failure::NotEmpty => "The folder is no longer empty",
        Failure::DoesNotRun | Failure::Cancelled => "Stopped",
    }
}

/// The alert when the download stopped: what happened, then what is kept and what to do.
fn download_failure(failure: Failure, version: Option<&str>) -> (&'static str, String) {
    const EMPTY: &str = "Your folder is still empty.";
    match failure {
        Failure::Offline => (
            "Jobdesk could not reach the internet.",
            "Check that this computer is online, then try again. Your folder is still empty, so nothing is half done.".to_owned(),
        ),
        Failure::Refused(_) => ("The download site did not let Jobdesk in.", format!("It may be busy. Wait a few minutes, then try again. {EMPTY}")),
        Failure::NoPin => (
            "Jobdesk cannot download ai-job-search right now.",
            format!(
                "It has no record of a version it can check, so it downloads nothing. Try again later, or open a folder you already have from the welcome. {EMPTY}"
            ),
        ),
        Failure::Mismatch => {
            let known = version.map_or_else(|| "the ai-job-search".to_owned(), |version| format!("the ai-job-search {version}"));
            // One line, so the line that failed fits the first window under it.
            ("This download doesn't match. Nothing was saved.", format!("It is not {known} that Jobdesk knows. {EMPTY}"))
        }
        Failure::TooLarge => {
            ("This download is too large. Nothing was saved.", format!("It is larger than ai-job-search should be, so it was thrown away. {EMPTY}"))
        }
        Failure::BadArchive => (
            "This download is not safe to unpack. Nothing was saved.",
            format!("Part of it would land outside your folder, so none of it was unpacked. {EMPTY}"),
        ),
        Failure::Disk => ("Jobdesk could not write to this computer.", format!("The disk may be full, or the folder locked. {EMPTY}")),
        Failure::NotEmpty => {
            ("That folder is no longer empty.", "Something was put in it after you chose it, so Jobdesk left it alone. Go back and choose again.".to_owned())
        }
        Failure::DoesNotRun | Failure::Cancelled => ("The download stopped.", EMPTY.to_owned()),
    }
}

/// The Computer step: the three helpers by purpose, checked, then installed.
fn computer(ui: &mut Ui, lines: &[HelperLine; 3], work: Work, asked: &Asked) {
    let ready = lines.iter().filter(|line| line.ready()).count();
    let failed = lines.iter().filter(|line| matches!(line, HelperLine::Failed(_))).count();
    let missing = lines.contains(&HelperLine::Missing);
    let latex = lines[2];
    let latex_left = matches!(latex, HelperLine::Missing | HelperLine::Installing(_));
    // Continue without PDFs leaves out LaTeX alone: Python and Bun must work.
    let only_latex = failed > 0 && lines[0].ready() && lines[1].ready() && !latex.ready();
    let lead = match work {
        Work::Checking => "Your job search needs three free helpers. Jobdesk checks which ones this computer has, then installs the rest.".to_owned(),
        Work::Installing { .. } if latex_left => "Installing what was missing. LaTeX is the slow one: you can step away and come back.".to_owned(),
        Work::Installing { .. } => "Installing what was missing.".to_owned(),
        Work::Idle if failed > 0 => format!("{} {}", count_ready(ready), count_failed(failed)),
        Work::Idle if latex == HelperLine::Missing => "Jobdesk checked this computer. LaTeX is the slow one: you can step away while it installs.".to_owned(),
        Work::Idle if missing => "Jobdesk checked this computer. What is missing installs in a minute or two.".to_owned(),
        Work::Idle => "All three helpers are ready.".to_owned(),
    };
    let why = match work {
        Work::Checking => "Checking takes a few seconds.",
        Work::Installing { cancelling: true } => "Stopping the install…",
        Work::Installing { .. } => "Continue opens when all three are ready.",
        Work::Idle if failed > 0 => "",
        Work::Idle if missing => "Each download is checked before it is installed.",
        Work::Idle => "Next, Jobdesk opens your job search.",
    };
    card(
        ui,
        &WizardStep { steps: &STEPS, at: 1, title: COMPUTER_TITLE, lead: &lead },
        |ui| {
            paper(ui, false, |ui| {
                for (index, (helper, line)) in HELPERS.into_iter().zip(lines).enumerate() {
                    helper_line(ui, index, helper, *line, work);
                }
            });
            match work {
                Work::Installing { .. } => {
                    desk_label(ui, "Each download is compared with Jobdesk's own record of it before it is installed.", Type::NOTE, Ink::Muted);
                }
                Work::Idle if only_latex => {
                    desk_label(ui, WITHOUT_LATEX, Type::BODY, Ink::Text);
                }
                Work::Idle if failed > 0 => {
                    desk_label(ui, NEEDS_BOTH, Type::BODY, Ink::Text);
                }
                _ => {
                    desk_label(ui, SAFE_HELPERS, Type::NOTE, Ink::Muted);
                }
            }
        },
        |ui| match work {
            Work::Checking => {
                enabled_when(ui, false, why, |ui| primary_button(ui, "Install what is missing"));
                enabled_when(ui, false, why, |ui| button(ui, "Back"));
            }
            Work::Installing { cancelling } => {
                enabled_when(ui, false, why, |ui| primary_button(ui, "Continue"));
                press(enabled_when(ui, !cancelling, why, |ui| button(ui, "Cancel")), FirstRunRequest::Cancel, asked);
            }
            Work::Idle if failed > 0 => {
                press(primary_button(ui, "Try again"), FirstRunRequest::Install, asked);
                if only_latex {
                    press(link(ui, "Continue without PDFs"), FirstRunRequest::WithoutPdfs, asked);
                }
                get_help(ui);
            }
            Work::Idle if missing => {
                press(primary_button(ui, "Install what is missing"), FirstRunRequest::Install, asked);
                press(button(ui, "Back"), FirstRunRequest::Back, asked);
            }
            Work::Idle => {
                press(primary_button(ui, "Continue"), FirstRunRequest::Finish, asked);
                press(button(ui, "Back"), FirstRunRequest::Back, asked);
            }
        },
        why,
    );
}

/// "Two helpers are ready." and its like.
fn count_ready(ready: usize) -> String {
    match ready {
        0 => "No helper is ready yet.".to_owned(),
        1 => "One helper is ready.".to_owned(),
        count => format!("{} helpers are ready.", number(count)),
    }
}

/// "One could not be installed." and its like.
fn count_failed(failed: usize) -> String {
    format!("{} could not be installed.", number(failed))
}

fn number(count: usize) -> &'static str {
    match count {
        0 => "None",
        1 => "One",
        2 => "Two",
        _ => "Three",
    }
}

/// One helper's line: what it is for, its name under it, and how it stands.
fn helper_line(ui: &mut Ui, index: usize, helper: Helper, line: HelperLine, work: Work) {
    let installing = matches!(work, Work::Installing { .. });
    let state = match line {
        HelperLine::Here | HelperLine::Installed => KnotState::Done,
        HelperLine::Checking | HelperLine::Installing(_) => KnotState::Working,
        HelperLine::Failed(_) => KnotState::Failed,
        HelperLine::Missing => KnotState::Next,
        HelperLine::Next => KnotState::Ahead,
    };
    let under = match line {
        HelperLine::Missing | HelperLine::Installing(_) => format!("{}, {}", name(helper), cost(helper)),
        HelperLine::Failed(failure) => format!("{}. {}", name(helper), helper_failure(failure)),
        _ => name(helper).to_owned(),
    };
    let end = match line {
        HelperLine::Next => LineEnd::Note("Next"),
        HelperLine::Checking => LineEnd::Tag("Checking…", Tone::Plain),
        HelperLine::Here => LineEnd::Tag("Already here", Tone::Hired),
        HelperLine::Missing if installing => LineEnd::Note("Next"),
        HelperLine::Missing => LineEnd::Tag("To install", Tone::Plain),
        HelperLine::Installing(_) => LineEnd::Tag("Installing…", Tone::Active),
        HelperLine::Installed => LineEnd::Tag("Installed", Tone::Hired),
        HelperLine::Failed(_) => LineEnd::Tag("Could not install", Tone::Closed),
    };
    let bar = match line {
        HelperLine::Installing(phase) => Some(install_bar(phase)),
        _ => None,
    };
    let knot = (index + 1).to_string();
    setup_line(ui, &SetupLine { knot: &knot, state, title: purpose(helper), under: &under, end, bar, live: None }, index == 0);
}

/// What a helper is for, the line's headline.
fn purpose(helper: Helper) -> &'static str {
    match helper {
        Helper::Python => "Keeps your tracker",
        Helper::Bun => "Searches job sites",
        Helper::Latex => "Makes your CV and cover letter PDFs",
    }
}

fn name(helper: Helper) -> &'static str {
    match helper {
        Helper::Python => "Python",
        Helper::Bun => "Bun",
        Helper::Latex => "LaTeX",
    }
}

/// What a missing helper costs in plain words, from its measured download and time: "about 30 MB, under a minute".
fn cost(helper: Helper) -> String {
    // The sizes differ little between processors, and not at all for LaTeX, so a build for neither shows Apple silicon's.
    let megabytes = helper.download_size(Arch::of_this_build().unwrap_or(Arch::Arm64)) as f64 / 1_000_000.0;
    let size = ((megabytes / 10.0).round() as u64).max(1) * 10;
    let (fast, slow) = helper.seconds();
    let time = if slow <= 60 { "under a minute".to_owned() } else { format!("{} to {} minutes", fast.div_ceil(60), slow.div_ceil(60)) };
    let time = if helper == Helper::Latex { format!("the slow one: {time}") } else { time };
    format!("about {size} MB, {time}")
}

/// Why a helper could not install, under its name.
fn helper_failure(failure: Failure) -> &'static str {
    match failure {
        Failure::Offline => "The download stopped before it finished.",
        Failure::Refused(_) => "The download site turned Jobdesk away for now.",
        Failure::TooLarge | Failure::Mismatch => "The download did not match Jobdesk's record, so it was thrown away.",
        Failure::BadArchive => "The download held a file that would land outside its folder.",
        Failure::Disk => "It could not be written to this computer.",
        Failure::NotEmpty => "Something else already takes its place.",
        Failure::DoesNotRun => "It installed, but does not run.",
        Failure::NoPin | Failure::Cancelled => "It stopped.",
    }
}

/// The bar under an installing helper: the share downloaded, then the step it is on.
fn install_bar(phase: Option<InstallPhase>) -> (f32, &'static str) {
    match phase {
        None => (0.0, "Starting"),
        Some(InstallPhase::Downloading { received, total }) => ((received as f64 / total.max(1) as f64).clamp(0.0, 1.0) as f32, "Downloading"),
        Some(InstallPhase::Checking) => (1.0, "Checking each file"),
        Some(InstallPhase::Unpacking) => (1.0, "Unpacking"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_helper_names_its_measured_size_and_time() {
        assert_eq!(cost(Helper::Latex), "about 100 MB, the slow one: 1 to 4 minutes");
        assert_eq!(cost(Helper::Python), "about 30 MB, under a minute");
        assert_eq!(cost(Helper::Bun), "about 40 MB, 1 to 2 minutes");
    }

    #[test]
    fn paths_in_the_home_folder_show_from_it() {
        let screen = FirstRunScreen::new(PathBuf::from("/Users/sam"));
        assert_eq!(screen.shown(Path::new("/Users/sam/Documents/Job search")), "Documents/Job search");
        assert_eq!(screen.shown(Path::new("/Volumes/Work/Job search")), "/Volumes/Work/Job search");
        assert_eq!(screen.shown(Path::new("/Users/sam")), "/Users/sam");
    }

    #[test]
    fn a_failure_stops_the_line_it_belongs_to() {
        assert_eq!(failed_line(Failure::Offline, Phase::Made), 1, "the feed is read before the download");
        assert_eq!(failed_line(Failure::Mismatch, Phase::Checking), 2);
        assert_eq!(failed_line(Failure::Disk, Phase::Made), 0);
        assert_eq!(failed_line(Failure::Disk, Phase::Unpacking), 3);
        assert_eq!(failed_line(Failure::TooLarge, Phase::Downloading { received: 9 }), 1);
    }
}
