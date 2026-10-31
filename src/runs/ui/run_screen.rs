use std::collections::{HashMap, HashSet};

use egui::{Frame, Margin, Panel, RichText, ScrollArea, Stroke, TextEdit, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::runs::application::{Activity, ActivityKind, Decision, Prompt, PromptKind, Question, Run, RunDesk, RunStatus, ToolState};
use crate::ui_kit::{Palette, Tone, bar, divider, faint_mono, muted, panel, panel_body, panel_header, primary_button, semibold, strong, tag};

const ACTIVITY_SHOWN: usize = 400;

#[derive(Default)]
pub struct RunScreen {
    selected: Option<String>,
    reply: String,
    picks: HashMap<String, HashSet<String>>,
    markdown: CommonMarkCache,
}

fn status_tone(run: &Run) -> Tone {
    if run.needs_user() {
        return Tone::Interview;
    }
    match run.status {
        RunStatus::Starting | RunStatus::Working | RunStatus::Stopping => Tone::Active,
        RunStatus::AwaitingReply => Tone::Interview,
        RunStatus::Failed(_) => Tone::Closed,
        RunStatus::Stopped | RunStatus::Closed => Tone::Plain,
    }
}

fn status_label(run: &Run) -> &str {
    match run.current_prompt().map(|prompt| &prompt.kind) {
        Some(PromptKind::Permission { .. }) => "Waiting for permission",
        Some(PromptKind::Questions(_)) => "Waiting for your answer",
        None => run.status.label(),
    }
}

impl RunScreen {
    pub fn select(&mut self, id: String) {
        self.selected = Some(id);
        self.reply.clear();
    }

    pub fn show(&mut self, ui: &mut Ui, desk: &mut RunDesk) {
        if desk.runs().is_empty() {
            panel(ui, |ui| {
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    strong(ui, "No runs yet");
                    muted(ui, "Press Run command to start one.");
                });
            });
            return;
        }
        let known = self.selected.as_ref().is_some_and(|id| desk.run(id).is_some());
        if !known {
            self.selected = desk.runs().first().map(|run| run.id.clone());
        }
        Panel::left("run_list").exact_size(240.0).resizable(false).frame(Frame::new().inner_margin(Margin { right: 12, ..Default::default() })).show(ui, |ui| self.list(ui, desk));
        let Some(id) = self.selected.clone() else { return };
        let Some(run) = desk.run(&id).cloned() else { return };
        Panel::right("run_facts").exact_size(280.0).resizable(false).frame(Frame::new().inner_margin(Margin { left: 12, ..Default::default() })).show(ui, |ui| facts(ui, &run));
        egui::CentralPanel::default().frame(Frame::new().inner_margin(Margin { right: 1, bottom: 1, ..Default::default() })).show(ui, |ui| self.detail(ui, desk, &run));
    }

    fn list(&mut self, ui: &mut Ui, desk: &RunDesk) {
        let p = Palette::of(ui);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for run in desk.runs() {
                let on = self.selected.as_ref() == Some(&run.id);
                let response = Frame::new()
                    .fill(if on { p.raised } else { p.bg })
                    .corner_radius(6)
                    .inner_margin(Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add(egui::Label::new(RichText::new(first_line(&run.line)).monospace()).truncate());
                        ui.horizontal(|ui| {
                            tag(ui, status_label(run), status_tone(run));
                        });
                        ui.label(RichText::new(&run.started_at).small().color(p.faint));
                    })
                    .response
                    .interact(egui::Sense::click());
                if response.clicked() {
                    self.select(run.id.clone());
                }
            }
        });
    }

    fn detail(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 28.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if run.status == RunStatus::Stopping {
                if ui.button("Force stop").clicked() {
                    desk.close(&run.id);
                }
            } else if run.is_busy() || run.needs_user() {
                if ui.button(RichText::new("Stop run").color(Palette::of(ui).closed)).clicked() {
                    desk.stop(&run.id);
                }
            } else if run.status == RunStatus::AwaitingReply && ui.button("End session").clicked() {
                desk.close(&run.id);
            }
            tag(ui, status_label(run), status_tone(run));
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(first_line(&run.line)).monospace().size(14.0)).truncate());
            });
        });
        if let Some(steps) = &run.steps {
            ui.add_space(4.0);
            step_rail(ui, steps.labels, steps.current, run.is_busy() || run.needs_user());
        }
        ui.add_space(8.0);

        self.prompt_or_reply(ui, desk, run);
        panel(ui, |ui| {
            ui.set_min_height(ui.available_height() - 1.0);
            panel_header(ui, "Activity", None);
            ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let skipped = run.activity.len().saturating_sub(ACTIVITY_SHOWN);
                if skipped > 0 {
                    panel_body(ui, |ui| muted(ui, format!("{skipped} earlier items are in the run log.")));
                }
                for item in &run.activity[skipped..] {
                    self.activity_row(ui, item);
                }
                if !run.streaming_text.is_empty() {
                    panel_body(ui, |ui| ui.label(run.streaming_text.as_str()));
                }
            });
        });
    }

    fn activity_row(&mut self, ui: &mut Ui, item: &Activity) {
        let indent = if item.nested { 36 } else { 14 };
        Frame::new().inner_margin(Margin { left: indent, right: 14, top: 5, bottom: 5 }).show(ui, |ui| match &item.kind {
            ActivityKind::You(text) => {
                ui.horizontal_wrapped(|ui| {
                    strong(ui, "You");
                    ui.label(RichText::new(text).monospace());
                });
            }
            ActivityKind::Claude(text) => {
                CommonMarkViewer::new().show(ui, &mut self.markdown, text);
            }
            ActivityKind::Notice(text) => {
                muted(ui, text);
            }
            ActivityKind::Tool { name, summary, state, .. } => {
                ui.horizontal(|ui| {
                    ui.add_sized([56.0, 16.0], egui::Label::new(RichText::new(name).small().color(Palette::of(ui).faint)).truncate());
                    let (text, tone) = match state {
                        ToolState::Running => ("Running", Tone::Active),
                        ToolState::Done => ("Done", Tone::Hired),
                        ToolState::Failed => ("Failed", Tone::Closed),
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        tag(ui, text, tone);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(first_line(summary)).monospace()).truncate()).on_hover_text(summary);
                        });
                    });
                });
            }
        });
    }

    /// Shown under the activity, at the bottom of the screen.
    fn prompt_or_reply(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        let prompt = run.current_prompt().cloned();
        let can_reply = run.session_id.is_some() && !run.is_busy() && prompt.is_none();
        Panel::bottom("run_input").frame(Frame::new().inner_margin(Margin { top: 8, ..Default::default() })).show(ui, |ui| {
            if let Some(prompt) = prompt {
                self.prompt(ui, desk, run, &prompt);
            } else if can_reply {
                self.reply_box(ui, desk, run);
            }
        });
    }

    fn reply_box(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run) {
        let blocked = desk.is_busy();
        ui.horizontal(|ui| {
            let send = ui.add_enabled_ui(!self.reply.trim().is_empty() && !blocked, |ui| primary_button(ui, "Send")).inner.clicked();
            let hint = if run.status == RunStatus::AwaitingReply { "Reply to Claude" } else { "Reply to resume this session" };
            let edit = ui.add(TextEdit::singleline(&mut self.reply).hint_text(hint).desired_width(ui.available_width() - 8.0));
            let entered = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (send || entered) && !self.reply.trim().is_empty() && !blocked {
                let text = std::mem::take(&mut self.reply);
                let _ = desk.reply(&run.id, text.trim());
            }
        });
    }

    fn prompt(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, prompt: &Prompt) {
        let p = Palette::of(ui);
        Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.interview)).corner_radius(6).inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match &prompt.kind {
                PromptKind::Permission { tool, detail, can_remember } => permission(ui, desk, run, prompt, tool, detail, *can_remember),
                PromptKind::Questions(questions) => self.questions(ui, desk, run, prompt, questions),
            }
        });
    }

    fn questions(&mut self, ui: &mut Ui, desk: &mut RunDesk, run: &Run, prompt: &Prompt, questions: &[Question]) {
        ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for question in questions {
                ui.label(RichText::new(&question.text).family(semibold()));
                let picked = self.picks.entry(question.text.clone()).or_default();
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
                let picked = self.picks.get(&question.text)?;
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
            if ui.button("Decline").clicked() {
                desk.decide(&run.id, &prompt.request_id, Decision::Deny);
                self.picks.clear();
            }
        });
    }
}

fn permission(ui: &mut Ui, desk: &mut RunDesk, run: &Run, prompt: &Prompt, tool: &str, detail: &str, can_remember: bool) {
    let p = Palette::of(ui);
    ui.label(RichText::new(format!("Claude wants to use {tool}")).family(semibold()));
    muted(ui, "Not on the allow list in .claude/settings.json.");
    Frame::new().fill(p.bg).stroke(Stroke::new(1.0, p.line)).corner_radius(4).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(detail).monospace()).wrap());
        });
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if primary_button(ui, "Allow once").clicked() {
            desk.decide(&run.id, &prompt.request_id, Decision::Allow { remember: false });
        }
        if can_remember && ui.button("Allow for this run").clicked() {
            desk.decide(&run.id, &prompt.request_id, Decision::Allow { remember: true });
        }
        if ui.button(RichText::new("Deny").color(p.closed)).clicked() {
            desk.decide(&run.id, &prompt.request_id, Decision::Deny);
        }
    });
}

fn step_rail(ui: &mut Ui, labels: &[&str], current: usize, live: bool) {
    let p = Palette::of(ui);
    ui.horizontal_wrapped(|ui| {
        for (index, label) in labels.iter().enumerate() {
            let now = index == current && live;
            let done = index < current;
            let color = if now { p.on_primary } else if done { p.text } else { p.faint };
            Frame::new().fill(if now { p.primary } else { p.bg }).stroke(Stroke::new(1.0, if now { p.primary } else { p.line })).corner_radius(6).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
                ui.label(RichText::new(format!("{index} {label}")).small().color(color));
            });
        }
    });
}

fn facts(ui: &mut Ui, run: &Run) {
    let p = Palette::of(ui);
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        panel(ui, |ui| {
            panel_header(ui, "This run", None);
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                fact(ui, "Cost", &format!("${:.2} of ${:.2}", run.cost_usd, run.budget_usd));
                bar(ui, (run.cost_usd / run.budget_usd.max(0.01)) as f32, p.text);
                ui.add_space(4.0);
                fact(ui, "Duration", &format!("{}s", run.duration_ms / 1000));
                if let Some(model) = &run.model {
                    fact(ui, "Model", model);
                }
                fact(ui, "Started", &run.started_at);
            });
        });
        for (title, items) in [("Agents", &run.agents), ("Files changed", &run.files_changed)] {
            if items.is_empty() {
                continue;
            }
            ui.add_space(10.0);
            panel(ui, |ui| {
                panel_header(ui, title, None);
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        divider(ui);
                    }
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(egui::Label::new(RichText::new(item).monospace()).truncate()).on_hover_text(item);
                    });
                }
            });
        }
        ui.add_space(10.0);
        panel(ui, |ui| {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some(usage) = run.usage.five_hour {
                    fact(ui, "5-hour usage", &format!("{:.0}%", usage * 100.0));
                }
                if let Some(usage) = run.usage.seven_day {
                    fact(ui, "7-day usage", &format!("{:.0}%", usage * 100.0));
                }
                if let Some(session) = &run.session_id {
                    faint_mono(ui, session.chars().take(8).collect::<String>());
                }
            });
        });
    });
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        muted(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.monospace(value));
    });
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}
