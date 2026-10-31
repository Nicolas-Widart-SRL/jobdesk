use std::collections::HashMap;
use std::path::PathBuf;

use egui::{ColorImage, Frame, ScrollArea, TextureHandle, TextureOptions, Ui, Vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::dossier::application::{CheckResult, Dossier, KeywordRow, NamedText, OutcomeRecord, PageImage, PageState, PageStore, PdfDocument, StatusBucket};
use crate::ui_kit::{Ink, Tone, Type, button, command_button, divider, faint_mono, list_row, muted, open_link, page_frame, panel, panel_body, panel_header, rich, segmented, side_column, slip, small_button, strong, tag};

const PAGE_WIDTH: f32 = 270.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Section {
    #[default]
    Documents,
    Evaluation,
    Posting,
    Prep,
    Outcome,
}

#[derive(Default)]
pub struct DossierScreen {
    section: Section,
    selected_pack: usize,
    markdown: CommonMarkCache,
    textures: HashMap<(PathBuf, u64), Vec<TextureHandle>>,
}

/// What the user asked for this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DossierAction {
    Back,
}

fn bucket_tone(bucket: StatusBucket) -> Tone {
    match bucket {
        StatusBucket::Drafted => Tone::Drafted,
        StatusBucket::Active => Tone::Active,
        StatusBucket::Interview => Tone::Interview,
        StatusBucket::Offer => Tone::Offer,
        StatusBucket::Hired => Tone::Hired,
        StatusBucket::Closed => Tone::Closed,
    }
}

impl DossierScreen {
    pub fn reset(&mut self) {
        self.section = Section::Documents;
        self.selected_pack = 0;
    }

    pub fn show(&mut self, ui: &mut Ui, dossier: &Dossier, pages: &mut PageStore) -> Option<DossierAction> {
        let action = header(ui, dossier);
        ui.add_space(8.0);
        let sections = [
            (Section::Documents, "Documents".to_owned()),
            (Section::Evaluation, "Evaluation".to_owned()),
            (Section::Posting, "Posting".to_owned()),
            (Section::Prep, format!("Interview prep  {}", dossier.contents.prep_packs.len())),
            (Section::Outcome, format!("Outcome and follow-ups  {}", dossier.contents.follow_ups.len())),
        ];
        if let Some(section) = segmented(ui, self.section, &sections) {
            self.section = section;
        }
        ui.add_space(10.0);
        match self.section {
            Section::Documents => self.documents(ui, dossier, pages),
            Section::Evaluation => self.evaluation(ui, dossier),
            Section::Posting => posting(ui, dossier),
            Section::Prep => self.prep(ui, dossier),
            Section::Outcome => self.outcome(ui, dossier),
        }
        action
    }

    fn documents(&mut self, ui: &mut Ui, dossier: &Dossier, pages: &mut PageStore) {
        side_column("dossier_checks", 380.0).show(ui, |ui| {
            ScrollArea::vertical().id_salt("dossier_checks_scroll").auto_shrink([false, false]).show(ui, |ui| {
                verification(ui, &mut self.markdown, dossier);
                ui.add_space(10.0);
                keywords(ui, dossier);
            });
        });
        egui::CentralPanel::default().frame(Frame::new()).show(ui, |ui| {
            ScrollArea::vertical().id_salt("dossier_pages_scroll").auto_shrink([false, false]).show(ui, |ui| {
                if dossier.contents.documents.is_empty() {
                    panel(ui, |ui| {
                        panel_body(ui, |ui| {
                            ui.set_width(ui.available_width());
                            strong(ui, "No compiled PDFs");
                            muted(ui, "PDFs appear here after /apply compiles them.");
                        });
                    });
                }
                for document in &dossier.contents.documents {
                    self.document(ui, document, pages);
                    ui.add_space(10.0);
                }
            });
        });
    }

    fn document(&mut self, ui: &mut Ui, document: &PdfDocument, pages: &mut PageStore) {
        panel(ui, |ui| {
            let name = document.path.file_name().map(|name| name.to_string_lossy().into_owned());
            panel_header(ui, &document.label, name.as_deref());
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                match pages.pages(document) {
                    PageState::Loading => {
                        muted(ui, "Rendering pages");
                    }
                    PageState::Failed(reason) => {
                        muted(ui, reason);
                    }
                    PageState::Ready(images) => {
                        let key = (document.path.clone(), document.modified);
                        let textures = self.textures.entry(key).or_insert_with(|| textures_for(ui, &images));
                        ui.horizontal_wrapped(|ui| {
                            for texture in textures.iter() {
                                let size = texture.size_vec2();
                                let shown = Vec2::new(PAGE_WIDTH, PAGE_WIDTH * size.y / size.x);
                                page_frame(ui, |ui| {
                                    ui.add(egui::Image::new((texture.id(), shown)).alt_text(format!("{} page", document.label)));
                                });
                            }
                        });
                    }
                }
            });
        });
    }

    fn evaluation(&mut self, ui: &mut Ui, dossier: &Dossier) {
        side_column("dossier_sources", 340.0).show(ui, |ui| sources(ui, dossier));
        egui::CentralPanel::default().frame(Frame::new()).show(ui, |ui| {
            panel(ui, |ui| {
                panel_header(ui, "Fit evaluation", dossier.findings_from.as_deref());
                ScrollArea::vertical().id_salt("dossier_evaluation").auto_shrink([false, true]).show(ui, |ui| {
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match &dossier.findings.evaluation {
                            Some(text) => {
                                CommonMarkViewer::new().show(ui, &mut self.markdown, text);
                            }
                            None => {
                                muted(ui, "The evaluation lives in the /apply run. No saved run matches this application.");
                            }
                        }
                    });
                });
            });
        });
    }

    fn prep(&mut self, ui: &mut Ui, dossier: &Dossier) {
        let packs = &dossier.contents.prep_packs;
        side_column("dossier_stages", 320.0).show(ui, |ui| stages(ui, dossier.card.outcome.as_ref()));
        egui::CentralPanel::default().frame(Frame::new()).show(ui, |ui| {
            if packs.is_empty() {
                panel(ui, |ui| {
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        strong(ui, "No prep pack yet");
                        command_button(ui, &format!("/interview {}", dossier.card.company.to_lowercase()));
                    });
                });
                return;
            }
            if packs.len() > 1 {
                let options: Vec<_> = packs.iter().enumerate().map(|(index, pack)| (index, pack.name.clone())).collect();
                if let Some(index) = segmented(ui, self.selected_pack, &options) {
                    self.selected_pack = index;
                }
                ui.add_space(8.0);
            }
            let pack = &packs[self.selected_pack.min(packs.len() - 1)];
            panel(ui, |ui| {
                panel_header(ui, &format!("Prep: {}", pack.name), None);
                ScrollArea::vertical().id_salt("dossier_prep").auto_shrink([false, true]).show(ui, |ui| {
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        CommonMarkViewer::new().show(ui, &mut self.markdown, &pack.text);
                    });
                });
            });
        });
    }

    fn outcome(&mut self, ui: &mut Ui, dossier: &Dossier) {
        ui.columns_const(|[left, right]| {
            panel(left, |ui| {
                panel_header(ui, "Outcome notes", dossier.card.outcome.as_ref().and_then(|outcome| outcome.status.as_deref()));
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match dossier.card.outcome.as_ref().map(|outcome| outcome.notes.as_str()).filter(|notes| !notes.is_empty()) {
                        Some(notes) => {
                            ui.label(notes);
                        }
                        None => {
                            muted(ui, "No outcome notes yet.");
                        }
                    }
                });
            });
            ScrollArea::vertical().id_salt("dossier_follow_ups").auto_shrink([false, true]).show(right, |ui| {
                panel(ui, |ui| {
                    panel_header(ui, "Follow-ups", Some(&format!("{} sent", dossier.card.follow_ups_sent)));
                    if dossier.contents.follow_ups.is_empty() {
                        panel_body(ui, |ui| {
                            ui.set_width(ui.available_width());
                            muted(ui, "No follow-up drafts.");
                        });
                    }
                    for (index, follow_up) in dossier.contents.follow_ups.iter().enumerate() {
                        if index > 0 {
                            divider(ui);
                        }
                        follow_up_row(ui, follow_up);
                    }
                });
            });
        });
    }
}

fn textures_for(ui: &Ui, images: &[PageImage]) -> Vec<TextureHandle> {
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let color = ColorImage::from_rgba_unmultiplied([image.width, image.height], &image.rgba);
            ui.ctx().load_texture(format!("pdf_page_{index}"), color, TextureOptions::LINEAR)
        })
        .collect()
}

fn header(ui: &mut Ui, dossier: &Dossier) -> Option<DossierAction> {
    let card = &dossier.card;
    let mut action = None;
    ui.horizontal(|ui| {
        if button(ui, "Tracker").clicked() {
            action = Some(DossierAction::Back);
        }
        ui.label(rich(ui, format!("{} · {}", card.company, card.role), Type::SECTION, Ink::Text));
        tag(ui, card.status_label.as_str(), bucket_tone(card.bucket));
        if let Some(fit) = card.fit_rating {
            ui.monospace(format!("Fit {fit}"));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            open_link(ui, "Open posting", card.source.as_ref());
            let company = card.company.to_lowercase();
            command_button(ui, &format!("/interview {company}"));
            command_button(ui, &format!("/outcome {company}"));
        });
    });
    action
}

fn verification(ui: &mut Ui, markdown: &mut CommonMarkCache, dossier: &Dossier) {
    let checklist = &dossier.findings.checklist;
    let passed = checklist.iter().filter(|item| item.result == CheckResult::Pass).count();
    let note = (!checklist.is_empty()).then(|| format!("{passed} of {}", checklist.len()));
    slip(ui, |ui| {
        panel_header(ui, "Verification", note.as_deref());
        if let Some(text) = &dossier.findings.verification_text {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                CommonMarkViewer::new().show(ui, markdown, text);
            });
        } else if checklist.is_empty() {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                muted(ui, "Results appear after an /apply run in Jobdesk.");
            });
        }
        for (index, item) in checklist.iter().enumerate() {
            list_row(ui, index == 0, |ui| {
                ui.horizontal(|ui| {
                    let (text, tone) = if item.result == CheckResult::Pass { ("Pass", Tone::Hired) } else { ("Fail", Tone::Closed) };
                    tag(ui, text, tone);
                    ui.add(egui::Label::new(item.label.as_str()).wrap());
                });
            });
        }
    });
}

fn keyword_tone(row: &KeywordRow) -> Tone {
    let status = row.status.to_lowercase();
    if status.contains("gap") {
        Tone::Closed
    } else if status.contains("missing") || status.contains("synonym") {
        Tone::Interview
    } else {
        Tone::Hired
    }
}

fn keywords(ui: &mut Ui, dossier: &Dossier) {
    let rows = &dossier.findings.keywords;
    if rows.is_empty() {
        return;
    }
    slip(ui, |ui| {
        panel_header(ui, "Keyword coverage", Some("CV text layer"));
        for (index, row) in rows.iter().enumerate() {
            list_row(ui, index == 0, |ui| {
                ui.horizontal(|ui| {
                    ui.monospace(row.keyword.as_str());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| tag(ui, row.status.as_str(), keyword_tone(row)));
                });
                muted(ui, [row.priority.as_str(), row.note.as_str()].into_iter().filter(|text| !text.is_empty()).collect::<Vec<_>>().join(" · "));
            });
        }
    });
}

fn sources(ui: &mut Ui, dossier: &Dossier) {
    ScrollArea::vertical().id_salt("dossier_sources_scroll").auto_shrink([false, false]).show(ui, |ui| {
        let guard = dossier.contents.posting.as_ref().and_then(|posting| posting.guard.as_deref());
        slip(ui, |ui| {
            panel_header(ui, "Source checks", None);
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                match guard {
                    Some(line) if line.trim() == "clean" => {
                        tag(ui, "Posting guard clean", Tone::Hired);
                    }
                    Some(line) => {
                        tag(ui, "Posting guard flagged", Tone::Interview);
                        muted(ui, line);
                    }
                    None => {
                        tag(ui, "Posting guard unknown", Tone::Plain);
                    }
                }
            });
        });
        ui.add_space(10.0);
        slip(ui, |ui| {
            let research = dossier.contents.research.as_ref();
            let note = research.and_then(|note| note.fetched_on).map(|date| format!("fetched {date}"));
            panel_header(ui, "Company research", note.as_deref());
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                match research {
                    Some(note) => {
                        if note.is_stale {
                            tag(ui, "Older than 30 days", Tone::Interview);
                        }
                        ui.add(egui::Label::new(rich(ui, &note.text, Type::MONO, Ink::Text)).wrap());
                    }
                    None => {
                        muted(ui, "No cached research for this company.");
                    }
                }
            });
        });
    });
}

/// Postings are third-party text, so they render as plain text and never as Markdown.
fn posting(ui: &mut Ui, dossier: &Dossier) {
    panel(ui, |ui| {
        panel_header(ui, "Archived posting", Some("job_posting.md"));
        ScrollArea::vertical().id_salt("dossier_posting").auto_shrink([false, true]).show(ui, |ui| {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                match &dossier.contents.posting {
                    Some(posting) => {
                        ui.add(egui::Label::new(posting.text.as_str()).wrap());
                    }
                    None => {
                        muted(ui, "No archived posting. /apply saves it when it records the application.");
                    }
                }
            });
        });
    });
}

fn stages(ui: &mut Ui, outcome: Option<&OutcomeRecord>) {
    slip(ui, |ui| {
        panel_header(ui, "Stages", Some("outcome.md"));
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            let stages = outcome.map_or(&[][..], |outcome| outcome.stages.as_slice());
            if stages.is_empty() {
                muted(ui, "No stages recorded.");
            }
            for stage in stages {
                ui.horizontal(|ui| {
                    if stage.reached {
                        tag(ui, "Done", Tone::Hired);
                    } else {
                        tag(ui, "Open", Tone::Plain);
                    }
                    ui.label(stage.name.as_str());
                    if let Some(date) = stage.date {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| faint_mono(ui, date.to_string()));
                    }
                });
            }
        });
    });
}

fn follow_up_row(ui: &mut Ui, follow_up: &NamedText) {
    panel_body(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            faint_mono(ui, &follow_up.name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if small_button(ui, "Copy").on_hover_text("You send follow-ups yourself").clicked() {
                    ui.ctx().copy_text(follow_up.text.clone());
                }
            });
        });
        ui.add(egui::Label::new(follow_up.text.as_str()).wrap());
    });
}
