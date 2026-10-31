use egui::{Panel, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::discovery::application::{FitLevel, Gate, JobBoard, JobView, Lane, Signal};
use crate::ui_kit::{Palette, Tone, command_button, divider, faint_mono, muted, open_link, panel, panel_body, segmented, strong, tag};

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

impl JobsScreen {
    pub fn show(&mut self, ui: &mut Ui, board: &JobBoard) {
        let options: Vec<_> = Lane::ALL.iter().map(|lane| (*lane, format!("{}  {}", lane_label(*lane), board.count(*lane)))).collect();
        if let Some(lane) = segmented(ui, self.lane, &options) {
            self.lane = lane;
            self.selected = None;
        }
        ui.add_space(10.0);

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

        Panel::right("job_details").exact_size(340.0).resizable(false).frame(egui::Frame::new().inner_margin(egui::Margin { left: 12, ..Default::default() })).show(ui, |ui| {
            if let Some(job) = selected {
                panel(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| details(ui, job));
                });
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin { right: 1, bottom: 1, ..Default::default() })).show(ui, |ui| {
            panel(ui, |ui| {
                ui.set_min_height(ui.available_height());
                egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| self.table(ui, jobs));
            });
        });
    }

    fn table(&mut self, ui: &mut Ui, jobs: &[JobView]) {
        let selected = self.selected.clone();
        TableBuilder::new(ui)
            .striped(false)
            .sense(Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(56.0))
            .column(Column::remainder().at_least(120.0).clip(true))
            .column(Column::exact(140.0).clip(true))
            .column(Column::exact(92.0))
            .column(Column::exact(150.0).clip(true))
            .header(28.0, |mut header| {
                for title in ["Score", "Role", "Location", "Deadline", "Signals"] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).small().color(Palette::of(ui).faint));
                    });
                }
            })
            .body(|body| {
                body.rows(46.0, jobs.len(), |mut row| {
                    let job = &jobs[row.index()];
                    row.set_selected(selected.as_ref() == Some(&job.key));
                    row.col(|ui| {
                        let score = job.score.map_or_else(|| job.scraper_fit.map_or("-", fit_label).to_owned(), |s| s.to_string());
                        ui.label(RichText::new(score).monospace().strong());
                    });
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.add(egui::Label::new(job.title.as_str()).truncate());
                            ui.add(egui::Label::new(RichText::new(job.company.as_str()).color(Palette::of(ui).muted)).truncate());
                        });
                    });
                    row.col(|ui| {
                        ui.add(egui::Label::new(RichText::new(job.location.as_deref().unwrap_or("-")).color(Palette::of(ui).muted)).truncate());
                    });
                    row.col(|ui| {
                        ui.monospace(job.deadline.map_or_else(|| "-".to_owned(), |d| d.format("%d %b").to_string()));
                    });
                    row.col(|ui| {
                        if let Some(signal) = job.signals.first() {
                            let (text, tone) = signal_tag(signal);
                            tag(ui, text, tone);
                            if job.signals.len() > 1 {
                                muted(ui, format!("+{}", job.signals.len() - 1));
                            }
                        }
                    });
                    if row.response().clicked() {
                        self.selected = Some(job.key.clone());
                    }
                });
            });
    }
}

fn empty_state(ui: &mut Ui, lane: Lane, board: &JobBoard) {
    let (text, command) = match lane {
        Lane::Shortlist if board.count(Lane::New) > 0 => (format!("{} new postings wait for scoring.", board.count(Lane::New)), Some("/rank")),
        Lane::Shortlist | Lane::New => ("No postings wait here. Search your portals for new ones.".to_owned(), Some("/scrape")),
        _ => (format!("Nothing in {}.", lane_label(lane)), None),
    };
    strong(ui, text);
    if let Some(command) = command {
        command_button(ui, command);
    }
}

fn details(ui: &mut Ui, job: &JobView) {
    panel_body(ui, |ui| {
        ui.label(RichText::new(job.title.as_str()).size(15.0).family(crate::ui_kit::semibold()));
        muted(ui, [Some(job.company.as_str()), job.location.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" · "));
        ui.add_space(4.0);
        match (job.score, job.verdict) {
            (Some(score), Some(verdict)) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(score.to_string()).monospace().size(26.0));
                    muted(ui, verdict.label());
                });
            }
            _ => {
                if let Some(fit) = job.scraper_fit {
                    muted(ui, format!("Scraper fit: {}. Not scored yet.", fit_label(fit)));
                }
            }
        }
    });
    divider(ui);
    panel_body(ui, |ui| {
        if let Some(gate) = job.location_gate {
            check_row(ui, "Location check", gate_tag(gate));
        }
        if let Some(gate) = job.language_gate {
            check_row(ui, "Language gate", gate_tag(gate));
        }
        for (label, reading) in [("Contract", &job.freelance.contract), ("Day rate", &job.freelance.day_rate), ("Commute", &job.freelance.commute)] {
            check_row(ui, label, gate_tag(reading.gate));
            ui.add(egui::Label::new(RichText::new(reading.value.as_str()).small().color(Palette::of(ui).faint)).truncate());
        }
        let guard = match job.guard.as_deref() {
            Some("clean") => ("Clean", Tone::Hired),
            Some(_) => ("Flagged", Tone::Interview),
            None => ("Not checked", Tone::Plain),
        };
        check_row(ui, "Posting guard", guard);
        if job.guard_flagged {
            muted(ui, job.guard.as_deref().unwrap_or_default());
        }
        if let Some(note) = &job.language_note {
            muted(ui, note);
        }
        if let Some(attempts) = job.fetch_attempts {
            muted(ui, format!("Attempt {attempts}: {}", job.fetch_error.as_deref().unwrap_or("fetch failed")));
        }
        for signal in &job.signals {
            let (text, tone) = signal_tag(signal);
            tag(ui, text, tone);
        }
    });
    if !job.strengths.is_empty() || !job.gaps.is_empty() {
        divider(ui);
        panel_body(ui, |ui| {
            bullets(ui, "Strengths", &job.strengths);
            bullets(ui, "Gaps", &job.gaps);
        });
    }
    divider(ui);
    panel_body(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let apply = job.url.as_ref().map_or_else(|| "/apply".to_owned(), |url| format!("/apply {}", url.as_str()));
            command_button(ui, &apply);
            open_link(ui, "Open posting", job.url.as_ref());
        });
        if let Some(portal) = &job.portal {
            faint_mono(ui, portal);
        }
    });
}

fn check_row(ui: &mut Ui, label: &str, (text, tone): (&str, Tone)) {
    ui.horizontal(|ui| {
        muted(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| tag(ui, text, tone));
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
    ui.add_space(4.0);
}
