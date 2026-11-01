use std::collections::{HashMap, HashSet};

use egui::style::ScrollAnimation;
use egui::text::LayoutJob;
use egui::{Align, FontSelection, Frame, Grid, Layout, Margin, Popup, PopupCloseBehavior, Rect, RectAlign, Response, Role, ScrollArea, Sense, TextEdit, Ui, UiBuilder, Vec2, vec2};
use egui_commonmark::CommonMarkCache;

use crate::runs::application::{
    Activity, ActivityKind, BandText, BandTone, ClosingQuestion, Decision, Prompt, PromptKind, Question, QuickReply, Run, RunDesk, RunStatus, StepLine, StepState, ToolState, band,
    closing_question, elapsed_text, short_path, step_lines, stop_point, wants_reply,
};
use crate::ui_kit::tokens::{
    BAND_COMPACT_HEIGHT, BUTTON_HEIGHT, CODE_PADDING, EVENT_GAP, EVENT_NESTED, EVENT_PADDING, EVENT_WHO_WIDTH, ITEM_GAP, KNOT_SIZE_SMALL, OFFSET_SHADOW, PROMPT_PADDING, ROW_GAP, RUN_SIDE,
    SCREEN_GAP, STEP_LINE_GAP, STEP_LINE_HEIGHT, STEP_NAME_WIDTH,
};
use crate::ui_kit::{
    BandCell, CellLook, Ink, KnotState, Subtitle, Title, Tone, Type, button, button_width, code_box, danger_button, divider, document, list_row, markdown, muted, page_body, page_head, panel,
    panel_body, panel_header, path_label, pick_row, primary_button, prompt_box_tone, rich, slip, slip_bar, small_button, small_danger_button, solid_tag, split, step_line, strong, tag, turn_band,
    wrapped_height,
};

const ACTIVITY_SHOWN: usize = 400;
/// What the story keeps under the turn bar, its heading included, however much the bar asks for.
/// A prompt's least heights win over it: when room is short, the story shrinks, not the prompt.
const STORY_MIN_HEIGHT: f32 = 160.0;
/// Claude's closing question is cut after this many lines, with the full text on hover.
const QUESTION_ROWS: usize = 3;
const END_POPOVER_WIDTH: f32 = 320.0;
const END_SESSION: &str = "End session…";
const RUN_DETAILS: &str = "Run details";
/// Below this window width, the side column moves into the Run details popup.
const DETAILS_BELOW_WIDTH: f32 = 1100.0;
/// The Run details popup scrolls past this height.
const DETAILS_MAX_HEIGHT: f32 = 440.0;
/// The least the reply field keeps beside the quick replies before they move to a row of their own.
const REPLY_FIELD_MIN_WIDTH: f32 = 200.0;
/// A prompt's scrolling part: at least two choices or lines, at most what reads at a glance.
const CHOICES_MIN_HEIGHT: f32 = 60.0;
const CHOICES_MAX_HEIGHT: f32 = 260.0;
const DETAIL_MIN_HEIGHT: f32 = 40.0;
const DETAIL_MAX_HEIGHT: f32 = 180.0;
const DETAIL_GAP: f32 = 4.0;

#[derive(Default)]
pub struct RunScreen {
    selected: Option<String>,
    reply: String,
    /// Choices ticked so far, by request and question.
    picks: HashMap<(String, String), HashSet<String>>,
    /// The run and request the picks belong to.
    picks_for: Option<(String, String)>,
    /// The run and turn start whose reply field was offered focus, so it is offered once.
    focused_for: Option<(String, usize)>,
    /// The run whose End session popover is open, and whether Keep it open still has to take focus.
    ending: Option<(String, bool)>,
    /// Whether the Run details popup is open.
    details: bool,
    /// The steps whose actions are shown, by run. `None` is the one toggle of a command without steps.
    opened: HashSet<(String, Option<usize>)>,
    /// The run and message the story last opened on, so the user's own scrolling wins after it.
    opened_at: Option<(String, usize)>,
    markdown: CommonMarkCache,
}

/// A click in the head, reported out of `page_head`'s actions.
#[derive(Clone, Copy)]
enum HeadAction {
    Stop,
    ForceStop,
    AskToEnd,
    Details,
}

/// What the head reports: a click, and the buttons the popovers anchor on.
#[derive(Default)]
struct Head {
    action: Option<HeadAction>,
    end_button: Option<Response>,
    details_button: Option<Response>,
}

/// Why replies wait while another run works.
const BLOCKED: &str = "Another run is working. You can reply when it finishes.";

/// Whose turn it is, in words and tone.
fn status(run: &Run) -> (&str, Tone) {
    if wants_reply(run) {
        return ("Your turn", Tone::Turn);
    }
    match run.status {
        RunStatus::Starting => ("Starting", Tone::Active),
        RunStatus::Stopping => (run.status.label(), Tone::Active),
        _ if run.background_agents > 0 => ("Claude's turn", Tone::Active),
        RunStatus::Working => ("Claude's turn", Tone::Active),
        // Claude finished and asks nothing: a reply is welcome, not awaited.
        RunStatus::AwaitingReply => (run.status.label(), Tone::Plain),
        RunStatus::Failed(_) => (run.status.label(), Tone::Closed),
        RunStatus::Stopped | RunStatus::Closed => (run.status.label(), Tone::Plain),
    }
}

/// Your turn is the one filled tag.
fn status_tag(ui: &mut Ui, run: &Run) {
    let (text, tone) = status(run);
    if tone == Tone::Turn {
        solid_tag(ui, text, tone);
    } else {
        tag(ui, text, tone);
    }
}

/// Whether the field sits under the story: the session can take a reply, and the turn bar does not ask for one.
/// That is a run that closed, failed or was stopped, or one that finished and asks nothing.
fn footer_reply(run: &Run) -> bool {
    let ended = matches!(run.status, RunStatus::Closed | RunStatus::Failed(_) | RunStatus::Stopped);
    run.session_id.is_some() && !run.is_busy() && !run.needs_user() && (ended || (run.awaits_reply() && !wants_reply(run)))
}

/// Whether anything sits under the story: word of background agents, or the reply field.
fn has_footer(run: &Run) -> bool {
    footer_reply(run) || run.background_agents > 0
}

/// Where the footer's measured height is kept, by run and by what the footer holds.
fn footer_height_id(run: &Run) -> egui::Id {
    egui::Id::NULL.with(("run_footer_height", &run.id, footer_reply(run)))
}

/// What a screen reader says for the turn bar.
fn turn_label(run: &Run, question: Option<&ClosingQuestion>) -> String {
    match run.current_prompt().map(|prompt| &prompt.kind) {
        Some(PromptKind::Permission { tool, .. }) => format!("Your turn. Claude wants to use {tool}"),
        Some(PromptKind::Questions(questions)) => format!("Your turn. Claude asks: {}", questions.first().map_or("", |question| question.text.as_str())),
        None => match question {
            Some(question) => format!("Your turn. Claude asks: {}", question.text),
            None => "Your turn. Claude finished this step.".to_owned(),
        },
    }
}

/// Whether the story opens on the newest message rather than following the run: at a stop, and once finished.
fn settled(run: &Run) -> bool {
    run.awaits_reply() || run.status == RunStatus::Closed
}

/// Rows the story shows outside any toggle: the user's words and Claude's own messages.
fn in_story(item: &Activity) -> bool {
    !item.nested && matches!(item.kind, ActivityKind::You(_) | ActivityKind::Claude(_))
}

fn knot_state(state: StepState) -> Option<KnotState> {
    match state {
        StepState::Done => Some(KnotState::Done),
        StepState::Working => Some(KnotState::Working),
        StepState::Waiting => Some(KnotState::Waiting),
        StepState::Failed => Some(KnotState::Failed),
        StepState::Next => None,
    }
}

fn mark_ink(state: StepState) -> Option<Ink> {
    match state {
        StepState::Done => Some(Ink::Text),
        StepState::Working => Some(Ink::Status(Tone::Active)),
        StepState::Waiting => Some(Ink::Status(Tone::Turn)),
        StepState::Failed => Some(Ink::Status(Tone::Closed)),
        StepState::Next => None,
    }
}

/// The band's tally, as runs of one ink.
fn tally_marks(states: &[StepState]) -> Vec<(usize, Option<Ink>)> {
    let mut marks: Vec<(usize, Option<Ink>)> = Vec::new();
    for state in states {
        let ink = mark_ink(*state);
        match marks.last_mut() {
            Some((count, last)) if *last == ink => *count += 1,
            _ => marks.push((1, ink)),
        }
    }
    marks
}

fn cell_look(text: &BandText) -> CellLook {
    match text.tone {
        BandTone::Plain => CellLook::Plain,
        BandTone::Claude => CellLook::Ink(Tone::Active),
        BandTone::Yours => CellLook::Yours,
        BandTone::Failed => CellLook::Ink(Tone::Closed),
    }
}

/// "Show 3 actions", "so far" while the step can still gain some, or "Hide actions" once open.
fn toggle_text(actions: usize, live: bool, open: bool) -> String {
    if open {
        return "Hide actions".to_owned();
    }
    let noun = if actions == 1 { "action" } else { "actions" };
    if live { format!("Show {actions} {noun} so far") } else { format!("Show {actions} {noun}") }
}

/// The rows of one part of the story, in the order they arrived. The command line, row 0, is the head's title.
struct Story<'a> {
    run: &'a Run,
    root: &'a str,
    /// The first row shown: older ones are in the run log.
    from: usize,
    /// The newest top-level message, which the story opens on when settled.
    newest: Option<usize>,
}

impl Story<'_> {
    fn rows(&self, step: Option<usize>) -> impl Iterator<Item = (usize, &Activity)> {
        self.run.activity.iter().enumerate().skip(self.from.max(1)).filter(move |(_, item)| item.step == step)
    }
}

impl RunScreen {
    pub fn select(&mut self, id: String) {
        self.selected = Some(id);
        self.reply.clear();
        self.picks.clear();
        self.picks_for = None;
    }

    /// `root` is the workspace folder, cut from the front of the paths the story shows.
    pub fn show(&mut self, ui: &mut Ui, desk: &mut RunDesk, root: &str) {
        let known = self.selected.as_ref().is_some_and(|id| desk.run(id).is_some());
        if !known {
            match desk.runs().first() {
                Some(first) => self.select(first.id.clone()),
                None => self.selected = None,
            }
        }
        let Some(run) = self.selected.as_ref().and_then(|id| desk.run(id)).cloned() else {
            page_head(ui, Title::Slab("Runs"), Some(Subtitle::Text("Every command Jobdesk ran in this workspace")), |_| {});
            page_body(ui, |ui| {
                panel(ui, |ui| {
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        strong(ui, "No runs yet");
                        muted(ui, "Press Ask or run to start one.");
                    });
                });
            });
            return;
        };
        let narrow = ui.ctx().content_rect().width() < DETAILS_BELOW_WIDTH;
        let mut head = Head::default();
        page_head(ui, Title::Typed(first_line(&run.line)), None, |ui| {
            let pass = head_actions(ui, &run, narrow);
            // The real pass comes last, so its buttons are the ones kept.
            head = Head { action: pass.action.or(head.action), ..pass };
        });
        match head.action {
            Some(HeadAction::Stop) => desk.stop(&run.id),
            Some(HeadAction::ForceStop) => desk.close(&run.id),
            Some(HeadAction::AskToEnd) => self.ending = Some((run.id.clone(), true)),
            Some(HeadAction::Details) => self.details = !self.details,
            None => {}
        }
        match head.end_button {
            Some(button) => self.end_popover(desk, &run, &button),
            None => self.ending = None,
        }
        match head.details_button {
            Some(button) => self.details_popup(desk, &run, &button),
            None => self.details = false,
        }
        page_body(ui, |ui| self.body(ui, desk, &run, root, narrow));
    }

    /// Asks before the session ends. Keep it open is the default, and Esc keeps it open too.
    fn end_popover(&mut self, desk: &mut RunDesk, run: &Run, button: &Response) {
        let (mut open, mut focus) = match &self.ending {
            Some((id, focus)) if *id == run.id => (true, *focus),
            _ => (false, false),
        };
        let (mut keep, mut end) = (false, false);
        Popup::from_response(button)
            .open_bool(&mut open)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .align(RectAlign::BOTTOM_END)
            .gap(ITEM_GAP)
            .width(END_POPOVER_WIDTH)
            .accessible_name("End this session?")
            .show(|ui| {
                ui.set_max_width(END_POPOVER_WIDTH);
                strong(ui, "End this session?");
                let note = "Ending stops the Claude process. A reply sent later resumes the session.";
                ui.add(egui::Label::new(rich(ui, note, Type::BODY, Ink::Muted)).wrap());
                ui.add_space(ITEM_GAP);
                ui.horizontal(|ui| {
                    let kept = primary_button(ui, "Keep it open");
                    if focus && !ui.is_sizing_pass() {
                        kept.request_focus();
                        focus = false;
                    }
                    keep = kept.clicked();
                    end = danger_button(ui, "End session").clicked();
                });
                ui.label(rich(ui, "Esc keeps it open.", Type::NOTE, Ink::Faint));
            });
        if end {
            desk.close(&run.id);
        }
        self.ending = (open && !keep && !end).then(|| (run.id.clone(), focus));
    }

    /// The side column, in a popup under Run details when the window is narrow.
    fn details_popup(&mut self, desk: &RunDesk, run: &Run, button: &Response) {
        let mut open = self.details;
        // It scrolls in what the window leaves under the button, up to its most.
        let room = button.ctx.content_rect().bottom() - button.rect.bottom() - ITEM_GAP - SCREEN_GAP;
        let height = room.clamp(0.0, DETAILS_MAX_HEIGHT);
        Popup::from_response(button)
            .open_bool(&mut open)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .align(RectAlign::BOTTOM_END)
            .gap(ITEM_GAP)
            .width(RUN_SIDE)
            .accessible_name(RUN_DETAILS)
            .show(|ui| {
                ui.set_width(RUN_SIDE);
                self.facts(ui, desk, run, Some(height));
            });
        // Picking a run closes the popup.
        self.details = open && self.details;
    }

    fn body(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, root: &str, narrow: bool) {
        let compact = ui.ctx().content_rect().height() < BAND_COMPACT_HEIGHT;
        let band = band(run);
        let marks = tally_marks(&band.marks);
        let cells = [
            BandCell { label: "Where", value: &band.place.value, note: &band.place.note, look: cell_look(&band.place), marks: &marks },
            BandCell { label: "Whose turn", value: &band.who.value, note: &band.who.note, look: cell_look(&band.who), marks: &[] },
            BandCell { label: "What next", value: &band.next.value, note: &band.next.note, look: cell_look(&band.next), marks: &[] },
        ];
        turn_band(ui, &cells, compact, "Where the run is", &band.tally);
        ui.add_space(ROW_GAP);
        if wants_reply(run) {
            self.turn_bar(ui, desk, run);
            ui.add_space(ROW_GAP);
        }
        if narrow {
            self.story(ui, desk, run, root);
        } else {
            split(ui, RUN_SIDE, |main, side| {
                self.facts(side, desk, run, None);
                self.story(main, desk, run, root);
            });
        }
    }

    /// The story, then the footer under it, outside the story's scroll area.
    /// The story takes what the footer left on the frame before. Each footer has its own measure:
    /// with none yet, it gets the whole column. A new measure redraws the frame, so the footer is never left below the window.
    fn story(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, root: &str) {
        let reserved_id = footer_height_id(run);
        let input = has_footer(run);
        let reserved = if input { ui.data(|data| data.get_temp::<f32>(reserved_id)).unwrap_or(ui.available_height()) } else { 0.0 };
        let settled = settled(run);
        let title = if run.status == RunStatus::Closed { "The run" } else { "The run so far" };
        panel(ui, |ui| {
            panel_header(ui, title, None);
            let room = (ui.available_height() - reserved).max(0.0);
            // While Claude works, the story follows it. Settled, it opens once on the newest message. Either way, the user's scrolling wins.
            let area = ScrollArea::vertical().id_salt(("run_story", &run.id)).max_height(room).stick_to_bottom(!settled).auto_shrink([false, true]);
            area.show(ui, |ui| {
                ui.set_width(ui.available_width());
                let anchor = self.story_rows(ui, run, root);
                if let Some((index, rect)) = anchor.filter(|_| settled) {
                    let key = (run.id.clone(), index);
                    if self.opened_at.as_ref() != Some(&key) {
                        ui.scroll_to_rect_animation(rect, Some(Align::TOP), ScrollAnimation::none());
                        if !ui.ctx().will_discard() {
                            self.opened_at = Some(key);
                        }
                    }
                }
            });
        });
        if !input {
            return;
        }
        // The cursor moves past a gap after each item, so this counts the gap below the footer.
        // The gap above it, between the story and the cursor, is added.
        let start = ui.cursor().top();
        self.footer(ui, desk, run);
        let used = ui.cursor().top() - start + ui.spacing().item_spacing.y;
        if (used - reserved).abs() > 0.5 {
            ui.data_mut(|data| data.insert_temp(reserved_id, used));
            // A discard redraws within this frame when a pass is left. The repaint covers the
            // frames where none is, so an idle window still settles on the new measure.
            ui.ctx().request_discard("the footer under the story changed height");
            ui.ctx().request_repaint();
        }
    }

    /// One line per started step, each followed by its messages and the user's replies.
    /// A command without steps has one toggle for all its actions, then its messages.
    /// Returns where the newest message sits, so a settled story can open on it.
    fn story_rows(&mut self, ui: &mut Ui, run: &Run, root: &str) -> Option<(usize, Rect)> {
        let from = run.activity.len().saturating_sub(ACTIVITY_SHOWN);
        let newest = run.activity.iter().rposition(|item| !item.nested && matches!(item.kind, ActivityKind::Claude(_)));
        let story = Story { run, root, from, newest };
        let mut first = true;
        let mut anchor = None;
        if from > 0 {
            panel_body(ui, |ui| muted(ui, format!("{from} earlier items are in the run log.")));
            first = false;
        }
        let lines = step_lines(run);
        if lines.is_empty() {
            self.part(ui, &story, None, None, &mut first, &mut anchor);
        } else {
            for line in &lines {
                self.part(ui, &story, Some(line.step), Some(line), &mut first, &mut anchor);
            }
        }
        if !run.streaming_text.is_empty() {
            separate(ui, &mut first);
            // Plain while it streams: the finished message renders as a document.
            Frame::new().inner_margin(Margin::symmetric(0, EVENT_PADDING as i8)).show(ui, |ui| {
                ui.add(egui::Label::new(rich(ui, run.streaming_text.as_str(), Type::TAKE, Ink::Text)).wrap());
            });
        }
        anchor
    }

    /// One step: its line and toggle, its actions when open, then its messages and replies.
    fn part(&mut self, ui: &mut Ui, story: &Story, step: Option<usize>, line: Option<&StepLine>, first: &mut bool, anchor: &mut Option<(usize, Rect)>) {
        let run = story.run;
        let key = (run.id.clone(), step);
        let open = self.opened.contains(&key);
        let tools = story.rows(step).filter(|(_, item)| matches!(item.kind, ActivityKind::Tool { .. })).count();
        let actions = line.map_or(tools, |line| line.actions);
        let live = line.map_or(run.is_busy(), |line| line.live);
        let toggle = (actions > 0).then(|| toggle_text(actions, live, open));
        let clicked = match line {
            Some(line) => {
                separate(ui, first);
                let number = line.step.to_string();
                let knot = knot_state(line.state).map(|state| (number.as_str(), state));
                step_line(ui, knot, line.name, &line.sentence, toggle.as_deref().map(|text| (text, open)))
            }
            None => toggle.as_deref().and_then(|text| {
                separate(ui, first);
                step_line(ui, None, "Actions", "", Some((text, open)))
            }),
        };
        if clicked.is_some_and(|toggle| toggle.clicked()) {
            if open {
                self.opened.remove(&key);
            } else {
                self.opened.insert(key);
            }
        }
        // With no action to count, there is no toggle, so the step's notes show as they are.
        if open || actions == 0 {
            let others: Vec<&Activity> = story.rows(step).map(|(_, item)| item).filter(|item| !in_story(item)).collect();
            if !others.is_empty() {
                let indent = KNOT_SIZE_SMALL + STEP_NAME_WIDTH + STEP_LINE_GAP * 2.0;
                Frame::new().inner_margin(Margin { left: indent as i8, ..Default::default() }).show(ui, |ui| {
                    for (index, item) in others.into_iter().enumerate() {
                        if index > 0 {
                            divider(ui);
                        }
                        self.action_row(ui, item, story.root);
                    }
                });
            }
        }
        for (index, item) in story.rows(step).filter(|(_, item)| in_story(item)) {
            match &item.kind {
                ActivityKind::You(text) => {
                    separate(ui, first);
                    you_line(ui, text);
                }
                ActivityKind::Claude(text) => {
                    *first = false;
                    let shown = Frame::new().inner_margin(Margin::symmetric(0, ROW_GAP as i8)).show(ui, |ui| document(ui, &mut self.markdown, text));
                    if story.newest == Some(index) {
                        *anchor = Some((index, shown.response.rect));
                    }
                }
                _ => {}
            }
        }
    }

    /// One action behind a step's toggle, as the activity showed it: its tool, what it touched and how it went.
    fn action_row(&mut self, ui: &mut Ui, item: &Activity, root: &str) {
        let indent = if item.nested { EVENT_NESTED } else { 0.0 };
        let padding = Margin { left: indent as i8, right: 0, top: EVENT_PADDING as i8, bottom: EVENT_PADDING as i8 };
        Frame::new().inner_margin(padding).show(ui, |ui| match &item.kind {
            ActivityKind::You(text) => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = EVENT_GAP;
                    who(ui, "You");
                    ui.add(egui::Label::new(rich(ui, text, Type::MONO, Ink::Text)).wrap());
                });
            }
            ActivityKind::Claude(text) => {
                markdown(ui, &mut self.markdown, text);
            }
            ActivityKind::Notice(text) => {
                muted(ui, text);
            }
            ActivityKind::Tool { name, summary, state, .. } => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = EVENT_GAP;
                    who(ui, name);
                    let (text, tone) = match state {
                        ToolState::Running => ("Running", Tone::Active),
                        ToolState::Done => ("Done", Tone::Hired),
                        ToolState::Failed(_) => ("Failed", Tone::Closed),
                    };
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        tag(ui, text, tone);
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let summary = first_line(summary);
                            if matches!(name.as_str(), "Read" | "Write" | "Edit" | "MultiEdit") {
                                let (dir, file) = short_path(root, summary);
                                path_label(ui, &dir, &file).on_hover_text(summary);
                            } else {
                                ui.add(egui::Label::new(rich(ui, summary, Type::MONO, Ink::Text)).truncate()).on_hover_text(summary);
                            }
                        });
                    });
                });
                if let ToolState::Failed(Some(reason)) = state {
                    failure_reason(ui, reason);
                }
            }
        });
    }

    /// Under the story, outside its scroll area: word of background agents, or the field that resumes the session.
    fn footer(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        divider(ui);
        Frame::new().inner_margin(Margin { top: ROW_GAP as i8, ..Default::default() }).show(ui, |ui| {
            if run.background_agents > 0 {
                muted(ui, background_note(run.background_agents));
            } else {
                let hint = if run.status == RunStatus::AwaitingReply { "Reply to Claude" } else { "Reply to resume this session" };
                self.reply_row(ui, desk, run, &[], hint, false);
            }
        });
    }

    /// Under the band while it is the user's turn, framed in the turn colour:
    /// the open prompt, or Claude's closing question and the reply row.
    fn turn_bar(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        let question = closing_question(run);
        // What the bar's content may take: the room above the story's least height and any footer under it,
        // less the bar's frame and the gaps under it. A footer not yet measured counts as nothing here.
        let (_, padding) = PROMPT_PADDING;
        let footer = if has_footer(run) { ui.data(|data| data.get_temp::<f32>(footer_height_id(run))).unwrap_or(0.0) } else { 0.0 };
        let gaps = ui.spacing().item_spacing.y + ROW_GAP;
        let budget = ui.available_height() - STORY_MIN_HEIGHT - footer - gaps - padding as f32 * 2.0 - OFFSET_SHADOW as f32;
        let group = UiBuilder::new().accessibility_role(Role::Group).accessibility_label(turn_label(run, question.as_ref()));
        ui.scope_builder(group, |ui| {
            prompt_box_tone(ui, Tone::Turn, |ui| match run.current_prompt().cloned() {
                Some(prompt) => match &prompt.kind {
                    PromptKind::Permission { tool, detail, can_remember } => permission(ui, desk, run, &prompt, tool, detail, *can_remember, budget),
                    PromptKind::Questions(questions) => self.questions(ui, desk, run, &prompt, questions, budget),
                },
                None => self.stop_reply(ui, desk, run, question.as_ref(), budget),
            });
        });
    }

    /// Claude's question, cut to a few lines, then one row: the stop point's quick replies, the field and Send.
    /// The question gives up lines first, so the blocked line and the reply row stay inside `budget`.
    fn stop_reply(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, question: Option<&ClosingQuestion>, budget: f32) {
        let replies = stop_point(run).map_or(&[][..], |stop| stop.replies);
        match question {
            Some(question) => {
                let gap = ui.spacing().item_spacing.y;
                let line = wrapped_height(ui, "Claude asks", Type::BODY, f32::INFINITY);
                let blocked = if desk.is_busy() { line + gap } else { 0.0 };
                let rows = (budget - blocked - reply_rows_height(ui, replies) - gap) / line;
                let rows = (rows.floor().max(1.0) as usize).min(QUESTION_ROWS);
                // Plain text: Claude's message is untrusted, and the question is read, not rendered.
                let style = ui.style().clone();
                let mut job = LayoutJob::default();
                for (text, ink) in [("Claude asks: ", Ink::Muted), (question.text.as_str(), Ink::Text)] {
                    rich(ui, text, Type::BODY, ink).append_to(&mut job, &style, FontSelection::Default, Align::Min);
                }
                job.wrap.max_rows = rows;
                ui.add(egui::Label::new(job).wrap()).on_hover_text(&question.text);
            }
            None => {
                muted(ui, "Claude finished this step. Read its message, then reply.");
            }
        }
        let hint = if replies.is_empty() { "Reply to Claude" } else { "Or write your own reply" };
        self.reply_row(ui, desk, run, replies, hint, true);
    }

    /// Quick replies, the field, then Send on its right. A quick reply sends its words as a typed reply would.
    /// When the quick replies would leave the field under `REPLY_FIELD_MIN_WIDTH`, they take a row of their own above it, wrapped.
    /// With `focus`, the field takes focus once per run and turn: on the turn's first frame, unless the user is typing in another field.
    fn reply_row(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, replies: &[QuickReply], hint: &str, focus: bool) {
        let blocked = desk.is_busy();
        if blocked {
            muted(ui, BLOCKED);
        }
        let gap = ui.spacing().item_spacing.x;
        let send_width = button_width(ui, "Send");
        let inline = inline_replies(ui, replies);
        let mut sent = None;
        let quick_replies = |ui: &mut Ui, sent: &mut Option<String>| {
            for reply in replies {
                // The words it sends show on hover, enabled or not. Why it waits is the muted line's to say.
                let response = ui.add_enabled_ui(!blocked, |ui| button(ui, reply.button)).inner.on_hover_text(reply.text).on_disabled_hover_text(reply.text);
                if response.clicked() {
                    *sent = Some(reply.text.to_owned());
                }
            }
        };
        if !inline && !replies.is_empty() {
            ui.horizontal_wrapped(|ui| quick_replies(ui, &mut sent));
        }
        ui.horizontal(|ui| {
            if inline {
                quick_replies(ui, &mut sent);
            }
            let room = ui.available_width() - send_width - gap;
            let edit = ui.add(TextEdit::singleline(&mut self.reply).hint_text(hint).desired_width(room));
            let turn = (run.id.clone(), run.current_turn_start());
            if focus && self.focused_for.as_ref() != Some(&turn) {
                // Offered once: when the user is typing in another field, this turn passes without taking it.
                // A focused row or tab does not count.
                let elsewhere = ui.ctx().text_edit_focused() && !edit.has_focus();
                if !elsewhere {
                    edit.request_focus();
                }
                self.focused_for = Some(turn);
            }
            let typed = !self.reply.trim().is_empty();
            let send = ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.add_enabled_ui(typed && !blocked, |ui| primary_button(ui, "Send")).inner.clicked()).inner;
            let entered = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (send || entered) && typed && !blocked {
                sent = Some(self.reply.trim().to_owned());
            }
        });
        if let Some(text) = sent {
            // A quick reply clears the field as Send does.
            self.reply.clear();
            let _ = desk.reply(&run.id, &text);
        }
    }

    fn questions(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, prompt: &Prompt, questions: &[Question], budget: f32) {
        // Picks belong to one request: a later prompt asking the same question starts blank.
        let owner = (run.id.clone(), prompt.request_id.clone());
        if self.picks_for.as_ref() != Some(&owner) {
            self.picks.clear();
            self.picks_for = Some(owner);
        }
        // The choices scroll in what the budget leaves above the gap and the buttons, but never under their least height.
        let room = budget - ui.spacing().item_spacing.y - BUTTON_HEIGHT;
        let height = prompt_height(room, CHOICES_MIN_HEIGHT, CHOICES_MAX_HEIGHT);
        ScrollArea::vertical().id_salt(("run_questions", &run.id, &prompt.request_id)).max_height(height).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // The choices sit in two columns, read across.
            let column = ((ui.available_width() - SCREEN_GAP) / 2.0).max(0.0);
            for (index, question) in questions.iter().enumerate() {
                strong(ui, &question.text);
                let picked = self.picks.entry((prompt.request_id.clone(), question.text.clone())).or_default();
                let grid = Grid::new(("run_choices", &prompt.request_id, index)).num_columns(2).min_col_width(column).max_col_width(column).spacing([SCREEN_GAP, ROW_GAP]);
                grid.show(ui, |ui| {
                    for (at, option) in question.options.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = DETAIL_GAP;
                            let mut on = picked.contains(&option.label);
                            let changed = if question.multi_select {
                                ui.checkbox(&mut on, &option.label).changed()
                            } else {
                                let clicked = ui.radio(on, &option.label).clicked();
                                on |= clicked;
                                clicked
                            };
                            if !option.description.is_empty() {
                                // Under the choice's words, past its radio or checkbox.
                                let indent = ui.spacing().icon_width + ui.spacing().icon_spacing;
                                Frame::new().inner_margin(Margin { left: indent as i8, ..Default::default() }).show(ui, |ui| {
                                    ui.add(egui::Label::new(rich(ui, &option.description, Type::BODY, Ink::Muted)).wrap());
                                });
                            }
                            if changed {
                                if !question.multi_select {
                                    picked.clear();
                                }
                                if on {
                                    picked.insert(option.label.clone());
                                } else {
                                    picked.remove(&option.label);
                                }
                            }
                        });
                        if at % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(6.0);
            }
        });
        let answers: Vec<(String, String)> = questions
            .iter()
            .filter_map(|question| {
                let picked = self.picks.get(&(prompt.request_id.clone(), question.text.clone()))?;
                let chosen: Vec<&str> = question.options.iter().map(|option| option.label.as_str()).filter(|label| picked.contains(*label)).collect();
                (!chosen.is_empty()).then(|| (question.text.clone(), chosen.join(", ")))
            })
            .collect();
        let complete = answers.len() == questions.len();
        ui.horizontal(|ui| {
            if ui.add_enabled_ui(complete, |ui| primary_button(ui, "Send answer")).inner.clicked() {
                desk.decide(&run.id, &prompt.request_id, Decision::Answer(answers.clone()));
                self.picks.clear();
            }
            if button(ui, "Decline").clicked() {
                desk.decide(&run.id, &prompt.request_id, Decision::Deny);
                self.picks.clear();
            }
        });
    }

    /// This run and Recent runs: the side column, or the Run details popup, `popup` tall at most.
    fn facts(&mut self, ui: &mut Ui, desk: &RunDesk, run: &Run, popup: Option<f32>) {
        let area = match popup {
            // A popup has no height of its own to fill, so its scroll area is told the height to take.
            Some(height) => ScrollArea::vertical().id_salt("run_details").auto_shrink([false, true]).max_height(height).min_scrolled_height(height),
            None => ScrollArea::vertical().auto_shrink([false, false]),
        };
        area.show(ui, |ui| {
            slip(ui, |ui| {
                panel_header(ui, "This run", None);
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = ROW_GAP;
                    fact(ui, "Cost", &format!("${:.2} of ${:.2}", run.cost_usd, run.budget_usd));
                    slip_bar(ui, (run.cost_usd / run.budget_usd.max(0.01)) as f32, Ink::Text);
                    fact(ui, "Elapsed", &elapsed_text(run.duration_ms));
                    if let Some(model) = &run.model {
                        fact(ui, "Model", model);
                    }
                    if let Some(session) = &run.session_id {
                        fact(ui, "Session", &session.chars().take(8).collect::<String>());
                    }
                    if let Some(usage) = run.usage.five_hour {
                        fact(ui, "5-hour usage", &format!("{:.0}%", usage * 100.0));
                    }
                    if let Some(usage) = run.usage.seven_day {
                        fact(ui, "7-day usage", &format!("{:.0}%", usage * 100.0));
                    }
                });
            });
            for (title, items) in [("Agents", &run.agents), ("Files changed", &run.files_changed)] {
                if items.is_empty() {
                    continue;
                }
                ui.add_space(ROW_GAP);
                slip(ui, |ui| {
                    panel_header(ui, title, None);
                    for (index, item) in items.iter().enumerate() {
                        list_row(ui, index == 0, |ui| {
                            ui.add(egui::Label::new(rich(ui, item, Type::MONO, Ink::Text)).truncate()).on_hover_text(item);
                        });
                    }
                });
            }
            ui.add_space(ROW_GAP);
            slip(ui, |ui| {
                panel_header(ui, "Recent runs", None);
                for (index, other) in desk.runs().iter().enumerate() {
                    if index > 0 {
                        divider(ui);
                    }
                    let on = other.id == run.id;
                    // Picking the run already shown keeps a reply being typed.
                    if self.recent_row(ui, other, on) && !on {
                        self.select(other.id.clone());
                        self.details = false;
                    }
                }
            });
        });
    }

    /// One run in the recent list. The picked run reads in full ink, the others muted.
    /// Its labels take no clicks, so a click anywhere on the row picks the run.
    fn recent_row(&self, ui: &mut Ui, run: &Run, on: bool) -> bool {
        let response = pick_row(ui, on, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(egui::Label::new(rich(ui, format!("${:.2}", run.cost_usd), Type::BODY, Ink::Faint)).selectable(false));
                    status_tag(ui, run);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let ink = if on { Ink::Text } else { Ink::Muted };
                        ui.add(egui::Label::new(rich(ui, first_line(&run.line), Type::MONO, ink)).truncate().selectable(false));
                    });
                });
            });
        });
        response.clicked()
    }
}

/// A hairline between story items, but not above the first.
fn separate(ui: &mut Ui, first: &mut bool) {
    if !*first {
        divider(ui);
    }
    *first = false;
}

/// The user's words in the story, in quotes, lined up with the step names.
fn you_line(ui: &mut Ui, text: &str) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(vec2(width, STEP_LINE_HEIGHT), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_width(width);
        ui.set_min_height(STEP_LINE_HEIGHT);
        ui.spacing_mut().item_spacing.x = STEP_LINE_GAP;
        // The knot's place stays empty.
        ui.allocate_exact_size(Vec2::splat(KNOT_SIZE_SMALL), Sense::hover());
        ui.allocate_ui_with_layout(vec2(STEP_NAME_WIDTH, KNOT_SIZE_SMALL), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(STEP_NAME_WIDTH);
            ui.label(rich(ui, "You", Type::STRONG, Ink::Text));
        });
        ui.add(egui::Label::new(rich(ui, format!("\u{201c}{text}\u{201d}"), Type::BODY, Ink::Text)).wrap());
    });
}

/// Whether the quick replies fit beside the field and Send.
fn inline_replies(ui: &Ui, replies: &[QuickReply]) -> bool {
    let gap = ui.spacing().item_spacing.x;
    let quick_width: f32 = replies.iter().map(|reply| button_width(ui, reply.button) + gap).sum();
    quick_width + REPLY_FIELD_MIN_WIDTH + gap + button_width(ui, "Send") <= ui.available_width()
}

/// The height of the reply row, with the quick replies' own wrapped rows above it when they do not fit beside the field.
fn reply_rows_height(ui: &Ui, replies: &[QuickReply]) -> f32 {
    let spacing = ui.spacing().item_spacing;
    if replies.is_empty() || inline_replies(ui, replies) {
        return BUTTON_HEIGHT;
    }
    let width = ui.available_width();
    let mut rows = 1;
    let mut used = 0.0;
    for reply in replies {
        let wide = button_width(ui, reply.button);
        if used > 0.0 && used + spacing.x + wide > width {
            rows += 1;
            used = 0.0;
        }
        used += if used > 0.0 { spacing.x + wide } else { wide };
    }
    (rows + 1) as f32 * (BUTTON_HEIGHT + spacing.y) - spacing.y
}

/// A prompt's scrolling part takes what `room` leaves, between its least and its most.
/// The least wins over the room: the story under the bar shrinks instead, so the prompt stays answerable.
fn prompt_height(room: f32, least: f32, most: f32) -> f32 {
    room.clamp(least, most)
}

/// Stop, force or end, by the run's state, then Run details when the window is narrow. Added right to left.
/// `page_head` runs this twice a frame, so it only reports a click, and the buttons to anchor the popovers on.
fn head_actions(ui: &mut Ui, run: &Run, narrow: bool) -> Head {
    let mut head = Head::default();
    if run.status == RunStatus::Stopping {
        if small_danger_button(ui, "Force stop").clicked() {
            head.action = Some(HeadAction::ForceStop);
        }
    } else if run.is_busy() || run.needs_user() {
        if small_danger_button(ui, "Stop run").clicked() {
            head.action = Some(HeadAction::Stop);
        }
    } else if run.awaits_reply() {
        let response = small_button(ui, END_SESSION);
        if response.clicked() {
            head.action = Some(HeadAction::AskToEnd);
        }
        head.end_button = Some(response);
    }
    if narrow {
        let response = small_button(ui, RUN_DETAILS);
        if response.clicked() {
            head.action = Some(HeadAction::Details);
        }
        head.details_button = Some(response);
    }
    head
}

/// `budget` is the height the prompt's content may take, buttons included.
#[allow(clippy::too_many_arguments)]
fn permission(ui: &mut Ui, desk: &mut RunDesk, run: &Run, prompt: &Prompt, tool: &str, detail: &str, can_remember: bool, budget: f32) {
    let top = ui.cursor().top();
    strong(ui, format!("Claude wants to use {tool}"));
    muted(ui, "Not on the allow list in .claude/settings.json.");
    // The command scrolls in what the budget leaves after the lines above it, the code box's padding,
    // the one gap after the box, the extra space and the buttons, but never under its least height.
    let above = ui.cursor().top() - top;
    let room = budget - above - CODE_PADDING.1 as f32 * 2.0 - ui.spacing().item_spacing.y - DETAIL_GAP - BUTTON_HEIGHT;
    let height = prompt_height(room, DETAIL_MIN_HEIGHT, DETAIL_MAX_HEIGHT);
    code_box(ui, |ui| {
        ScrollArea::vertical().id_salt(("run_permission", &run.id, &prompt.request_id)).max_height(height).show(ui, |ui| {
            ui.add(egui::Label::new(rich(ui, detail, Type::MONO, Ink::Text)).wrap());
        });
    });
    ui.add_space(DETAIL_GAP);
    ui.horizontal(|ui| {
        if primary_button(ui, "Allow once").clicked() {
            desk.decide(&run.id, &prompt.request_id, Decision::Allow { remember: false });
        }
        if can_remember && button(ui, "Allow for this run").clicked() {
            desk.decide(&run.id, &prompt.request_id, Decision::Allow { remember: true });
        }
        if danger_button(ui, "Deny").clicked() {
            desk.decide(&run.id, &prompt.request_id, Decision::Deny);
        }
    });
}

fn background_note(count: usize) -> String {
    match count {
        1 => "1 background agent is still working. Claude continues when it reports back.".to_owned(),
        _ => format!("{count} background agents are still working. Claude continues when they report back."),
    }
}

/// Under the summary, one plain label per line. Tool output can hold posting text, so it is never Markdown.
fn failure_reason(ui: &mut Ui, reason: &str) {
    let indent = EVENT_WHO_WIDTH + EVENT_GAP;
    Frame::new().inner_margin(Margin { left: indent as i8, ..Default::default() }).show(ui, |ui| {
        for line in reason.lines() {
            ui.add(egui::Label::new(rich(ui, line, Type::BODY, Ink::Muted)).truncate()).on_hover_text(reason);
        }
    });
}

fn who(ui: &mut Ui, name: &str) {
    // Left-aligned in a fixed slot, as `.who2` is, so the text after it lines up.
    let slot = vec2(EVENT_WHO_WIDTH, 16.0);
    ui.allocate_ui_with_layout(slot, Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(slot);
        ui.add(egui::Label::new(rich(ui, name, Type::SMALL, Ink::Muted)).truncate());
    });
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        muted(ui, label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(egui::Label::new(rich(ui, value, Type::BODY, Ink::Text)).truncate());
        });
    });
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}
