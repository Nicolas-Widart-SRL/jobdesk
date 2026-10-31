use std::collections::HashMap;
use std::path::PathBuf;

use chrono::NaiveDate;
use egui::{Align, ColorImage, Frame, Layout, Margin, ScrollArea, TextureHandle, TextureOptions, Ui, vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::dossier::application::{CheckResult, Dossier, KeywordRow, NamedText, OutcomeRecord, PageImage, PageState, PageStore, PdfDocument, StatusBucket};
use crate::ui_kit::tokens::{GUTTER, LEADING_TAG_WIDTH, PAGE_GAP, PAGE_SIZE, SPLIT_GAP, SPLIT_SIDE_BASIS, TABLE_CELL_PADDING, TABLE_HEAD_PADDING, TAG_HEIGHT};
use crate::ui_kit::{
    Ink, Subtitle, Title, Tone, Type, command_action, divider, table_rule, list_row, muted, open_link_button, page_body, page_frame, page_head, panel, panel_body, panel_header,
    rich, segmented, side_column, slip, small_button, table_head, tag,
};

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

/// The canvas split: the main column on the left, a slip column on the right.
fn split(ui: &mut Ui, id: &'static str, main: impl FnOnce(&mut Ui), side: impl FnOnce(&mut Ui)) {
    side_column(id, SPLIT_SIDE_BASIS + GUTTER as f32).show(ui, |ui| {
        ScrollArea::vertical().id_salt((id, "side")).auto_shrink([false, false]).show(ui, side);
    });
    let gap = Margin { right: (SPLIT_GAP - GUTTER as f32) as i8, ..Default::default() };
    egui::CentralPanel::default().frame(Frame::new().inner_margin(gap)).show(ui, |ui| {
        ScrollArea::vertical().id_salt((id, "main")).auto_shrink([false, false]).show(ui, main);
    });
}

fn short_date(date: NaiveDate) -> String {
    date.format("%-d %b").to_string()
}

impl DossierScreen {
    pub fn reset(&mut self) {
        self.section = Section::Documents;
        self.selected_pack = 0;
    }

    pub fn show(&mut self, ui: &mut Ui, dossier: &Dossier, pages: &mut PageStore) {
        let card = &dossier.card;
        let title = format!("{}, {}", card.company, card.role);
        let folder = card.archive_folder.as_ref().map(|folder| format!("documents/applications/{folder}/"));
        page_head(ui, Title::Slab(&title), folder.as_deref().map(Subtitle::Typed), |ui| {
            let company = card.company.to_lowercase();
            open_link_button(ui, "Open posting", card.source.as_ref());
            command_action(ui, "Prepare interview", &format!("/interview {company}"));
            command_action(ui, "Record a result", &format!("/outcome {company}"));
            if let Some(fit) = card.fit_rating {
                ui.label(rich(ui, format!("Fit {fit}"), Type::BODY, Ink::Text));
            }
            tag(ui, card.status_label.as_str(), bucket_tone(card.bucket));
        });
        page_body(ui, |ui| self.body(ui, dossier, pages));
    }

    fn body(&mut self, ui: &mut Ui, dossier: &Dossier, pages: &mut PageStore) {
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
        ui.add_space(SPLIT_GAP - ui.spacing().item_spacing.y);
        match self.section {
            Section::Documents => self.documents(ui, dossier, pages),
            Section::Evaluation => self.evaluation(ui, dossier),
            Section::Posting => posting(ui, dossier),
            Section::Prep => self.prep(ui, dossier),
            Section::Outcome => outcome(ui, dossier),
        }
    }

    fn documents(&mut self, ui: &mut Ui, dossier: &Dossier, pages: &mut PageStore) {
        let documents: Vec<(&PdfDocument, PageState)> = dossier.contents.documents.iter().map(|document| (document, pages.pages(document))).collect();
        let markdown = &mut self.markdown;
        let textures = &mut self.textures;
        split(
            ui,
            "dossier_documents",
            |ui| {
                panel(ui, |ui| {
                    panel_header(ui, "Compiled PDFs", page_count_note(&documents).as_deref());
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        compiled_pages(ui, textures, &documents);
                    });
                    divider(ui);
                    panel_header(ui, "Keyword coverage", Some("from CV text layer"));
                    keywords(ui, &dossier.findings.keywords);
                });
            },
            |ui| verification(ui, markdown, dossier),
        );
    }

    fn evaluation(&mut self, ui: &mut Ui, dossier: &Dossier) {
        let markdown = &mut self.markdown;
        split(
            ui,
            "dossier_evaluation",
            |ui| {
                panel(ui, |ui| {
                    panel_header(ui, "Fit evaluation", dossier.findings_from.as_deref());
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match &dossier.findings.evaluation {
                            Some(text) => {
                                CommonMarkViewer::new().show(ui, markdown, text);
                            }
                            None => {
                                muted(ui, "The evaluation lives in the /apply run. No saved run matches this application.");
                            }
                        }
                    });
                });
            },
            |ui| sources(ui, dossier),
        );
    }

    fn prep(&mut self, ui: &mut Ui, dossier: &Dossier) {
        let packs = &dossier.contents.prep_packs;
        let company = dossier.card.company.to_lowercase();
        let (markdown, selected) = (&mut self.markdown, &mut self.selected_pack);
        split(
            ui,
            "dossier_prep",
            |ui| {
                if packs.is_empty() {
                    panel(ui, |ui| {
                        panel_header(ui, "Interview prep", None);
                        panel_body(ui, |ui| {
                            ui.set_width(ui.available_width());
                            muted(ui, "No prep pack yet. /interview writes one into this folder.");
                            command_action(ui, "Prepare interview", &format!("/interview {company}"));
                        });
                    });
                    return;
                }
                if packs.len() > 1 {
                    let options: Vec<_> = packs.iter().enumerate().map(|(index, pack)| (index, pack.name.clone())).collect();
                    if let Some(index) = segmented(ui, *selected, &options) {
                        *selected = index;
                    }
                    ui.add_space(8.0);
                }
                let pack = &packs[(*selected).min(packs.len() - 1)];
                panel(ui, |ui| {
                    panel_header(ui, "Interview prep", Some(&pack.name));
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        CommonMarkViewer::new().show(ui, markdown, &pack.text);
                    });
                });
            },
            |ui| stages(ui, dossier.card.outcome.as_ref()),
        );
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

/// "CV 2 pages, Cover letter 1 page", from the documents that have rendered.
fn page_count_note(documents: &[(&PdfDocument, PageState)]) -> Option<String> {
    let counts: Vec<String> = documents
        .iter()
        .filter_map(|(document, state)| match state {
            PageState::Ready(images) => Some(format!("{} {} {}", document.label, images.len(), if images.len() == 1 { "page" } else { "pages" })),
            _ => None,
        })
        .collect();
    (!counts.is_empty()).then(|| counts.join(", "))
}

/// Every page of every document as a thumbnail, in one wrapping row.
fn compiled_pages(ui: &mut Ui, textures: &mut HashMap<(PathBuf, u64), Vec<TextureHandle>>, documents: &[(&PdfDocument, PageState)]) {
    if documents.is_empty() {
        muted(ui, "No compiled PDFs. They appear here after /apply compiles them.");
        return;
    }
    ui.spacing_mut().item_spacing = vec2(PAGE_GAP, PAGE_GAP);
    ui.horizontal_wrapped(|ui| {
        for (document, state) in documents {
            match state {
                PageState::Loading => {
                    muted(ui, format!("Rendering {}", document.label));
                }
                PageState::Failed(reason) => {
                    muted(ui, format!("{}: {reason}", document.label));
                }
                PageState::Ready(images) => {
                    let key = (document.path.clone(), document.modified);
                    let pages = textures.entry(key).or_insert_with(|| textures_for(ui, images));
                    for (index, texture) in pages.iter().enumerate() {
                        let size = texture.size_vec2();
                        let scale = (PAGE_SIZE.0 / size.x).min(PAGE_SIZE.1 / size.y);
                        page_frame(ui, |ui| {
                            ui.add(egui::Image::new((texture.id(), size * scale)).alt_text(format!("{} page {}", document.label, index + 1)));
                        });
                    }
                }
            }
        }
    });
}

/// One table cell of a fixed width, padded like the canvas's table. A single line centres on `height`.
fn cell(ui: &mut Ui, width: f32, height: f32, padding: (f32, f32), add: impl FnOnce(&mut Ui)) {
    let inner = vec2(width - padding.0 * 2.0, height);
    Frame::new().inner_margin(Margin::symmetric(padding.0 as i8, padding.1 as i8)).show(ui, |ui| {
        ui.allocate_ui_with_layout(inner, Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_size(inner);
            add(ui);
        });
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

/// Column shares of the keyword table, from the canvas.
const KEYWORD_COLUMNS: [f32; 4] = [0.28, 0.25, 0.23, 0.24];

fn keywords(ui: &mut Ui, rows: &[KeywordRow]) {
    if rows.is_empty() {
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            muted(ui, "Keyword coverage appears after an /apply run in Jobdesk.");
        });
        return;
    }
    let total = ui.available_width();
    let widths = KEYWORD_COLUMNS.map(|share| (total * share).floor());
    let head = Type::TABLE_HEAD.font();
    let head_height = ui.fonts_mut(|fonts| fonts.row_height(&head));
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (title, width) in ["Keyword", "Priority", "Status", "Where"].into_iter().zip(widths) {
            cell(ui, width, head_height, TABLE_HEAD_PADDING, |ui| {
                table_head(ui, title);
            });
        }
    });
    table_rule(ui);
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            divider(ui);
        }
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            cell(ui, widths[0], TAG_HEIGHT, TABLE_CELL_PADDING, |ui| {
                ui.add(egui::Label::new(rich(ui, row.keyword.as_str(), Type::MONO, Ink::Text)).wrap());
            });
            cell(ui, widths[1], TAG_HEIGHT, TABLE_CELL_PADDING, |ui| {
                ui.add(egui::Label::new(rich(ui, row.priority.as_str(), Type::BODY, Ink::Muted)).wrap());
            });
            cell(ui, widths[2], TAG_HEIGHT, TABLE_CELL_PADDING, |ui| {
                tag(ui, row.status.as_str(), keyword_tone(row));
            });
            cell(ui, widths[3], TAG_HEIGHT, TABLE_CELL_PADDING, |ui| {
                ui.add(egui::Label::new(rich(ui, row.note.as_str(), Type::BODY, Ink::Muted)).wrap());
            });
        });
    }
}

/// A slip row: a label that wraps, with a tag on the right.
fn tagged_row(ui: &mut Ui, first: bool, label: &str, tag_text: &str, tone: Tone) {
    list_row(ui, first, |ui| {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                tag(ui, tag_text, tone);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(label).wrap());
                });
            });
        });
    });
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
            let (text, tone) = if item.result == CheckResult::Pass { ("Pass", Tone::Hired) } else { ("Fail", Tone::Closed) };
            tagged_row(ui, index == 0, &item.label, text, tone);
        }
    });
}

fn sources(ui: &mut Ui, dossier: &Dossier) {
    let guard = dossier.contents.posting.as_ref().and_then(|posting| posting.guard.as_deref());
    slip(ui, |ui| {
        panel_header(ui, "Source checks", None);
        match guard {
            Some(line) if line.trim() == "clean" => tagged_row(ui, true, "Posting guard", "clean", Tone::Hired),
            Some(line) => {
                tagged_row(ui, true, "Posting guard", "flagged", Tone::Interview);
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    muted(ui, line);
                });
            }
            None => tagged_row(ui, true, "Posting guard", "unknown", Tone::Plain),
        }
        divider(ui);
        let research = dossier.contents.research.as_ref();
        let note = research.and_then(|note| note.fetched_on).map(|date| format!("fetched {}", short_date(date)));
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

/// A tag in the leading slot of a row, so the text after it lines up.
fn leading_tag(ui: &mut Ui, text: &str, tone: Tone) {
    let slot = vec2(LEADING_TAG_WIDTH, TAG_HEIGHT);
    ui.allocate_ui_with_layout(slot, Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(slot);
        tag(ui, text, tone);
    });
}

fn faint_date(ui: &mut Ui, date: NaiveDate) {
    ui.label(rich(ui, short_date(date), Type::BODY, Ink::Faint));
}

fn stages(ui: &mut Ui, outcome: Option<&OutcomeRecord>) {
    slip(ui, |ui| {
        panel_header(ui, "Stages", Some("outcome.md"));
        let stages = outcome.map_or(&[][..], |outcome| outcome.stages.as_slice());
        if stages.is_empty() {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                muted(ui, "No stages recorded.");
            });
        }
        let next = stages.iter().position(|stage| !stage.reached);
        for (index, stage) in stages.iter().enumerate() {
            list_row(ui, index == 0, |ui| {
                ui.horizontal(|ui| {
                    match (stage.reached, Some(index) == next) {
                        (true, _) => leading_tag(ui, "Done", Tone::Hired),
                        (false, true) => leading_tag(ui, "Next", Tone::Interview),
                        (false, false) => leading_tag(ui, "Open", Tone::Plain),
                    }
                    ui.label(stage.name.as_str());
                    if let Some(date) = stage.date {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| faint_date(ui, date));
                    }
                });
            });
        }
    });
}

/// One line of the outcome notes: `- 2026-10-21: text` gives the date and the text.
fn note_line(line: &str) -> Option<(Option<NaiveDate>, &str)> {
    let line = line.trim().trim_start_matches(['-', '*']).trim_start();
    if line.is_empty() {
        return None;
    }
    let dated = line.split_once(':').and_then(|(date, text)| NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok().map(|date| (Some(date), text.trim())));
    Some(dated.unwrap_or((None, line)))
}

fn outcome(ui: &mut Ui, dossier: &Dossier) {
    let record = dossier.card.outcome.as_ref();
    split(
        ui,
        "dossier_outcome",
        |ui| {
            panel(ui, |ui| {
                panel_header(ui, "Outcome notes", record.and_then(|outcome| outcome.status.as_deref()));
                let lines: Vec<_> = record.map_or("", |outcome| outcome.notes.as_str()).lines().filter_map(note_line).collect();
                if lines.is_empty() {
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        muted(ui, "No outcome notes yet.");
                    });
                }
                for (index, (date, text)) in lines.into_iter().enumerate() {
                    list_row(ui, index == 0, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(date) = date {
                                faint_date(ui, date);
                            }
                            ui.add(egui::Label::new(text).wrap());
                        });
                    });
                }
            });
        },
        |ui| {
            slip(ui, |ui| {
                panel_header(ui, "Follow-ups", Some(&format!("{} sent", dossier.card.follow_ups_sent)));
                for (index, follow_up) in dossier.contents.follow_ups.iter().enumerate() {
                    follow_up_row(ui, index == 0, follow_up);
                }
                if !dossier.contents.follow_ups.is_empty() {
                    divider(ui);
                }
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let text = if dossier.contents.follow_ups.is_empty() { "No follow-up drafts." } else { "The app never sends mail. You copy the draft and send it yourself." };
                    muted(ui, text);
                });
            });
        },
    );
}

fn follow_up_row(ui: &mut Ui, first: bool, follow_up: &NamedText) {
    list_row(ui, first, |ui| {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if small_button(ui, "Copy").clicked() {
                    ui.ctx().copy_text(follow_up.text.clone());
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(rich(ui, follow_up.name.as_str(), Type::MONO, Ink::Text)).truncate());
                });
            });
        });
        ui.add(egui::Label::new(follow_up.text.as_str()).wrap());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_notes_split_into_dated_lines() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 21);
        assert_eq!(note_line("- 2026-10-21: They valued the replay story."), Some((date, "They valued the replay story.")));
        assert_eq!(note_line("Moved to applied: see tracker"), Some((None, "Moved to applied: see tracker")));
        assert_eq!(note_line("  - "), None);
    }
}
