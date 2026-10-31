use egui::{Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::discovery::application::{FitLevel, Gate, JobBoard, JobView, Lane, Signal, Verdict};
use crate::ui_kit::tokens::{ITEM_GAP, ROW_GAP, ROW_ITEM_GAP, SCREEN_GAP, TABLE_HEAD_HEIGHT};
use crate::ui_kit::{
    CellRule, Ink, Subtitle, Title, Tone, Type, command_action, divider, muted, open_link, page_body, page_head, panel, panel_body, primary_command_action, rich, score, segmented, side_column, slip, stamp,
    strong, table_cell, table_head, tag, title_over,
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

fn lane_label(lane: Lane) -> &'static str {
    match lane {
        Lane::Shortlist => "Shortlist",
        Lane::BelowThreshold => "Below threshold",
        Lane::Excluded => "Excluded",
        Lane::CouldNotFetch => "Could not fetch",
        Lane::New => "New",
        Lane::Skipped => "Skipped",
        Lane::Expired => "Expired",
    }
}

fn signal_tag(signal: &Signal) -> (String, Tone) {
    match signal {
        Signal::ClosingIn { days } => (format!("Closing in {days}d"), Tone::Closed),
        Signal::PostedDaysAgo { days } => (format!("Posted {days}d ago"), Tone::Interview),
        Signal::LocationFlag => ("Location flag".into(), Tone::Interview),
        Signal::LanguageFlag => ("Language flag".into(), Tone::Interview),
        Signal::LocationFail => ("Location fail".into(), Tone::Closed),
        Signal::LanguageFail => ("Language fail".into(), Tone::Closed),
        Signal::GuardFlag => ("Guard flag".into(), Tone::Interview),
        Signal::ContractFail => ("Not freelance".into(), Tone::Closed),
        Signal::RateBelowFloor => ("Rate below floor".into(), Tone::Closed),
    }
}

fn gate_tag(gate: Gate) -> (&'static str, Tone) {
    match gate {
        Gate::Pass => ("Pass", Tone::Hired),
        Gate::Flag => ("Flag", Tone::Interview),
        Gate::Fail => ("Fail", Tone::Closed),
    }
}

fn fit_label(fit: FitLevel) -> &'static str {
    match fit {
        FitLevel::High => "High",
        FitLevel::Medium => "Medium",
        FitLevel::Low => "Low",
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

impl JobsScreen {
    pub fn show(&mut self, ui: &mut Ui, board: &JobBoard) {
        let subtitle = Subtitle::Text("Triage scores from posting text only. /apply re-evaluates with company research.");
        page_head(ui, Title::Slab("Jobs"), Some(subtitle), |ui| {
            primary_command_action(ui, "Score new postings", "/rank");
            command_action(ui, "Find new postings", "/scrape");
        });
        page_body(ui, |ui| self.body(ui, board));
    }

    fn body(&mut self, ui: &mut Ui, board: &JobBoard) {
        let options: Vec<_> = Lane::ALL.iter().map(|lane| (*lane, format!("{}  {}", lane_label(*lane), board.count(*lane)))).collect();
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
                slip(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| details(ui, job));
                });
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| {
            // The fixed columns overflow a narrow window, so the table scrolls sideways there.
            panel(ui, |ui| {
                egui::ScrollArea::horizontal().id_salt("jobs_table_x").auto_shrink([false, true]).show(ui, |ui| self.table(ui, jobs));
            });
        });
    }

    fn table(&mut self, ui: &mut Ui, jobs: &[JobView]) {
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
            .column(Column::exact(134.0).clip(true))
            .column(Column::exact(96.0).clip(true))
            .column(Column::exact(84.0))
            .column(Column::exact(140.0).clip(true))
            .column(Column::exact(112.0).clip(true))
            .header(TABLE_HEAD_HEIGHT, |mut header| {
                for title in ["Score", "Role", "Location", "Day rate", "Deadline", "Signals", "Portal"] {
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
                        table_cell(ui, rule, |ui| score(ui, job.score.map_or_else(|| job.scraper_fit.map_or("-", fit_label).to_owned(), |s| s.to_string())));
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| title_over(ui, &job.title, &job.company));
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| ui.add(egui::Label::new(rich(ui, job.location.as_deref().unwrap_or("-"), Type::BODY, Ink::Muted)).truncate()));
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| ui.add(egui::Label::new(day_rate(job)).truncate()));
                    });
                    row.col(|ui| {
                        table_cell(ui, rule, |ui| ui.label(job.deadline.map_or_else(|| "None".to_owned(), |d| d.format("%-d %b").to_string())));
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
                            if let Some(portal) = &job.portal {
                                ui.add(egui::Label::new(rich(ui, portal.as_str(), Type::MONO, Ink::Faint)).truncate());
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
    let waiting = board.count(Lane::New);
    match lane {
        Lane::Shortlist if waiting > 0 => {
            strong(ui, format!("{waiting} new postings wait for scoring."));
            primary_command_action(ui, &format!("Score {waiting} postings"), "/rank");
        }
        Lane::Shortlist | Lane::New => {
            strong(ui, "No postings wait here. Search your portals for new ones.");
            command_action(ui, "Find new postings", "/scrape");
        }
        _ => {
            strong(ui, format!("Nothing in {}.", lane_label(lane)));
        }
    }
}

fn details(ui: &mut Ui, job: &JobView) {
    panel_body(ui, |ui| {
        ui.spacing_mut().item_spacing.y = ROW_ITEM_GAP;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(rich(ui, job.title.as_str(), Type::SLIP_TITLE, Ink::Text));
            muted(ui, [Some(job.company.as_str()), job.location.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(", "));
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            match (job.score, job.verdict) {
                (Some(value), Some(verdict)) => {
                    stamp(ui, &value.to_string(), verdict_tone(verdict));
                    strong(ui, verdict.label());
                }
                _ => {
                    stamp(ui, "-", Tone::Plain);
                    strong(ui, "Not scored");
                }
            }
        });
        if job.score.is_none()
            && let Some(fit) = job.scraper_fit
        {
            muted(ui, format!("Scraper fit: {}.", fit_label(fit)));
        }
    });
    divider(ui);
    panel_body(ui, |ui| {
        for (label, reading) in [("Contract", &job.freelance.contract), ("Day rate", &job.freelance.day_rate), ("Commute", &job.freelance.commute)] {
            gate_row(ui, label, reading.value.as_str(), Some(gate_tag(reading.gate)));
        }
        if let Some(gate) = job.location_gate {
            gate_row(ui, "Location", job.location.as_deref().unwrap_or("Not stated"), Some(gate_tag(gate)));
        }
        if let Some(gate) = job.language_gate {
            gate_row(ui, "Language", job.language_note.as_deref().unwrap_or_default(), Some(gate_tag(gate)));
        }
        match job.guard.as_deref() {
            Some("clean") => gate_row(ui, "Posting guard", "clean", Some(gate_tag(Gate::Pass))),
            Some(guard) => gate_row(ui, "Posting guard", guard, Some(gate_tag(Gate::Flag))),
            None => gate_row(ui, "Posting guard", "Not checked", None),
        }
        if let Some(attempts) = job.fetch_attempts {
            muted(ui, format!("Attempt {attempts}: {}", job.fetch_error.as_deref().unwrap_or("fetch failed")));
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
            bullets(ui, "Strengths", &job.strengths);
            if !job.strengths.is_empty() && !job.gaps.is_empty() {
                ui.add_space(ITEM_GAP);
            }
            bullets(ui, "Gaps", &job.gaps);
        });
    }
    divider(ui);
    panel_body(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(ROW_GAP, ROW_GAP);
            let apply = job.url.as_ref().map_or_else(|| "/apply".to_owned(), |url| format!("/apply {}", url.as_str()));
            primary_command_action(ui, "Draft application", &apply);
            command_action(ui, "Find skill gaps", "/upskill");
            open_link(ui, "Open posting", job.url.as_ref());
        });
    });
}

/// A gate: its name on the left, what the posting says and the reading on the right.
/// The value wraps in the room between them, so a guard's reason or a language note shows in full.
fn gate_row(ui: &mut Ui, label: &str, value: &str, reading: Option<(&str, Tone)>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ROW_GAP;
        muted(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some((text, tone)) = reading {
                tag(ui, text, tone);
            }
            ui.add(egui::Label::new(value).wrap());
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
