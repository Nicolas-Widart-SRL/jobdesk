use std::collections::{HashMap, HashSet};

use egui::text::LayoutJob;
use egui::{FontSelection, Frame, Grid, Margin, Popup, PopupCloseBehavior, RectAlign, Response, Role, ScrollArea, TextEdit, Ui, UiBuilder};
use egui_commonmark::CommonMarkCache;

use crate::runs::application::{
    Activity, ActivityKind, ClosingQuestion, Decision, Prompt, PromptKind, Question, QuickReply, Run, RunDesk, RunStatus, ToolState, closing_question, stop_point, wants_reply,
};
use crate::ui_kit::tokens::{
    BUTTON_HEIGHT, CODE_PADDING, EVENT_GAP, EVENT_NESTED, EVENT_PADDING, EVENT_WHO_WIDTH, ITEM_GAP, OFFSET_SHADOW, PROMPT_PADDING, ROW_GAP, SCREEN_GAP, SPLIT_SIDE_BASIS,
};
use crate::ui_kit::{
    Ink, Subtitle, Title, Tone, Type, button, button_width, code_box, danger_button, divider, list_row, markdown, muted, page_body, page_head, panel, panel_body, panel_header, pick_row, primary_button,
    prompt_box_tone, rich, slip, slip_bar, small_button, small_danger_button, solid_tag, split, step_rail, strong, tag,
};

const ACTIVITY_SHOWN: usize = 400;
/// What the activity keeps under the turn bar, its heading included, however much the bar asks for.
const ACTIVITY_MIN_HEIGHT: f32 = 160.0;
/// Claude's closing question is cut after this many lines, with the full text on hover.
const QUESTION_ROWS: usize = 3;
const END_POPOVER_WIDTH: f32 = 320.0;
const END_SESSION: &str = "End session…";
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
    markdown: CommonMarkCache,
}

/// A click in the head, reported out of `page_head`'s actions.
#[derive(Clone, Copy)]
enum HeadAction {
    Stop,
    ForceStop,
    AskToEnd,
}

/// Why replies wait while another run works.
const BLOCKED: &str = "Another run is working. You can reply when it finishes.";

/// Whether the turn bar shows: Claude waits on the user, and no stop is under way.
fn your_turn(run: &Run) -> bool {
    run.status != RunStatus::Stopping && wants_reply(run)
}

/// Whose turn it is, in words and tone.
fn status(run: &Run) -> (&str, Tone) {
    if run.status == RunStatus::Stopping {
        return (run.status.label(), Tone::Active);
    }
    if wants_reply(run) {
        return ("Your turn", Tone::Turn);
    }
    match run.status {
        RunStatus::Starting => ("Starting", Tone::Active),
        _ if run.background_agents > 0 => ("Claude's turn", Tone::Active),
        RunStatus::Working | RunStatus::Stopping => ("Claude's turn", Tone::Active),
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

/// Whether the field sits under the activity: the session can take a reply, and the turn bar does not ask for one.
/// That is a run that closed, failed or was stopped, or one that finished and asks nothing.
fn footer_reply(run: &Run) -> bool {
    let ended = matches!(run.status, RunStatus::Closed | RunStatus::Failed(_) | RunStatus::Stopped);
    run.session_id.is_some() && !run.is_busy() && !run.needs_user() && (ended || (run.awaits_reply() && !wants_reply(run)))
}

/// Whether anything sits under the activity: word of background agents, or the reply field.
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

impl RunScreen {
    pub fn select(&mut self, id: String) {
        self.selected = Some(id);
        self.reply.clear();
        self.picks.clear();
        self.picks_for = None;
    }

    pub fn show(&mut self, ui: &mut Ui, desk: &mut RunDesk) {
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
        let (mut action, mut end_button) = (None, None);
        page_head(ui, Title::Typed(first_line(&run.line)), None, |ui| {
            let (clicked, button) = head_actions(ui, &run);
            action = clicked.or(action);
            // The real pass comes last, so its button is the one kept.
            end_button = button;
        });
        match action {
            Some(HeadAction::Stop) => desk.stop(&run.id),
            Some(HeadAction::ForceStop) => desk.close(&run.id),
            Some(HeadAction::AskToEnd) => self.ending = Some((run.id.clone(), true)),
            None => {}
        }
        match end_button {
            Some(button) => self.end_popover(desk, &run, &button),
            None => self.ending = None,
        }
        page_body(ui, |ui| self.body(ui, desk, &run));
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

    fn body(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        if let Some(steps) = &run.steps {
            let complete = steps.is_complete();
            let live = !complete && (run.is_busy() || run.needs_user() || run.status == RunStatus::AwaitingReply);
            step_rail(ui, steps.labels, steps.current, live, complete);
            ui.add_space(SCREEN_GAP);
        }
        if your_turn(run) {
            self.turn_bar(ui, desk, run);
            ui.add_space(ROW_GAP);
        }
        split(ui, SPLIT_SIDE_BASIS, |main, side| {
            self.facts(side, desk, run);
            self.detail(main, desk, run);
        });
    }

    /// The activity, then the footer under it, outside the activity's scroll area.
    /// The activity takes what the footer left on the frame before. Each footer has its own measure:
    /// with none yet, it gets the whole column. A new measure redraws the frame, so the footer is never left below the window.
    fn detail(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        let reserved_id = footer_height_id(run);
        let input = has_footer(run);
        let reserved = if input { ui.data(|data| data.get_temp::<f32>(reserved_id)).unwrap_or(ui.available_height()) } else { 0.0 };
        panel(ui, |ui| {
            panel_header(ui, "Activity", run.is_busy().then_some("live"));
            let room = (ui.available_height() - reserved).max(0.0);
            ScrollArea::vertical().id_salt(("run_activity", &run.id)).max_height(room).stick_to_bottom(true).auto_shrink([false, true]).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let skipped = run.activity.len().saturating_sub(ACTIVITY_SHOWN);
                if skipped > 0 {
                    panel_body(ui, |ui| muted(ui, format!("{skipped} earlier items are in the run log.")));
                }
                for (index, item) in run.activity[skipped..].iter().enumerate() {
                    if index > 0 || skipped > 0 {
                        divider(ui);
                    }
                    self.activity_row(ui, item);
                }
                if !run.streaming_text.is_empty() {
                    divider(ui);
                    panel_body(ui, |ui| ui.label(run.streaming_text.as_str()));
                }
            });
        });
        if !input {
            return;
        }
        // The cursor moves past a gap after each item, so this counts the gap below the footer.
        // The gap above it, between the activity and the cursor, is added.
        let start = ui.cursor().top();
        self.footer(ui, desk, run);
        let used = ui.cursor().top() - start + ui.spacing().item_spacing.y;
        if (used - reserved).abs() > 0.5 {
            ui.data_mut(|data| data.insert_temp(reserved_id, used));
            // A discard redraws within this frame when a pass is left. The repaint covers the
            // frames where none is, so an idle window still settles on the new measure.
            ui.ctx().request_discard("the footer under the activity changed height");
            ui.ctx().request_repaint();
        }
    }

    fn activity_row(&mut self, ui: &mut Ui, item: &Activity) {
        // The activity is a main-column section, so its rows sit flush with the column, as `.ev` does.
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        tag(ui, text, tone);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(rich(ui, first_line(summary), Type::MONO, Ink::Text)).truncate()).on_hover_text(summary);
                        });
                    });
                });
                if let ToolState::Failed(Some(reason)) = state {
                    failure_reason(ui, reason);
                }
            }
        });
    }

    /// Under the activity, outside its scroll area: word of background agents, or the field that resumes the session.
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

    /// Above the activity while it is the user's turn, framed in the turn colour:
    /// the open prompt, or Claude's closing question and the reply row.
    fn turn_bar(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        let question = closing_question(run);
        // What a prompt's content may take: the room above the activity's least height and any footer under it,
        // less the bar's frame and the gaps under it. A footer not yet measured counts as nothing here.
        let (_, padding) = PROMPT_PADDING;
        let footer = if has_footer(run) { ui.data(|data| data.get_temp::<f32>(footer_height_id(run))).unwrap_or(0.0) } else { 0.0 };
        let gaps = ui.spacing().item_spacing.y + ROW_GAP;
        let budget = ui.available_height() - ACTIVITY_MIN_HEIGHT - footer - gaps - padding as f32 * 2.0 - OFFSET_SHADOW as f32;
        let group = UiBuilder::new().accessibility_role(Role::Group).accessibility_label(turn_label(run, question.as_ref()));
        ui.scope_builder(group, |ui| {
            prompt_box_tone(ui, Tone::Turn, |ui| match run.current_prompt().cloned() {
                Some(prompt) => match &prompt.kind {
                    PromptKind::Permission { tool, detail, can_remember } => permission(ui, desk, run, &prompt, tool, detail, *can_remember, budget),
                    PromptKind::Questions(questions) => self.questions(ui, desk, run, &prompt, questions, budget),
                },
                None => self.stop_reply(ui, desk, run, question.as_ref()),
            });
        });
    }

    /// Claude's question, cut to a few lines, then one row: the stop point's quick replies, the field and Send.
    fn stop_reply(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, question: Option<&ClosingQuestion>) {
        match question {
            Some(question) => {
                // Plain text: Claude's message is untrusted, and the question is read, not rendered.
                let style = ui.style().clone();
                let mut job = LayoutJob::default();
                for (text, ink) in [("Claude asks: ", Ink::Muted), (question.text.as_str(), Ink::Text)] {
                    rich(ui, text, Type::BODY, ink).append_to(&mut job, &style, FontSelection::Default, egui::Align::Min);
                }
                job.wrap.max_rows = QUESTION_ROWS;
                ui.add(egui::Label::new(job).wrap()).on_hover_text(&question.text);
            }
            None => {
                muted(ui, "Claude finished this step. Read its message, then reply.");
            }
        }
        let replies = stop_point(run).map_or(&[][..], |stop| stop.replies);
        let hint = if replies.is_empty() { "Reply to Claude" } else { "Or write your own reply" };
        self.reply_row(ui, desk, run, replies, hint, true);
    }

    /// Quick replies, the field, then Send on its right. A quick reply sends its words as a typed reply would.
    /// When the quick replies would leave the field under `REPLY_FIELD_MIN_WIDTH`, they take a row of their own above it.
    /// With `focus`, the field takes focus once per run and turn: on the turn's first frame, and only when nothing else has focus.
    fn reply_row(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, replies: &[QuickReply], hint: &str, focus: bool) {
        let blocked = desk.is_busy();
        if blocked {
            muted(ui, BLOCKED);
        }
        let gap = ui.spacing().item_spacing.x;
        let send_width = button_width(ui, "Send");
        let quick_width: f32 = replies.iter().map(|reply| button_width(ui, reply.button) + gap).sum();
        let inline = quick_width + REPLY_FIELD_MIN_WIDTH + gap + send_width <= ui.available_width();
        let mut sent = None;
        let quick_replies = |ui: &mut Ui, sent: &mut Option<String>| {
            for reply in replies {
                let response = ui.add_enabled_ui(!blocked, |ui| button(ui, reply.button)).inner.on_hover_text(reply.text).on_disabled_hover_text(BLOCKED);
                if response.clicked() {
                    *sent = Some(reply.text.to_owned());
                }
            }
        };
        if !inline && !replies.is_empty() {
            ui.horizontal(|ui| quick_replies(ui, &mut sent));
        }
        ui.horizontal(|ui| {
            if inline {
                quick_replies(ui, &mut sent);
            }
            let room = ui.available_width() - send_width - gap;
            let edit = ui.add(TextEdit::singleline(&mut self.reply).hint_text(hint).desired_width(room));
            let turn = (run.id.clone(), run.current_turn_start());
            if focus && self.focused_for.as_ref() != Some(&turn) {
                // Offered once: when another widget has focus, this turn passes without taking it.
                if ui.memory(|memory| memory.focused().is_none()) {
                    edit.request_focus();
                }
                self.focused_for = Some(turn);
            }
            let typed = !self.reply.trim().is_empty();
            let send = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.add_enabled_ui(typed && !blocked, |ui| primary_button(ui, "Send")).inner.clicked()).inner;
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
        // The choices scroll in what the budget leaves above the gap and the buttons.
        let room = budget - ui.spacing().item_spacing.y - BUTTON_HEIGHT;
        // Never past the budget, even when it is under the least height.
        let height = room.clamp(CHOICES_MIN_HEIGHT, CHOICES_MAX_HEIGHT).min(budget.max(0.0));
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

    fn facts(&mut self, ui: &mut Ui, desk: &RunDesk, run: &Run) {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            slip(ui, |ui| {
                panel_header(ui, "This run", None);
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = ROW_GAP;
                    fact(ui, "Cost", &format!("${:.2} of ${:.2}", run.cost_usd, run.budget_usd));
                    slip_bar(ui, (run.cost_usd / run.budget_usd.max(0.01)) as f32, Ink::Text);
                    fact(ui, "Elapsed", &elapsed(run.duration_ms));
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
                    }
                }
            });
        });
    }

    /// One run in the recent list. The picked run reads in full ink, the others muted.
    fn recent_row(&self, ui: &mut Ui, run: &Run, on: bool) -> bool {
        let response = pick_row(ui, on, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(rich(ui, format!("${:.2}", run.cost_usd), Type::BODY, Ink::Faint));
                    status_tag(ui, run);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let ink = if on { Ink::Text } else { Ink::Muted };
                        ui.add(egui::Label::new(rich(ui, first_line(&run.line), Type::MONO, ink)).truncate());
                    });
                });
            });
        });
        response.clicked()
    }
}

/// Stop, force or end, by the run's state. Added right to left, so the button comes before the tag.
/// `page_head` runs this twice a frame, so it only reports a click, and End session's button to anchor the popover on.
fn head_actions(ui: &mut Ui, run: &Run) -> (Option<HeadAction>, Option<Response>) {
    let mut action = None;
    let mut end_button = None;
    if run.status == RunStatus::Stopping {
        if small_danger_button(ui, "Force stop").clicked() {
            action = Some(HeadAction::ForceStop);
        }
    } else if run.is_busy() || run.needs_user() {
        if small_danger_button(ui, "Stop run").clicked() {
            action = Some(HeadAction::Stop);
        }
    } else if run.awaits_reply() {
        let response = small_button(ui, END_SESSION);
        if response.clicked() {
            action = Some(HeadAction::AskToEnd);
        }
        end_button = Some(response);
    }
    status_tag(ui, run);
    (action, end_button)
}

/// `budget` is the height the prompt's content may take, buttons included.
#[allow(clippy::too_many_arguments)]
fn permission(ui: &mut Ui, desk: &mut RunDesk, run: &Run, prompt: &Prompt, tool: &str, detail: &str, can_remember: bool, budget: f32) {
    let top = ui.cursor().top();
    strong(ui, format!("Claude wants to use {tool}"));
    muted(ui, "Not on the allow list in .claude/settings.json.");
    // The command scrolls in what the budget leaves after the lines above it, the code box's padding,
    // the one gap after the box, the extra space and the buttons.
    let above = ui.cursor().top() - top;
    let room = budget - above - CODE_PADDING.1 as f32 * 2.0 - ui.spacing().item_spacing.y - DETAIL_GAP - BUTTON_HEIGHT;
    let height = room.clamp(DETAIL_MIN_HEIGHT, DETAIL_MAX_HEIGHT);
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
    let slot = egui::vec2(EVENT_WHO_WIDTH, 16.0);
    ui.allocate_ui_with_layout(slot, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(slot);
        ui.add(egui::Label::new(rich(ui, name, Type::SMALL, Ink::Muted)).truncate());
    });
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        muted(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(rich(ui, value, Type::BODY, Ink::Text)).truncate());
        });
    });
}

fn elapsed(duration_ms: u64) -> String {
    let seconds = duration_ms / 1000;
    match seconds / 60 {
        0 => format!("{seconds}s"),
        minutes => format!("{minutes}m {}s", seconds % 60),
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}
