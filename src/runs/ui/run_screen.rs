use std::collections::{HashMap, HashSet};

use egui::{Frame, Margin, ScrollArea, TextEdit, Ui};
use egui_commonmark::CommonMarkCache;

use crate::runs::application::{Activity, ActivityKind, Decision, Prompt, PromptKind, Question, Run, RunDesk, RunStatus, ToolState};
use crate::ui_kit::tokens::{
    BUTTON_HEIGHT, CODE_PADDING, EVENT_GAP, EVENT_NESTED, EVENT_PADDING, EVENT_WHO_WIDTH, OFFSET_SHADOW, PROMPT_MARGIN, PROMPT_PADDING, ROW_GAP, SCREEN_GAP, SPLIT_SIDE_BASIS,
};
use crate::ui_kit::{
    Ink, Subtitle, Title, Tone, Type, button, button_width, code_box, danger_button, divider, list_row, markdown, muted, page_body, page_head, panel, panel_body, panel_header, pick_row, primary_button, prompt_box, rich,
    slip, slip_bar, split, step_rail, strong, tag,
};

const ACTIVITY_SHOWN: usize = 400;
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
    markdown: CommonMarkCache,
}

fn status_tone(run: &Run) -> Tone {
    if run.needs_user() {
        return Tone::Interview;
    }
    if run.background_agents > 0 {
        return Tone::Active;
    }
    match run.status {
        RunStatus::Starting | RunStatus::Working | RunStatus::Stopping => Tone::Active,
        RunStatus::AwaitingReply => Tone::Interview,
        RunStatus::Failed(_) => Tone::Closed,
        RunStatus::Stopped | RunStatus::Closed => Tone::Plain,
    }
}

/// Whether the run waits on a prompt, or its session can take a reply.
fn takes_input(run: &Run) -> bool {
    run.needs_user() || (run.session_id.is_some() && !run.is_busy())
}

/// Whether anything sits under the activity: a prompt, the reply box, or word of background agents.
fn has_footer(run: &Run) -> bool {
    takes_input(run) || run.background_agents > 0
}

fn status_label(run: &Run) -> &str {
    match run.current_prompt().map(|prompt| &prompt.kind) {
        Some(PromptKind::Permission { .. }) => "Waiting for permission",
        Some(PromptKind::Questions(_)) => "Waiting for your answer",
        None if run.background_agents > 0 => "Agents working",
        None => run.status.label(),
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
        page_head(ui, Title::Typed(first_line(&run.line)), None, |ui| head_actions(ui, desk, &run));
        page_body(ui, |ui| self.body(ui, desk, &run));
    }

    fn body(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        if let Some(steps) = &run.steps {
            step_rail(ui, steps.labels, steps.current, run.is_busy() || run.needs_user());
            ui.add_space(SCREEN_GAP);
        }
        split(ui, SPLIT_SIDE_BASIS, |main, side| {
            self.facts(side, desk, run);
            self.detail(main, desk, run);
        });
    }

    /// The activity, then the prompt under it, outside the activity's scroll area.
    /// The activity takes what the prompt left on the frame before. Each prompt, and the reply box,
    /// has its own measure: with none yet, it gets the whole column. Its own scroll areas are sized
    /// from the column, so it fits there and measures the same on the next frame.
    /// A new measure redraws the frame, so a prompt is never left below the window.
    fn detail(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        let gap = ui.spacing().item_spacing.y;
        let shown = run.current_prompt().map(|prompt| prompt.request_id.as_str());
        let reserved_id = ui.scope_id().with(("prompt_height", &run.id, shown));
        let input = has_footer(run);
        let reserved = if input { ui.data(|data| data.get_temp::<f32>(reserved_id)).unwrap_or(ui.available_height()) } else { 0.0 };
        let mut budget = 0.0;
        panel(ui, |ui| {
            panel_header(ui, "Activity", run.is_busy().then_some("live"));
            // What a prompt's content may take: the column under the heading, less the prompt's frame and gaps.
            let (_, padding) = PROMPT_PADDING;
            budget = ui.available_height() - gap * 2.0 - PROMPT_MARGIN.0 - padding as f32 * 2.0 - OFFSET_SHADOW as f32;
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
        // The cursor moves past a gap after each item, so this counts the gap below the prompt.
        // The gap above it, between the activity and the cursor, is added.
        let start = ui.cursor().top();
        self.prompt_or_reply(ui, desk, run, budget);
        let used = ui.cursor().top() - start + ui.spacing().item_spacing.y;
        if (used - reserved).abs() > 0.5 {
            ui.data_mut(|data| data.insert_temp(reserved_id, used));
            // A discard redraws within this frame when a pass is left. The repaint covers the
            // frames where none is, so an idle window still settles on the new measure.
            ui.ctx().request_discard("the prompt under the activity changed height");
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

    /// Shown under the activity, outside its scroll area.
    /// `budget` is the height the prompt's content may take, buttons included.
    fn prompt_or_reply(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, budget: f32) {
        if let Some(prompt) = run.current_prompt().cloned() {
            ui.add_space(PROMPT_MARGIN.0);
            self.prompt(ui, desk, run, &prompt, budget);
        } else if run.background_agents > 0 {
            divider(ui);
            Frame::new().inner_margin(Margin { top: ROW_GAP as i8, ..Default::default() }).show(ui, |ui| muted(ui, background_note(run.background_agents)));
        } else if takes_input(run) {
            divider(ui);
            Frame::new().inner_margin(Margin { top: ROW_GAP as i8, ..Default::default() }).show(ui, |ui| {
                if run.status == RunStatus::AwaitingReply {
                    muted(ui, "Claude finished this step. Reply to continue, or press End session when you are done.");
                }
                self.reply_box(ui, desk, run);
            });
        }
    }

    /// The field, then Send on its right. The field comes first, so it is first in focus order.
    fn reply_box(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        let blocked = desk.is_busy();
        ui.horizontal(|ui| {
            let hint = if run.status == RunStatus::AwaitingReply { "Reply to Claude" } else { "Reply to resume this session" };
            let room = ui.available_width() - button_width(ui, "Send") - ui.spacing().item_spacing.x;
            let edit = ui.add(TextEdit::singleline(&mut self.reply).hint_text(hint).desired_width(room));
            let send = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.add_enabled_ui(!self.reply.trim().is_empty() && !blocked, |ui| primary_button(ui, "Send")).inner.clicked())
                .inner;
            let entered = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (send || entered) && !self.reply.trim().is_empty() && !blocked {
                let text = std::mem::take(&mut self.reply);
                let _ = desk.reply(&run.id, text.trim());
            }
        });
    }

    fn prompt(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, prompt: &Prompt, budget: f32) {
        prompt_box(ui, |ui| {
            match &prompt.kind {
                PromptKind::Permission { tool, detail, can_remember } => permission(ui, desk, run, prompt, tool, detail, *can_remember, budget),
                PromptKind::Questions(questions) => self.questions(ui, desk, run, prompt, questions, budget),
            }
        });
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
        let height = room.clamp(CHOICES_MIN_HEIGHT, CHOICES_MAX_HEIGHT);
        ScrollArea::vertical().id_salt(("run_questions", &run.id, &prompt.request_id)).max_height(height).show(ui, |ui| {
            for question in questions {
                strong(ui, &question.text);
                let picked = self.picks.entry((prompt.request_id.clone(), question.text.clone())).or_default();
                for option in &question.options {
                    let mut on = picked.contains(&option.label);
                    let changed = if question.multi_select {
                        ui.checkbox(&mut on, &option.label).changed()
                    } else {
                        let clicked = ui.radio(on, &option.label).clicked();
                        on |= clicked;
                        clicked
                    };
                    if !option.description.is_empty() {
                        muted(ui, &option.description);
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
                }
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
                    tag(ui, status_label(run), status_tone(run));
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
fn head_actions(ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
    if run.status == RunStatus::Stopping {
        if button(ui, "Force stop").clicked() {
            desk.close(&run.id);
        }
    } else if run.is_busy() || run.needs_user() {
        if danger_button(ui, "Stop run").clicked() {
            desk.stop(&run.id);
        }
    } else if run.status == RunStatus::AwaitingReply && button(ui, "End session").clicked() {
        desk.close(&run.id);
    }
    tag(ui, status_label(run), status_tone(run));
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
