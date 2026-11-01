use std::collections::HashSet;

use egui::{Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::discovery::application::{CheckRow, CheckState, FitLevel, JobBoard, JobChecks, JobView, Lane, Signal, Verdict, fit_band};
use crate::ui_kit::tokens::{ITEM_GAP, ROW_GAP, ROW_ITEM_GAP, SCREEN_GAP, TABLE_HEAD_HEIGHT};
use crate::ui_kit::{
    CellRule, CheckMark, Ink, Subtitle, Title, Tone, Type, check_tag, command_action, divider, muted, open_link, page_body, page_head, panel, panel_body, paper, primary_command_action, rich,
    score, segmented, side_column, stamp, stamp_square, strong, table_cell, table_head, tag, title_over,
};

/// What the user is looking at on this screen.
pub struct JobsScreen {
    lane: Lane,
    selected: Option<String>,
}

impl Default for JobsScreen {
    fn default() -> Self {
        Self { lane: Lane::Shortlist, selected: None }
    }
}

/// The segments in the order a person works through them.
const LANES: [Lane; 7] = [Lane::Shortlist, Lane::New, Lane::BelowThreshold, Lane::Excluded, Lane::CouldNotFetch, Lane::Skipped, Lane::Expired];
const WAITING: &str = "Waiting in Today";
const NOT_STATED: &str = "Not stated";

fn lane_label(lane: Lane) -> &'static str {
    match lane {
        Lane::Shortlist => "Shortlist",
        Lane::BelowThreshold => "Low score",
        Lane::Excluded => "Ruled out",
        Lane::CouldNotFetch => "Could not open",
        Lane::New => "New",
        Lane::Skipped => "Skipped",
        Lane::Expired => "Expired",
    }
}

/// Expired shows only once a job has expired, so an empty segment does not crowd the row.
fn lane_shown(lane: Lane, board: &JobBoard) -> bool {
    lane != Lane::Expired || board.count(lane) > 0
}

fn signal_tag(signal: &Signal) -> (String, Tone) {
    match signal {
        Signal::ClosingIn { days } => (format!("Closes in {}", days_words(*days)), Tone::Interview),
        Signal::PostedDaysAgo { days } => (format!("Posted {days} days ago"), Tone::Interview),
        Signal::LocationFlag => ("Location unclear".into(), Tone::Interview),
        Signal::LanguageFlag => ("Language unclear".into(), Tone::Interview),
        Signal::LocationFail => ("Location does not suit".into(), Tone::Closed),
        Signal::LanguageFail => ("Language does not suit".into(), Tone::Closed),
        Signal::GuardFlag => ("Check this ad".into(), Tone::Interview),
        Signal::ContractFail => ("Not freelance".into(), Tone::Closed),
        Signal::RateBelowFloor => ("Rate under your floor".into(), Tone::Closed),
    }
}

fn days_words(days: i64) -> String {
    match days {
        0 => "under a day".to_owned(),
        1 => "1 day".to_owned(),
        days => format!("{days} days"),
    }
}

/// A check the ad never went through shows no tag.
fn check_mark(state: CheckState) -> Option<CheckMark> {
    match state {
        CheckState::Pass => Some(CheckMark::Ok),
        CheckState::Unclear => Some(CheckMark::Unclear),
        CheckState::Fail => Some(CheckMark::Fails),
        CheckState::Unchecked => None,
    }
}

fn fit_words(fit: FitLevel) -> &'static str {
    match fit {
        FitLevel::High => "high",
        FitLevel::Medium => "medium",
        FitLevel::Low => "low",
    }
}

fn verdict_tone(verdict: Verdict) -> Tone {
    match verdict {
        Verdict::Strong => Tone::Hired,
        Verdict::Good => Tone::Active,
        Verdict::Moderate => Tone::Interview,
        Verdict::Weak | Verdict::Poor => Tone::Closed,
    }
}

/// The stated rate without the floor it is read against. The slip shows the floor.
fn day_rate(job: &JobView) -> &str {
    let value = job.freelance.day_rate.value.as_str();
    value.split_once(", ").map_or(value, |(rate, _)| rate)
}

fn score_jobs_label(new: usize) -> String {
    match new {
        0 => "Score new jobs…".to_owned(),
        1 => "Score 1 new job…".to_owned(),
        new => format!("Score {new} new jobs…"),
    }
}

impl JobsScreen {
    /// `waiting` holds the key of every job with a card in Today.
    pub fn show(&mut self, ui: &mut Ui, board: &JobBoard, waiting: &HashSet<String>) {
        let subtitle = Subtitle::Text("Scored from the job ad alone. Drafting an application also checks the company.");
        let new = board.count(Lane::New);
        page_head(ui, Title::Slab("Jobs"), Some(subtitle), |ui| {
            primary_command_action(ui, &score_jobs_label(new), "/rank");
            command_action(ui, "Find new jobs…", "/scrape");
        });
        page_body(ui, |ui| self.body(ui, board, waiting));
    }

    fn body(&mut self, ui: &mut Ui, board: &JobBoard, waiting: &HashSet<String>) {
        if !lane_shown(self.lane, board) {
            self.lane = Lane::Shortlist;
        }
        let options: Vec<_> = LANES.into_iter().filter(|lane| lane_shown(*lane, board)).map(|lane| (lane, format!("{}  {}", lane_label(lane), board.count(lane)))).collect();
        if let Some(lane) = segmented(ui, self.lane, &options) {
            self.lane = lane;
            self.selected = None;
        }
        ui.add_space(SCREEN_GAP);

        let jobs = board.jobs_in(self.lane);
        if jobs.is_empty() {
            panel(ui, |ui| {
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    empty_state(ui, self.lane, board);
                });
            });
            return;
        }
        if !self.selected.as_ref().is_some_and(|key| jobs.iter().any(|job| &job.key == key)) {
            self.selected = jobs.first().map(|job| job.key.clone());
        }
        let selected = jobs.iter().find(|job| Some(&job.key) == self.selected.as_ref());

        side_column("job_details", 340.0).show(ui, |ui| {
            if let Some(job) = selected {
                paper(ui, false, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| details(ui, job));
                });
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| {
            // The fixed columns overflow a narrow window, so the table scrolls sideways there.
            panel(ui, |ui| {
                egui::ScrollArea::horizontal().id_salt("jobs_table_x").auto_shrink([false, true]).show(ui, |ui| self.table(ui, jobs, waiting));
            });
            if jobs.iter().any(|job| waiting.contains(&job.key)) {
                ui.add_space(ROW_GAP);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
                    stamp_square(ui, WAITING);
                    muted(ui, "Waiting in Today. Decide there, or open the drafting task from here.");
                });
            }
        });
    }

    fn table(&mut self, ui: &mut Ui, jobs: &[JobView], waiting: &HashSet<String>) {
        let selected = self.selected.clone();
        let last = jobs.len().saturating_sub(1);
        // Cells pad themselves, so rules and the selected row run unbroken.
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        TableBuilder::new(ui)
            .striped(false)
            .sense(Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(60.0))
            .column(Column::remainder().at_least(160.0).clip(true))
            .column(Column::exact(112.0).clip(true))
            .column(Column::exact(80.0).clip(true))
            .column(Column::exact(96.0))
            .column(Column::exact(168.0).clip(true))
            .column(Column::exact(84.0).clip(true))
            .column(Column::exact(24.0))
            .header(TABLE_HEAD_HEIGHT, |mut header| {
                for title in ["Score", "Job", "Where", "Day rate", "Closes", "Worth knowing", "Found on", ""] {
                    header.col(|ui| {
                        table_cell(ui, CellRule::Head, |ui| table_head(ui, title));
                    });
                }
            })
            .body(|body| {
                body.rows(58.0, jobs.len(), |mut row| {
                    let index = row.index();
                    let job = &jobs[index];
                    let rule = if index == last { CellRule::Last } else { CellRule::Row };
                    row.set_selected(selected.as_ref() == Some(&job.key));
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| score(ui, job.score.map_or_else(|| job.scraper_fit.map_or("-", fit_words).to_owned(), |s| s.to_string())));
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| title_over(ui, &job.title, &job.company));
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| ui.add(egui::Label::new(rich(ui, job.location.as_deref().unwrap_or(NOT_STATED), Type::BODY, Ink::Muted)).wrap()));
                    });
                    row.col(|ui| {
                        let rate = day_rate(job);
                        let ink = if rate == NOT_STATED { Ink::Faint } else { Ink::Text };
                        table_cell(ui, rule, |ui| ui.add(egui::Label::new(rich(ui, rate, Type::BODY, ink)).wrap()));
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| match job.deadline {
                            Some(deadline) => ui.label(deadline.format("%a %-d %b").to_string()),
                            None => ui.label(rich(ui, "None", Type::BODY, Ink::Faint)),
                        });
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| {
                            if let Some(signal) = job.signals.first() {
                                let (text, tone) = signal_tag(signal);
                                tag(ui, text, tone);
                                if job.signals.len() > 1 {
                                    muted(ui, format!("+{}", job.signals.len() - 1));
                                }
                            }
                        });
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| {
                            if let Some(site) = &job.found_on {
                                ui.add(egui::Label::new(site.as_str()).truncate());
                            }
                        });
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| {
                            if waiting.contains(&job.key) {
                                stamp_square(ui, WAITING);
                            }
                        });
                    });
                    if row.response().clicked() {
                        self.selected = Some(job.key.clone());
                    }
                });
            });
    }
}

fn empty_state(ui: &mut Ui, lane: Lane, board: &JobBoard) {
    let new = board.count(Lane::New);
    match lane {
        Lane::Shortlist if new > 0 => {
            strong(ui, if new == 1 { "1 new job waits for a score.".to_owned() } else { format!("{new} new jobs wait for a score.") });
            primary_command_action(ui, &score_jobs_label(new), "/rank");
        }
        Lane::Shortlist | Lane::New => {
            strong(ui, "No jobs wait here. Search your job sites for new ones.");
            command_action(ui, "Find new jobs…", "/scrape");
        }
        _ => {
            strong(ui, format!("Nothing in {}.", lane_label(lane)));
        }
    }
}

/// The verdict and the checks in one line, worded as the Today card words them.
fn verdict_line(job: &JobView, checks: &JobChecks) -> String {
    match (job.verdict, job.scraper_fit) {
        (Some(verdict), _) => format!("{} {}", fit_band(Some(verdict), job.score.unwrap_or_default()), checks.summary),
        (None, Some(fit)) => format!("Not scored yet. The search guessed a {} fit.", fit_words(fit)),
        (None, None) => "Not scored yet.".to_owned(),
    }
}

fn details(ui: &mut Ui, job: &JobView) {
    let checks = JobChecks::of(job, false);
    panel_body(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_GAP;
            match (job.score, job.verdict) {
                (Some(value), Some(verdict)) => stamp(ui, &value.to_string(), verdict_tone(verdict)),
                _ => stamp(ui, "-", Tone::Plain),
            };
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                muted(ui, job.company.as_str());
                ui.add(egui::Label::new(rich(ui, job.title.as_str(), Type::SLIP_TITLE, Ink::Text)).wrap());
                ui.add(egui::Label::new(rich(ui, verdict_line(job, &checks), Type::NOTE, Ink::Muted)).wrap());
            });
        });
    });
    divider(ui);
    panel_body(ui, |ui| {
        for check in &checks.rows {
            check_row(ui, check);
        }
        if let Some(attempts) = job.fetch_attempts {
            let tries = if attempts == 1 { "1 try".to_owned() } else { format!("{attempts} tries") };
            muted(ui, format!("Could not open after {tries}: {}", job.fetch_error.as_deref().unwrap_or("the page did not load")));
        }
        if !job.signals.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for signal in &job.signals {
                    let (text, tone) = signal_tag(signal);
                    tag(ui, text, tone);
                }
            });
        }
    });
    if !job.strengths.is_empty() || !job.gaps.is_empty() {
        divider(ui);
        panel_body(ui, |ui| {
            bullets(ui, "Why it fits", &job.strengths);
            if !job.strengths.is_empty() && !job.gaps.is_empty() {
                ui.add_space(ITEM_GAP);
            }
            bullets(ui, "What is missing", &job.gaps);
        });
    }
    divider(ui);
    panel_body(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(ROW_GAP, ROW_GAP);
            let apply = job.url.as_ref().map_or_else(|| "/apply".to_owned(), |url| format!("/apply {}", url.as_str()));
            primary_command_action(ui, "Draft application…", &apply);
            command_action(ui, "Find skill gaps…", "/upskill");
            open_link(ui, "Open job ad", job.url.as_ref());
        });
    });
}

/// A check: its name on the left, what the posting says and the result on the right.
/// The value wraps in the room between them, so a guard's reason or a language note shows in full.
fn check_row(ui: &mut Ui, check: &CheckRow) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ROW_GAP;
        muted(ui, check.kind.label());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(mark) = check_mark(check.state) {
                check_tag(ui, mark);
            }
            ui.add(egui::Label::new(check.value.as_str()).wrap());
        });
    });
}

fn bullets(ui: &mut Ui, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    strong(ui, title);
    for item in items {
        muted(ui, item);
    }
}
