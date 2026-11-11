//! One sheet showing every Ledger component, in both themes.

use egui::{CursorIcon, Grid, Role, ScrollArea, Theme, Ui};
use egui_commonmark::CommonMarkCache;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder, SnapshotOptions};
use jobdesk::shared_kernel::placeholders::{self, Piece};
use jobdesk::shell::install_look;
use jobdesk::ui_kit::tokens::{GUTTER, KNOT_SIZE, KNOT_SIZE_SMALL, MARK_RULE, ROW_GAP, RUN_SIDE, TRAY_RAIL};
use jobdesk::ui_kit::{
    AddedFile, AnswersSlip, Change, DropForm, ProposalPart, Question, SheetPart, SubItem, answers_slip, command_chord, cv_check, cv_slip, drop_zone, file_list,
    primary_shortcut_button, question_fallback, question_sheet, reply_slip, review_changes, review_slip, side_slip, slip_text, split_card, wide_card,
};
use jobdesk::ui_kit::{
    AgendaEnd, BandCell, BlockEditor, CardLook, CellLook, ChangedOnDisk, CheckMark, Confirm, DayHeatmap, DealLook, EditorBusy, Figure, FlowNode, HeatDay, Ink,
    KnotState, NodeMarks, NotesAction, PageSet, Palette, ReleaseNotes, RoleEntry, Secret, Subtitle, SweepForm, TabMark, Title, Tone, Type, agenda_return,
    agenda_row, alert, amount_field, attention_count, bar, blank_card, blank_page, block_editor, brand, button, card_skeleton, card_tab, check_tag, code_box,
    command_button, command_row, confirm_popover, copy_button, danger_button, danger_shortcut_button, day_heatmap, day_heatmap_loading, day_stamp, deal,
    deal_footer, desk_label, divider, document, doughnut, empty_state, enabled_when, estimate_note, faint_mono, fat_tally, figure, figure_band,
    figure_band_loading, fitted_pages, fitted_row_tag, flow_band, folder_tab, index_card, key_button, key_hint, knot, lane, lane_title, lead_line, link,
    list_row, live_sweep, measure, muted, name_line, note, page_frame, page_head, panel, panel_body, panel_header, panel_header_tone, paper, path_label,
    pick_row, primary_button, primary_key_button, profile_markdown, prompt_box, prompt_box_tone, rail_title, release_notes_popover, reveal_command, rich,
    role_entry, row_tag, ruled_note_with_command, score, search_field, secret_field, segmented, select_row, set_reduced_motion, setting_check,
    setting_check_amount, setting_check_number, setting_radio, setting_row, setting_section, settings_page, shell_top_frame, shortcut_button, skeleton,
    skeleton_lines, slip, slip_bar, small_button, small_command_action_hinted, small_danger_button, small_key_button, small_toggle, solid_tag, split_paper,
    stamp, stamp_square, stamped_head, status_action, status_button, status_divider, status_dot, status_frame, status_line, status_mark_button,
    status_progress, status_waits, step_line, strong, switch, table_head, tag, tally, tally_strip, text_field, toggle_group, turn_band, turn_band_live,
    undo_row,
};
use jobdesk::ui_kit::{LineEnd, SetupLine, WizardStep, alert_slip, choice_card, setup_line, setup_stamp, step_dots, wizard_card};

/// Snapshot baselines come from one renderer, so only macOS draws and compares them. Every other assertion runs everywhere.
const SNAPSHOTS: bool = cfg!(target_os = "macos");

/// Gives the harness the renderer that draws snapshots, where they are compared.
fn with_renderer<State>(builder: HarnessBuilder<State>) -> HarnessBuilder<State> {
    if SNAPSHOTS { builder.wgpu() } else { builder }
}

/// Compares the harness to the baseline `name`, where snapshots are compared.
fn compare(harness: &mut Harness, name: &str) {
    if SNAPSHOTS {
        harness.snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
    }
}

#[cfg(target_os = "macos")]
#[test]
#[allow(clippy::assertions_on_constants)]
fn snapshots_compare_on_macos() {
    assert!(SNAPSHOTS);
}

/// The width of a side slip in the in-tray's rail.
const SLIP: f32 = TRAY_RAIL - GUTTER as f32;
/// The room a dealt card takes in the sheet, with its tab and the edges behind it.
const DEALT_HEIGHT: f32 = 250.0;

const TONES: [(&str, Tone); 8] = [
    ("Plain", Tone::Plain),
    ("Drafted", Tone::Drafted),
    ("Active", Tone::Active),
    ("Interview", Tone::Interview),
    ("Offer", Tone::Offer),
    ("Hired", Tone::Hired),
    ("Closed", Tone::Closed),
    ("Turn", Tone::Turn),
];

const SWATCHES_PER_ROW: usize = 7;

/// The sheet's size. One point per pixel keeps it under the 8192px a texture holds.
const SHEET: [f32; 2] = [1000.0, 8192.0];

/// The row the pointer rests on in the snapshots.
const HOVERED_ROW: &str = "/match    Pick row, hovered";

fn swatches(ui: &mut Ui) {
    let p = Palette::of(ui);
    let colors = [
        ("sheet", p.sheet),
        ("panel", p.panel),
        ("raised", p.raised),
        ("line", p.line),
        ("field", p.field),
        ("off", p.off),
        ("text", p.text),
        ("muted", p.muted),
        ("faint", p.faint),
        ("primary", p.primary),
        ("on_primary", p.on_primary),
        ("drafted", p.drafted),
        ("active", p.active),
        ("interview", p.interview),
        ("offer", p.offer),
        ("hired", p.hired),
        ("closed", p.closed),
        ("turn", p.turn),
        ("on_turn", p.on_turn),
        ("page", p.page),
        ("page_line", p.page_line),
        ("page_head", p.page_head),
        ("grid", p.grid),
        ("mark", p.mark),
        ("card", p.card),
    ];
    for row in colors.chunks(SWATCHES_PER_ROW) {
        ui.horizontal(|ui| {
            for (name, color) in row {
                ui.vertical(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(72.0, 26.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 0.0, *color);
                    ui.label(rich(ui, *name, Type::SMALL, Ink::Muted));
                });
            }
        });
    }
}

fn type_scale(ui: &mut Ui) {
    for (name, style) in [
        ("Lead 44", Type::LEAD),
        ("Heading 32", Type::H1),
        ("Deal 28", Type::DEAL),
        ("Brand 23", Type::BRAND),
        ("Section 17", Type::SECTION),
        ("Score 21", Type::SCORE),
        ("Path 21", Type::PATH),
        ("Command 18", Type::COMMAND),
        ("Take 15", Type::TAKE),
        ("Body 14", Type::BODY),
        ("Strong 14", Type::STRONG),
        ("Mono 13.5", Type::MONO),
        ("Note 13", Type::NOTE),
        ("Small 12.5", Type::SMALL),
        ("Key 11.5", Type::KEY),
    ] {
        ui.label(rich(ui, name, style, Ink::Text));
    }
}

fn text_styles(ui: &mut Ui) {
    muted(ui, "Muted body text");
    strong(ui, "Strong body text");
    faint_mono(ui, "cv/main_harbor.pdf");
    alert(ui, "One source could not be read");
    table_head(ui, "Table head");
    score(ui, "78");
    figure(ui, "12", Tone::Active);
}

fn buttons(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        button(ui, "Plain");
        primary_button(ui, "Primary");
        danger_button(ui, "Deny");
        small_button(ui, "Small");
        small_danger_button(ui, "Stop run");
        key_button(ui, "Later", "L");
        primary_key_button(ui, "Send", "Y");
        command_button(ui, "/rank");
        link(ui, "Show the command");
        copy_button(ui, "A draft");
    });
    // Disabled keeps the opaque rest fill, with ink and border at half. A primary's fill is at half over the paper.
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        muted(ui, "Disabled");
        ui.add_enabled_ui(false, |ui| {
            button(ui, "Plain");
            primary_button(ui, "Primary");
            danger_button(ui, "Deny");
            small_button(ui, "Small");
            key_button(ui, "Later", "L");
            primary_key_button(ui, "Send", "Y");
        });
    });
}

fn shell(ui: &mut Ui) {
    shell_top_frame().inner_margin(12).show(ui, |ui| {
        ui.horizontal(|ui| {
            brand(ui);
            ui.add_space(24.0);
            shortcut_button(ui, "New task", &["Cmd", "K"]);
            attention_count(ui, 8);
            status_dot(ui, Tone::Hired);
        });
    });
    status_frame(ui).show(ui, |ui| {
        ui.horizontal(|ui| {
            status_line(ui, true, "Claude is ready");
            status_line(ui, false, "Claude Code is not installed");
        });
    });
    page_head(ui, Title::Slab("All commands"), Some(Subtitle::Text("Every action is the unchanged slash command.")), |ui| {
        primary_button(ui, "Find skill gaps");
    });
    panel_header_tone(ui, "Interview", Some("2 waiting"), Tone::Interview);
}

fn marks(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        for (name, tone) in TONES {
            tag(ui, name, tone);
        }
        solid_tag(ui, "Your turn", Tone::Turn);
    });
    ui.horizontal(|ui| {
        stamp(ui, "78", Tone::Hired);
        stamp(ui, "61", Tone::Active);
        stamp(ui, "42", Tone::Interview);
        for key in ["Y", "L", "N", "U"] {
            key_hint(ui, key);
        }
    });
    ui.horizontal(|ui| {
        tally(ui, 12, 20, Tone::Active);
        fat_tally(ui, 5, 8, Tone::Hired);
    });
    tally_strip(ui, &[(6, Some(Ink::Status(Tone::Active))), (3, Some(Ink::Status(Tone::Interview))), (1, Some(Ink::Status(Tone::Offer))), (4, None)], false);
    tally_strip(ui, &[(5, Some(Ink::Status(Tone::Closed))), (3, None)], true);
    bar(ui, 0.6, Ink::Text);
    bar(ui, 0.35, Tone::Offer);
    measure(ui, "Interview", 3, 10, Tone::Interview);
    let segments = [(4, Tone::Active), (3, Tone::Interview), (1, Tone::Offer), (2, Tone::Closed)];
    ui.horizontal(|ui| {
        doughnut(ui, 96.0, &segments);
        doughnut(ui, 96.0, &[]);
    });
}

fn tabs(ui: &mut Ui) {
    shell_top_frame().inner_margin(12).show(ui, |ui| {
        ui.horizontal(|ui| {
            folder_tab(ui, "Today", TabMark::Count(8), true);
            folder_tab(ui, "Jobs", TabMark::None, false);
            folder_tab(ui, "Applications", TabMark::Failed, false);
            folder_tab(ui, "Tasks", TabMark::Count(1), false);
        });
    });
    ui.add_space(10.0);
    segmented(ui, 1, &[(0, "Documents".to_owned()), (1, "Evaluation".to_owned()), (2, "Posting".to_owned())]);
}

fn sections(ui: &mut Ui) {
    ui.columns_const(|[left, right]| {
        panel(left, |ui| {
            panel_header(ui, "A section", Some("note"));
            list_row(ui, true, |ui| {
                strong(ui, "List row");
            });
            list_row(ui, false, |ui| {
                muted(ui, "Ruled off from the row above");
            });
            divider(ui);
            select_row(ui, "/rank     Pick row, on", Type::MONO, true);
            select_row(ui, "/scrape   Pick row, off", Type::MONO, false);
            select_row(ui, HOVERED_ROW, Type::MONO, false);
            command_row(ui, "/rank", "Score new postings", "seen_jobs.json", true);
            command_row(ui, "/apply", "Evaluate and draft", "cv, cover_letters, tracker", false);
            panel_body(ui, |ui| {
                muted(ui, "Panel body");
            });
        });
        slip(right, |ui| {
            panel_header(ui, "A side slip", None);
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                muted(ui, "Cost");
                slip_bar(ui, 0.4, Ink::Text);
                pick_row(ui, true, |ui| {
                    strong(ui, "Run row, picked");
                });
                pick_row(ui, false, |ui| {
                    strong(ui, "Run row");
                });
            });
        });
        right.add_space(10.0);
        note(right, |ui| {
            ui.label("A note across the screen");
        });
    });
    ui.add_space(10.0);
    let stages = [
        ("Scrape", "15", vec![(9, Some(Ink::Text)), (6, None)], "new of 64 seen"),
        ("Rank", "5", vec![(5, Some(Ink::Text))], "on shortlist"),
        ("Apply", "2", vec![(2, Some(Ink::Text))], "drafted, not sent"),
    ];
    let nodes: Vec<FlowNode> = stages
        .into_iter()
        .map(|(label, figure, marks, note)| FlowNode { label, figure: figure.to_owned(), marks: NodeMarks::Runs(marks), note: note.to_owned(), hidden: None })
        .collect();
    flow_band(ui, &nodes);
    ui.add_space(10.0);
    code_box(ui, |ui| {
        ui.label(rich(ui, "cd cv && lualatex main_harbor.tex", Type::MONO, Ink::Text));
    });
    ui.add_space(10.0);
    prompt_box(ui, |ui| {
        strong(ui, "A prompt that needs an answer");
    });
    ui.add_space(10.0);
    prompt_box_tone(ui, Tone::Turn, |ui| {
        strong(ui, "A prompt that needs you");
    });
    ui.add_space(10.0);
    page_frame(ui, |ui| {
        ui.allocate_exact_size(egui::vec2(120.0, 80.0), egui::Sense::hover());
    });
    ui.add_space(10.0);
    empty_state(
        ui,
        "Nothing here yet",
        "A sentence on what the screen holds and how to fill it, at most 560 px wide, then the actions that open the launcher.",
        |ui| {
            primary_button(ui, "Find things…");
            button(ui, "Something else…");
        },
    );
}

fn board(ui: &mut Ui) {
    ui.columns_const(|[left, right]| {
        lane(left, |ui| {
            lane_title(ui, "Interview", Tone::Interview);
            index_card(ui, CardLook { on: true, ..CardLook::default() }, "Picked card", |ui| {
                strong(ui, "Picked card");
                muted(ui, "Lead Java Engineer");
            });
            index_card(ui, CardLook { waits: true, ..CardLook::default() }, "Waiting card", |ui| {
                strong(ui, "Waiting card");
                ui.label(rich(ui, "No reply in 15 days", Type::TAG, Tone::Turn));
            });
            index_card(ui, CardLook { faded: true, ..CardLook::default() }, "Faded card", |ui| {
                strong(ui, "Faded card");
                ui.label(rich(ui, "fit 68", Type::SMALL, Ink::Muted));
            });
            blank_card(ui, |ui| skeleton_lines(ui, &[150.0, 110.0, 80.0], card_skeleton));
        });
        Grid::new("table").spacing([24.0, 6.0]).show(right, |ui| {
            table_head(ui, "Score");
            table_head(ui, "Role");
            ui.end_row();
            score(ui, "78");
            ui.label("Lead Java Engineer");
            ui.end_row();
        });
    });
}

fn tray(ui: &mut Ui) {
    dealt(ui, "send", |ui| {
        let tab = card_tab(ui, "Ready to send", Tone::Drafted, "Due today");
        deal(
            ui,
            tab,
            DealLook { key: "send", frame: None, behind: 3 },
            |ui| {
                ui.label(rich(ui, "A dealt card", Type::DEAL, Ink::Text));
            },
            |ui| {
                primary_key_button(ui, "Mark as sent…", "Y");
                key_button(ui, "Later", "L");
            },
        );
    });
    ui.add_space(10.0);
    dealt(ui, "asks", |ui| {
        let tab = card_tab(ui, "Claude asks", Tone::Turn, "Paused task");
        deal(
            ui,
            tab,
            DealLook { key: "asks", frame: Some(Tone::Interview), behind: 9 },
            |ui| {
                ui.label(rich(ui, "A card that waits", Type::DEAL, Ink::Text));
            },
            |ui| {
                key_button(ui, "Later", "L");
            },
        );
    });
    ui.add_space(10.0);
    dealt(ui, "turn", |ui| {
        let tab = card_tab(ui, "Your turn", Tone::Turn, "Task waits");
        deal(
            ui,
            tab,
            DealLook { key: "turn", frame: Some(Tone::Turn), behind: 1 },
            |ui| {
                ui.label(rich(ui, "A card for a task that waits", Type::DEAL, Ink::Text));
            },
            |ui| {
                key_button(ui, "Later", "L");
            },
        );
    });
    ui.add_space(10.0);
    ui.scope(|ui| {
        ui.set_max_width(SLIP);
        slip(ui, |ui| {
            panel_header(ui, "Done today", Some("2 of 5"));
            panel_body(ui, |ui| {
                fat_tally(ui, 2, 5, Ink::Text);
            });
            let toggles = [("Rate", false), ("Scope", true), ("Domain", false), ("Location", false)];
            undo_row(ui, "Quarry Labs", "Skipped. Why? Optional.", "Last decision: Skipped, Quarry Labs. Press U to undo.", &toggles, false);
            panel_body(ui, |ui| {
                ui.label("Saltmarsh Energy");
            });
        });
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        small_key_button(ui, "Undo", "U");
        small_toggle(ui, "Scope", true);
        small_toggle(ui, "Rate", false);
    });
}

/// The height the sample Settings page takes in the sheet.
const SETTINGS_PAGE_HEIGHT: f32 = 640.0;

fn settings(ui: &mut Ui) {
    ui.columns_const(|[left, right]| {
        slip(left, |ui| {
            panel_body(ui, |ui| {
                search_field(ui, &mut String::new(), "Search settings");
                search_field(ui, &mut "spend".to_owned(), "Search settings");
                rail_title(ui, "Personal");
                pick_row(ui, true, |ui| {
                    strong(ui, "Settings row, picked");
                });
                pick_row(ui, false, |ui| {
                    strong(ui, "Settings row");
                });
                enabled_when(ui, false, "Waits for the task that runs", |ui| {
                    pick_row(ui, false, |ui| {
                        strong(ui, "Settings row, disabled");
                    })
                });
            });
        });
        status_frame(right).show(right, |ui| {
            ui.vertical(|ui| {
                status_waits(ui, "Claude waits on you");
                ui.horizontal(|ui| {
                    status_button(ui, "Settings", &["Cmd", ","], false);
                    status_button(ui, "Settings", &["Cmd", ","], true);
                    ui.add_enabled_ui(false, |ui| status_button(ui, "Settings", &["Cmd", ","], false));
                });
            });
        });
    });
    ui.add_space(10.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), SETTINGS_PAGE_HEIGHT), |ui| {
        settings_page(
            ui,
            "Appearance",
            "How Jobdesk looks on this computer.",
            |ui| {
                button(ui, "Check again");
            },
            |ui| {
                setting_section(ui, "Look", None, "");
                setting_row(ui, "Theme", "System follows your Mac.", "", |ui| {
                    toggle_group(ui, Some(1), &[(0, "System"), (1, "Light"), (2, "Dark")]);
                });
                setting_row(ui, "Text size", "Off a step, no size is on.", "", |ui| {
                    toggle_group(ui, None, &[(90, "90%"), (100, "100%"), (110, "110%")]);
                });
                setting_section(ui, "Motion", Some("Decoration only: off, every change shows at once."), "");
                setting_check(ui, &mut false, "Reduce motion", "Cards and the day stamp change at once, with no slide.", "");
                setting_section(ui, "Spend", Some("A search for spend marks every place it shows."), "spend");
                setting_row(ui, "Spend at most, USD", "Empty starts each command at its suggested budget, so tasks spend what they need.", "spend", |ui| {
                    button(ui, "Suggested");
                });
                ui.add_enabled_ui(false, |ui| {
                    setting_check(ui, &mut true, "When a task finishes", "One notification per task, with what it cost.", "");
                    setting_row(ui, "Open on", "The screen you see first.", "", |ui| {
                        toggle_group(ui, Some(0), &[(0, "Today"), (1, "Jobs")]);
                    });
                });
            },
        );
    });
}

fn runs(ui: &mut Ui) {
    let at_stop = [(1, Some(Ink::Text)), (1, Some(Ink::Status(Tone::Turn))), (5, None)];
    let working = [(2, Some(Ink::Text)), (1, Some(Ink::Status(Tone::Active))), (4, None)];
    let finished = [(7, Some(Ink::Text))];
    turn_band(
        ui,
        &[
            BandCell { label: "Where", value: "1 Evaluate, done", note: "Draft waits for your go", look: CellLook::Plain, marks: &at_stop },
            BandCell { label: "Whose turn", value: "Yours", note: "Claude stopped 2 min ago", look: CellLook::Yours, marks: &[] },
            BandCell { label: "What next", value: "Answer Claude", note: "Its question is right below.", look: CellLook::Plain, marks: &[] },
        ],
        false,
        "Where the run is",
        "1 of 7 steps done",
    );
    ui.add_space(10.0);
    turn_band(
        ui,
        &[
            BandCell { label: "Where", value: "2 Draft", note: "2 of 7 steps done", look: CellLook::Plain, marks: &working },
            BandCell { label: "Whose turn", value: "Claude's", note: "Last update 3s ago", look: CellLook::Ink(Tone::Active), marks: &[] },
            BandCell {
                label: "What next",
                value: "Nothing for now",
                note: "You can leave. The Tasks tab marks it when it needs you.",
                look: CellLook::Plain,
                marks: &[],
            },
        ],
        false,
        "Where the run is",
        "2 of 7 steps done",
    );
    ui.add_space(10.0);
    turn_band(
        ui,
        &[
            BandCell { label: "Where", value: "All 7 steps", note: "Finished in 14:20", look: CellLook::Plain, marks: &finished },
            BandCell { label: "Whose turn", value: "Nobody", note: "The session is closed", look: CellLook::Plain, marks: &[] },
            BandCell { label: "What next", value: "Read the documents", note: "Open the dossier, then send it yourself.", look: CellLook::Plain, marks: &[] },
        ],
        false,
        "Where the run is",
        "7 of 7 steps done",
    );
    ui.add_space(10.0);
    turn_band(
        ui,
        &[
            BandCell { label: "Where", value: "1 Evaluate, done", note: "Draft waits for your go", look: CellLook::Plain, marks: &at_stop },
            BandCell { label: "Whose turn", value: "Yours", note: "Claude stopped 2 min ago", look: CellLook::Yours, marks: &[] },
            BandCell { label: "What next", value: "Answer Claude", note: "Its question is right below.", look: CellLook::Plain, marks: &[] },
        ],
        true,
        "Where the run is",
        "1 of 7 steps done",
    );
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        for size in [KNOT_SIZE, KNOT_SIZE_SMALL] {
            knot(ui, "1", KnotState::Done, size);
            knot(ui, "2", KnotState::Working, size);
            knot(ui, "3", KnotState::Waiting, size);
            knot(ui, "5", KnotState::Failed, size);
        }
    });
    ui.add_space(10.0);
    step_line(ui, Some(("1", KnotState::Done)), "Evaluate", "Read your profile and scored the posting.", Some(("Show 3 actions", false)));
    divider(ui);
    step_line(ui, None, "You", "\u{201c}Yes, proceed with drafting.\u{201d}", None);
    divider(ui);
    step_line(ui, Some(("2", KnotState::Working)), "Draft", "Wrote the CV. Writing the cover letter now.", Some(("Hide actions", true)));
    path_label(ui, ".claude/skills/job-application-assistant/", "05-cv-tailoring.md");
    path_label(ui, "cv/", "main_harbor_tech_lead.tex");
    divider(ui);
    step_line(ui, Some(("3", KnotState::Waiting)), "Verify", "Waits for your answer on the CV template.", Some(("Show 4 actions so far", false)));
    divider(ui);
    step_line(ui, Some(("5", KnotState::Failed)), "Compile", "The cover letter did not build.", Some(("Show 2 actions", false)));
}

const DOCUMENT: &str = "# Job fit evaluation: Tech Lead, Java Platform\n\nVerdict: Good fit, 74 out of 100. Every hard gate passes, and the two gaps are small.\n\n## Gates\n\n| Gate | In the posting | Result |\n|---|---|---|\n| Contract | Permanent, full time | Pass |\n| Work mode | Hybrid, 2 days a week | Pass |\n| Salary | Not stated | Check |\n\n## Strengths\n\n- Eight years of Java and Spring Boot, the core stack in the posting.\n- You led a platform team of six. This role leads five.\n\nShall I go ahead and draft the CV and cover letter for this role, or stop here?";

fn document_sample(ui: &mut Ui) {
    document(ui, &mut CommonMarkCache::default(), DOCUMENT);
}

/// A dealt card takes the height it is given, so each gets a fixed room and its own id for its footer.
fn dealt(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    ui.push_id(id, |ui| {
        ui.allocate_ui(egui::vec2(ui.available_width(), DEALT_HEIGHT), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    });
}

fn postmark(ui: &mut Ui) {
    ui.horizontal(|ui| {
        day_stamp(ui, "Sat", "31", "Oct", 11, 3, "Saturday 31 October. 3 of 11 decided today.");
        day_stamp(ui, "Sat", "31", "Oct", 11, 11, "Saturday 31 October. All 11 decided today.");
        day_stamp(ui, "Sat", "31", "Oct", 0, 0, "Saturday 31 October");
        day_stamp(ui, "Sun", "1", "Nov", 60, 20, "Sunday 1 November. 20 of 60 decided today.");
    });
    ui.horizontal(|ui| {
        solid_tag(ui, "Now", Tone::Turn);
        stamp_square(ui, "Needs you");
        knot(ui, "1", KnotState::Next, KNOT_SIZE);
        knot(ui, "2", KnotState::Ahead, KNOT_SIZE);
        check_tag(ui, CheckMark::Ok);
        check_tag(ui, CheckMark::Unclear);
        check_tag(ui, CheckMark::Fails);
        tally(ui, 3, 8, Tone::Turn);
    });
    lead_line(ui, "Good fit, 78 of 100.", "Five checks pass. The day rate is not in the ad, so check it before you apply.");
    reveal_command(ui, "/apply https://meridian.example/jobs/42", &mut false);
    ui.add_space(10.0);
    prompt_box_tone(ui, Tone::Closed, |ui| {
        ui.label(rich(ui, "Your applications could not be read.", Type::STRONG, Tone::Closed));
        ui.label("Line 14 has 13 columns instead of 14.");
    });
    ui.add_space(10.0);
    ui.columns_const(|[left, right]| {
        paper(left, true, |ui| {
            stamped_head(
                ui,
                |ui| {
                    day_stamp(ui, "Sat", "31", "Oct", 5, 0, "Saturday 31 October. 0 of 5 decided today.");
                },
                "5 to decide",
                "Start with Meridian Rail. It closes Thursday.",
            );
            deal_footer(ui, |ui| {
                primary_button(ui, "Open the task");
                button(ui, "Later");
            });
        });
        slip(right, |ui| {
            panel_header(ui, "Coming up", Some("2 weeks"));
            panel_body(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                agenda_return(ui, "Sun 1", "1 comes back", "Bring back", "1 set aside, back tomorrow: Juniper Retail. Bring it back now.", true, |ui| {
                    ui.label("Juniper Retail");
                });
                agenda_row(ui, "Tue 3", "Harbor Bank deadline", AgendaEnd::Done, false);
                agenda_row(ui, "Mon 2", "Juniper Retail deadline", AgendaEnd::LateNow("3 days late"), false);
                agenda_row(ui, "Wed 4", "Quarry Labs reply", AgendaEnd::Late("2 days late"), false);
                agenda_row(ui, "Thu 5", "Meridian Rail closes", AgendaEnd::Now, false);
                agenda_row(ui, "Fri 6", "Ardent Software offer reply", AgendaEnd::Nothing, false);
            });
        });
        right.add_space(10.0);
        slip(right, |ui| {
            panel_header(ui, "Done today", None);
            panel_body(ui, |ui| {
                skeleton(ui, 200.0, 18.0);
                skeleton(ui, 140.0, 12.0);
            });
        });
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        skeleton(ui, 68.0, 56.0);
        ui.vertical(|ui| {
            skeleton(ui, 120.0, 12.0);
            skeleton(ui, 260.0, 22.0);
        });
    });
}

/// Two roles as their `###` blocks, the way a profile file holds them.
const ROLE_BLOCKS: [&str; 2] = [
    "### Lead Software Engineer - Calder Freight, Antwerp. Freelance. (2021 – present)\n- Leads a team of six building the route-planning platform for 40 depots.\n- Moved nightly batch planning to event streams, so plans now update within a minute.\n- Budget or headcount you owned: [PLACEHOLDER]",
    "### Senior Backend Engineer - [COMPANY], Ghent (2017 – 2021)\n- Built the appointment service used by 120 clinics, at 99.95% uptime over three years.",
];

/// A role as the Profile screen shows one: its heading cut into title, company and dates by source position, each part
/// with a blank where the block's count found a token, and its body as written.
struct ShownRole {
    title: Vec<Piece>,
    company: Vec<Piece>,
    dates: Vec<Piece>,
    body: String,
}

fn shown_role(block: &str) -> ShownRole {
    let (line, body) = block.split_once('\n').expect("a body");
    let (name, dates) = line[4..].rsplit_once(" (").expect("dates");
    let (title, company) = name.split_once(" - ").expect("a company");
    let part = |part: &str| {
        let start = part.as_ptr() as usize - block.as_ptr() as usize;
        placeholders::shown_pieces(block, start..start + part.len())
    };
    ShownRole { title: part(title), company: part(company), dates: part(dates.trim_end_matches(')')), body: body.to_owned() }
}

const DRAFT: &str = "### Lead Software Engineer - Calder Freight (2021 – present)\n\nAntwerp. Freelance.\n\n- Leads a team of six building the route-planning platform for 40 depots.\n- Budget or headcount you owned: EUR 1.2M a year, eight people at the peak";

const NOW_IN_FILE: [(&str, bool); 2] =
    [("- Budget or headcount you owned: [PLACEHOLDER]", false), ("- Introduced a weekly planning review with depot managers.", true)];

const CHANGED: ChangedOnDisk = ChangedOnDisk {
    title: "This role changed while you were editing.",
    detail: "A task added a line to it. Your text below is not saved yet.",
    caption: "Now in the file, the end of this role",
    lines: Some(&NOW_IN_FILE),
    take: "Use the new version",
    take_note: "Replaces your text with the file's. To keep both, copy the new line into your text, then save.",
};

const BUSY: EditorBusy = EditorBusy {
    why: "A task is running and may change this file. Save once it ends.",
    line: "A task is running, so Save waits until it ends. Your text stays here.",
};

fn editor(busy: bool, changed: bool, unsaved: bool) -> BlockEditor<'static> {
    BlockEditor {
        title: "Edit this role",
        hint: "Markdown. Placeholders read [PLACEHOLDER].",
        save: if changed { "Save mine over it" } else { "Save" },
        note: if changed { "Your text replaces the role as it is now in the file." } else { "Only this role changes. The rest of the file stays as it is." },
        unsaved: unsaved.then_some("Unsaved changes"),
        busy: busy.then_some(BUSY),
        changed: changed.then_some(&CHANGED),
        focus: false,
        reveal: false,
    }
}

fn profile(ui: &mut Ui) {
    let mut cache = CommonMarkCache::default();
    let topics = [
        (1, Some(Ink::Text)),
        (1, Some(Ink::Status(Tone::Interview))),
        (1, Some(Ink::Text)),
        (1, Some(Ink::Status(Tone::Interview))),
        (3, Some(Ink::Text)),
        (1, Some(Ink::Status(Tone::Closed))),
        (1, Some(Ink::Text)),
        (1, Some(Ink::Status(Tone::Interview))),
        (1, Some(Ink::Text)),
    ];
    tally_strip(ui, &topics, true);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        danger_shortcut_button(ui, "Save mine over it", &["Cmd", "Enter"]);
        ui.add_enabled_ui(false, |ui| danger_shortcut_button(ui, "Save mine over it", &["Cmd", "Enter"]));
    });
    ui.add_space(10.0);
    paper(ui, false, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(22, 0)).show(ui, |ui| {
            for (index, role) in ROLE_BLOCKS.map(shown_role).iter().enumerate() {
                let entry = RoleEntry { dates: &role.dates, title: &role.title, company: &role.company, body: &role.body, first: index == 0 };
                role_entry(ui, &mut cache, &entry, (index > 0).then_some("Save or cancel the open edit first."));
            }
            divider(ui);
            ui.add_space(12.0);
            profile_markdown(ui, &mut cache, "Career goals: [ROLE_YOU_WANT_NEXT]. Code such as `cargo test` keeps its own look, and [a link](https://tessel.example/jobs/1) its own.\n\n<!-- A note from the template, kept off the paper. -->");
            ui.add_space(12.0);
            // Busy holds over a change on disk: the box stays, and Save mine over it waits too.
            for (id, busy, changed, unsaved) in [("rest", false, false, true), ("busy", true, false, false), ("changed", false, true, true), ("changed while busy", true, true, true)] {
                ui.push_id(id, |ui| {
                    let mut draft = DRAFT.to_owned();
                    block_editor(ui, &editor(busy, changed, unsaved), &mut draft, &mut false, |_, _| {});
                });
                ui.add_space(12.0);
            }
            ui.push_id("preview", |ui| {
                let mut draft = DRAFT.replace("EUR 1.2M a year, eight people at the peak", "[PLACEHOLDER]");
                block_editor(ui, &editor(false, false, false), &mut draft, &mut true, |ui, draft| profile_markdown(ui, &mut CommonMarkCache::default(), draft));
            });
            ui.add_space(12.0);
        });
    });
}

type Section = (&'static str, fn(&mut Ui));

fn gallery(ui: &mut Ui) {
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let sections_of_gallery: [Section; 14] = [
            ("Palette", swatches),
            ("Type scale", type_scale),
            ("Text", text_styles),
            ("Buttons", buttons),
            ("Shell", shell),
            ("Marks", marks),
            ("Tabs", tabs),
            ("Sections and rows", sections),
            ("Board", board),
            ("Today", tray),
            ("Settings", settings),
            ("Tasks", runs),
            ("Document", document_sample),
            ("Postmark", postmark),
        ];
        for (title, section) in sections_of_gallery {
            ui.heading(title);
            ui.add_space(6.0);
            section(ui);
            ui.add_space(24.0);
        }
    });
}

/// The CV documents in the list column: label, and the tag's word and tone when the document is not current.
const CV_MASTERS: [(&str, Option<(&str, Tone)>); 2] = [("Master CV", None), ("Master CV, engineer", None)];
const CV_APPLICATIONS: [(&str, Option<(&str, Tone)>); 5] = [
    ("Halyard labs tech lead", Some(("Not built yet", Tone::Interview))),
    ("Tessel platform lead", Some(("Source newer", Tone::Interview))),
    ("Brightwell eng manager", Some(("Render failed", Tone::Closed))),
    ("Ardent backend engineer", None),
    ("Granite cloud staff engineer", None),
];

/// The room the list beside the pages takes on the sheet, and the room for the loading pages and the blank page.
const CV_PAPER: [f32; 2] = [952.0, 640.0];
const CV_ROOM: [f32; 2] = [420.0, 320.0];

/// A stand-in for a rendered CV page: white, with a dark name block on the first page and grey lines of text.
fn cv_page(first: bool) -> egui::ColorImage {
    let [width, height] = [210, 297];
    let mut image = egui::ColorImage::filled([width, height], egui::Color32::WHITE);
    let mut bar = |left: usize, top: usize, right: usize, bottom: usize, color: egui::Color32| {
        for y in top..bottom {
            for x in left..right {
                image.pixels[y * width + x] = color;
            }
        }
    };
    let ink = egui::Color32::from_rgb(0x1f, 0x2d, 0x3d);
    let grey = egui::Color32::from_rgb(0xd4, 0xdc, 0xe6);
    let mut top = 24;
    if first {
        bar(20, top, 140, top + 14, ink);
        top += 30;
    }
    for line in 0..16 {
        let y = top + line * 13;
        if line % 5 == 0 {
            bar(20, y, 80, y + 5, ink);
        } else {
            bar(20, y, 190 - (line % 3) * 25, y + 4, grey);
        }
    }
    image
}

fn cv_list(ui: &mut Ui) {
    let rows = |ui: &mut Ui, title: &str, documents: &[(&str, Option<(&str, Tone)>)], picked: Option<usize>| {
        rail_title(ui, title);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, (label, tagged)) in documents.iter().enumerate() {
                pick_row(ui, picked == Some(index), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(rich(ui, *label, Type::BODY, Ink::Text));
                        if let Some((word, tone)) = tagged {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| row_tag(ui, *word, *tone));
                        }
                    });
                });
            }
        });
    };
    ui.add_space(4.0);
    rows(ui, "Masters", &CV_MASTERS, Some(0));
    rows(ui, "For applications", &CV_APPLICATIONS, None);
}

/// The CV documents' widgets: the list beside two fitted pages, then three pages with the third wrapped under the first
/// two, the pages while they render, and the blank page of a `.tex` with no PDF yet.
fn cv_documents(ui: &mut Ui) {
    // The pages load once and are kept, as the screen keeps a document's.
    let id = egui::Id::NULL.with("gallery_cv_pages");
    let pages = ui.ctx().data(|data| data.get_temp::<Vec<egui::TextureHandle>>(id)).unwrap_or_else(|| {
        let loaded: Vec<egui::TextureHandle> =
            (0..3).map(|index| ui.ctx().load_texture(format!("gallery_cv_page_{index}"), cv_page(index == 0), egui::TextureOptions::LINEAR)).collect();
        ui.ctx().data_mut(|data| data.insert_temp(id, loaded.clone()));
        loaded
    });
    ui.allocate_ui(egui::Vec2::from(CV_PAPER), |ui| {
        split_paper(
            ui,
            RUN_SIDE,
            cv_list,
            |ui| {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(22, 16)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(rich(ui, "Master CV", Type::DEAL, Ink::Text));
                    ui.horizontal(|ui| {
                        faint_mono(ui, "main_example.pdf");
                        muted(ui, "2 pages, 84 KB, built 3 days ago");
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        tag(ui, "Source newer", Tone::Interview);
                        muted(ui, "Its .tex changed today.");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            small_command_action_hinted(
                                ui,
                                "Rebuild the PDF…",
                                "/build-cv main_example",
                                "Opens the launcher with /build-cv main_example. Nothing runs until you start it there.",
                            );
                            enabled_when(ui, false, "Another run is active. One run at a time keeps your files safe.", |ui| {
                                small_command_action_hinted(ui, "Build the PDF…", "/build-cv main_example", "Opens the launcher.")
                            });
                        });
                    });
                });
                divider(ui);
                ScrollArea::vertical().id_salt("cv_pages").auto_shrink([false, false]).show(ui, |ui| {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(22, 16)).show(ui, |ui| {
                        fitted_pages(ui, PageSet::Ready { pages: &pages[..2], of: 2, alt: "Master CV" });
                    });
                });
            },
            |ui| {
                desk_label(ui, "Kept in ", Type::NOTE, Ink::Muted);
                faint_mono(ui, "cv/");
                desk_label(ui, ", beside the .tex each PDF is built from", Type::NOTE, Ink::Muted);
            },
        );
    });
    ui.add_space(16.0);
    paper(ui, false, |ui| {
        egui::Frame::new().inner_margin(16).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(24.0, 24.0);
                ui.allocate_ui(egui::Vec2::from(CV_ROOM), |ui| fitted_pages(ui, PageSet::Ready { pages: &pages, of: pages.len(), alt: "Master CV" }));
                ui.vertical(|ui| {
                    ui.allocate_ui(egui::Vec2::from(CV_ROOM), |ui| fitted_pages(ui, PageSet::Loading(2)));
                    ui.allocate_ui(egui::Vec2::from(CV_ROOM), |ui| blank_page(ui, "No PDF yet. It shows here once /build-cv has built it."));
                });
            });
        });
    });
}

fn profile_gallery(ui: &mut Ui) {
    ui.set_width(ui.available_width());
    ui.heading("Profile");
    ui.add_space(6.0);
    profile(ui);
    ui.add_space(24.0);
    ui.heading("CV documents");
    ui.add_space(6.0);
    cv_documents(ui);
}

/// The height of the Profile sheet. The main sheet is near the 8192px a texture holds, so these parts take a sheet of their own.
const PROFILE_HEIGHT: f32 = 3560.0;

fn render_profile(theme: Theme, name: &str) {
    let mut harness =
        with_renderer(Harness::builder().with_size([1000.0, PROFILE_HEIGHT]).with_pixels_per_point(1.0).with_theme(theme).with_context_setup(install_look))
            .build_ui(|ui| {
                jobdesk::ui_kit::desk(ui, |ui| jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, profile_gallery));
            });
    harness.run_steps(3);
    compare(&mut harness, name);
}

#[test]
fn ui_kit_gallery_profile_dark() {
    render_profile(Theme::Dark, "ui_kit_gallery_profile_dark");
}

#[test]
fn ui_kit_gallery_profile_light() {
    render_profile(Theme::Light, "ui_kit_gallery_profile_light");
}

fn render(theme: Theme, name: &str) {
    // One point per pixel: at more, the sheet is taller than the 8192px a texture holds.
    let mut harness = with_renderer(Harness::builder().with_size(SHEET).with_pixels_per_point(1.0).with_theme(theme).with_context_setup(install_look))
        .build_ui(|ui| {
            jobdesk::ui_kit::desk(ui, |ui| jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, gallery));
        });
    harness.run_steps(3);
    // The pointer rests on one row, so the sheet shows hover beside picked.
    harness.get_by_label(HOVERED_ROW).hover();
    harness.run_steps(2);
    compare(&mut harness, name);
}

/// The usage sheet's size: the main sheet is already as tall as a texture holds.
const USAGE_SHEET: [f32; 2] = [1000.0, 900.0];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
/// The year shown: 53 weeks from Monday 27 October to Saturday 31 October a year on, which is today.
const YEAR_WEEKS: usize = 53;
const TODAY: (usize, usize) = (52, 5);
/// The tip the pointer rests on in the snapshots.
const HOVERED_DAY: (usize, usize) = (50, 1);

/// A year's weeks of days, and the week each month's name stands over.
type Year = (Vec<[Option<HeatDay>; 7]>, Vec<(usize, &'static str)>);

/// A fictional year of tasks, every day from the first Monday to today. `blank` has no task at all.
fn year(blank: bool) -> Year {
    // Month lengths from October to October, so the day and month of each square can be named.
    let lengths = [31, 30, 31, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31];
    let mut dates = Vec::new();
    for (index, length) in lengths.into_iter().enumerate() {
        dates.extend((1..=length).map(|day| (day, (index + 9) % 12)));
    }
    let start = 26;
    let mut months = Vec::new();
    let weeks = (0..YEAR_WEEKS)
        .map(|week| {
            let (first, month) = dates[start + week * 7];
            if first <= 7 && week > 0 {
                months.push((week, MONTHS[month]));
            }
            std::array::from_fn(|weekday| {
                if (week, weekday) > TODAY {
                    return None;
                }
                let (day, month) = dates[start + week * 7 + weekday];
                let seed = (day * 7 + month * 13) * 2_654_435_761 % 97;
                let tasks = match () {
                    _ if blank || week < 37 => 0,
                    _ if weekday >= 5 => usize::from(seed > 77),
                    _ if seed > 87 => 4,
                    _ if seed > 60 => 2,
                    _ if seed > 29 => 1,
                    _ => 0,
                };
                let tasks = if !blank && (week, weekday) == HOVERED_DAY { 4 } else { tasks };
                let step = match tasks {
                    0 => 0,
                    1 => 1,
                    2 | 3 => 2,
                    _ => 3,
                };
                let what = match tasks {
                    0 => "no task".to_owned(),
                    1 => "1 task, $0.80".to_owned(),
                    tasks => format!("{tasks} tasks, ${}.10", tasks + 2),
                };
                Some(HeatDay { step, tip: format!("{} {day} {}: {what}", WEEKDAYS[weekday], MONTHS[month]) })
            })
        })
        .collect();
    (weeks, months)
}

const LEGEND: [&str; 4] = ["None", "1 task", "2 or 3 tasks", "4 or more"];

fn usage(ui: &mut Ui) {
    let figures = |bar: bool| {
        [
            Figure {
                label: "Spent",
                figure: "$41.20".to_owned(),
                note: if bar { "of your $60.00 monthly cap" } else { "in October" }.to_owned(),
                bar: bar.then_some((41.2 / 60.0, Ink::Text)),
            },
            Figure { label: "Tokens", figure: "4.1M".to_owned(), note: "in, and 0.6M out".to_owned(), bar: None },
            Figure { label: "Cache hits", figure: "82%".to_owned(), note: "of input read from cache, which costs less".to_owned(), bar: None },
            Figure { label: "Tasks", figure: "37".to_owned(), note: "33 finished, 3 stopped, 1 failed".to_owned(), bar: None },
        ]
    };
    figure_band(ui, &figures(true));
    ui.add_space(12.0);
    figure_band(ui, &figures(false));
    ui.add_space(12.0);
    figure_band_loading(ui, 4);
    ui.add_space(20.0);
    let (weeks, months) = year(false);
    let notes = ["Last 12 months, one square a day", "75 days with a task, since 13 July"];
    let map = DayHeatmap { weeks: &weeks, today: Some(TODAY), months: &months, notes: &notes, legend: LEGEND, faded: false };
    let shown = day_heatmap(ui, &map);
    ui.data_mut(|data| data.insert_temp(egui::Id::NULL.with(HOVERED_DAY), shown.cells[HOVERED_DAY.0][HOVERED_DAY.1]));
    ui.add_space(20.0);
    let (weeks, months) = year(true);
    let notes = ["Last 12 months, one square a day", "No task yet"];
    day_heatmap(ui, &DayHeatmap { weeks: &weeks, today: Some(TODAY), months: &months, notes: &notes, legend: LEGEND, faded: true });
    ui.add_space(20.0);
    day_heatmap_loading(ui, YEAR_WEEKS);
}

/// The usage widgets on a page of paper, as Settings shows them.
fn usage_sheet(theme: Theme, name: &str) {
    let mut harness = with_renderer(Harness::builder().with_size(USAGE_SHEET).with_pixels_per_point(1.0).with_theme(theme).with_context_setup(install_look))
        .build_ui(|ui| {
            jobdesk::ui_kit::desk(ui, |ui| {
                jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, |ui| {
                    paper(ui, false, |ui| {
                        egui::Frame::new().inner_margin(22).show(ui, usage);
                    });
                });
            });
        });
    harness.run_steps(3);
    // The pointer rests on one day, so the sheet shows its tip.
    let day: egui::Rect = harness.ctx.data(|data| data.get_temp(egui::Id::NULL.with(HOVERED_DAY))).expect("the hovered day was drawn");
    harness.hover_at(day.center());
    harness.run_steps(4);
    compare(&mut harness, name);
}

/// The live sheet's size: the main sheet is already as tall as a texture holds.
const LIVE_SHEET: [f32; 2] = [1000.0, 1270.0];
/// The width of a key frame of the sweep.
const SWEEP_FRAME: f32 = 130.0;

/// One band of a live run, with the sweep in `form` at `seconds` on the Whose turn cell. `reduced` stills it as Reduce motion does.
fn live_band(ui: &mut Ui, title: &str, form: SweepForm, seconds: f64, reduced: bool, who: &str, next: (&str, &str)) {
    desk_label(ui, title, Type::NOTE, Ink::Muted);
    let working = [(2, Some(Ink::Text)), (1, Some(Ink::Status(Tone::Active))), (4, None)];
    set_reduced_motion(ui.ctx(), reduced);
    turn_band_live(
        ui,
        &[
            BandCell { label: "Where", value: "2 Draft", note: "2 of 7 steps done", look: CellLook::Plain, marks: &working },
            BandCell { label: "Whose turn", value: "Claude's", note: who, look: CellLook::Ink(Tone::Active), marks: &[] },
            BandCell { label: "What next", value: next.0, note: next.1, look: CellLook::Plain, marks: &[] },
        ],
        false,
        "Where the run is",
        "2 of 7 steps done",
        Some((1, form, seconds)),
    );
    set_reduced_motion(ui.ctx(), false);
    ui.add_space(14.0);
}

/// Every form of a live run's sweep at a fixed time, its key frames, and the estimate note under Cost.
fn live(ui: &mut Ui) {
    let leave = ("Nothing for now", "You can leave. The Tasks tab marks it when it needs you.");
    live_band(ui, "Working, 1.2 s into a pass", SweepForm::Working, 1.2, false, "Last update 3s ago", leave);
    desk_label(ui, "Working, its key frames, then at rest", Type::NOTE, Ink::Muted);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        for seconds in [0.4, 0.8, 1.2, 1.6, 2.0, 2.6] {
            ui.vertical(|ui| {
                let (edge, _) = ui.allocate_exact_size(egui::vec2(SWEEP_FRAME, MARK_RULE), egui::Sense::hover());
                // The track shows here only to place the segment.
                ui.painter().rect_filled(edge, 0.0, Palette::of(ui).off);
                live_sweep(ui, edge, SweepForm::Working, seconds);
                desk_label(ui, format!("{seconds} s"), Type::SMALL, Ink::Muted);
            });
        }
    });
    ui.add_space(14.0);
    live_band(ui, "Quiet, 30 s to 2 min: half speed, 2.4 s into a pass", SweepForm::Quiet, 2.4, false, "No update for 48s", leave);
    let quiet = ("Nothing for now", "Long writes can be quiet. Stop run keeps the files so far.");
    live_band(ui, "Long quiet, 2 min and more", SweepForm::LongQuiet, 1.2, false, "No update for 3 min", quiet);
    live_band(ui, "Stopping", SweepForm::Stopping, 1.2, false, "Stopping", ("Nothing for now", ""));
    live_band(ui, "Reduced motion, working", SweepForm::Working, 1.2, true, "Last update 3s ago", leave);
    live_band(ui, "Reduced motion, quiet", SweepForm::Quiet, 2.4, true, "No update for 48s", leave);
    live_band(ui, "Reduced motion, long quiet", SweepForm::LongQuiet, 1.2, true, "No update for 3 min", quiet);
    ui.allocate_ui(egui::vec2(RUN_SIDE, 200.0), |ui| {
        slip(ui, |ui| {
            panel_header(ui, "This run", None);
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = ROW_GAP;
                ui.horizontal(|ui| {
                    muted(ui, "Cost");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| desk_label(ui, "About $1.24 of $10.00", Type::BODY, Ink::Text));
                });
                slip_bar(ui, 0.124, Ink::Text);
                estimate_note(ui, "Estimated until Claude's turn ends.");
            });
        });
    });
}

fn live_sheet(theme: Theme, name: &str) {
    let mut harness = with_renderer(Harness::builder().with_size(LIVE_SHEET).with_pixels_per_point(1.0).with_theme(theme).with_context_setup(install_look))
        .build_ui(|ui| {
            jobdesk::ui_kit::desk(ui, |ui| jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, live));
        });
    harness.run_steps(3);
    compare(&mut harness, name);
}

#[test]
fn ui_kit_gallery_live_dark() {
    live_sheet(Theme::Dark, "ui_kit_gallery_live_dark");
}

#[test]
fn ui_kit_gallery_live_light() {
    live_sheet(Theme::Light, "ui_kit_gallery_live_light");
}

/// The wizard sheet's size: a card at the first window's size, two narrower ones, then the loose parts.
const WIZARD_SHEET: [f32; 2] = [1000.0, 1520.0];
/// The first window, 720x560, less its brand row and its status strip.
const WIZARD_ROOM: [f32; 2] = [720.0, 470.0];
/// Half the sheet's width, for two cards side by side.
const WIZARD_HALF: [f32; 2] = [468.0, 500.0];
/// The widest a card gets in the first window.
const WIZARD_CARD: f32 = 640.0;
const WIZARD_STEPS: [&str; 4] = ["Folder", "Computer", "Profile", "Done"];

/// A helper's line, with nothing under its title but its name.
const fn helper(knot: &'static str, state: KnotState, title: &'static str, under: &'static str, end: LineEnd<'static>) -> SetupLine<'static> {
    SetupLine { knot, state, title, under, end, bar: None, live: None }
}

const PYTHON: SetupLine = helper("1", KnotState::Done, "Keeps your tracker", "Python", LineEnd::Tag("Already here", Tone::Hired));
const BUN: SetupLine = helper("2", KnotState::Done, "Searches job sites", "Bun", LineEnd::Tag("Installed", Tone::Hired));
const LATEX: SetupLine = SetupLine {
    bar: Some((0.35, "About 8 minutes left")),
    ..helper(
        "3",
        KnotState::Working,
        "Makes your CV and cover letter PDFs",
        "LaTeX, about 250 MB, the slow one: 5 to 15 minutes",
        LineEnd::Tag("Installing…", Tone::Active),
    )
};

/// One wizard card in a room the size of `room`.
fn wizard_in(ui: &mut Ui, room: [f32; 2], step: &WizardStep, body: impl FnOnce(&mut Ui), actions: impl FnOnce(&mut Ui), why: &str) {
    ui.allocate_ui_with_layout(egui::vec2(room[0], room[1]), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_min_size(egui::vec2(room[0], room[1]));
        wizard_card(ui, step, WIZARD_CARD, body, actions, why);
    });
}

/// The first-run wizard's parts: three cards, each with its foot, then step dots, setup stamps and choice cards.
fn wizard(ui: &mut Ui) {
    desk_label(ui, "Wizard card at 720x560: lines with a bar, a disabled primary, Cancel and when it opens", Type::NOTE, Ink::Muted);
    let installing = WizardStep {
        steps: &WIZARD_STEPS,
        at: 1,
        title: "Get this computer ready",
        lead: "Installing what was missing. LaTeX is the slow one: you can step away and come back.",
    };
    wizard_in(
        ui,
        WIZARD_ROOM,
        &installing,
        |ui| {
            paper(ui, false, |ui| {
                setup_line(ui, &PYTHON, true);
                setup_line(ui, &BUN, false);
                setup_line(ui, &LATEX, false);
            });
            desk_label(ui, "Each download is compared with Jobdesk's own record of it before it is installed.", Type::NOTE, Ink::Muted);
        },
        |ui| {
            enabled_when(ui, false, "Continue opens when all three are ready.", |ui| primary_button(ui, "Continue"));
            button(ui, "Cancel");
        },
        "Continue opens when all three are ready.",
    );
    ui.add_space(14.0);
    desk_label(ui, "A failed line, an alert and a quiet link; a live line, a line ahead and a slip on the card", Type::NOTE, Ink::Muted);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        let download = WizardStep { steps: &WIZARD_STEPS, at: 0, title: "Download ai-job-search", lead: "Into the new Job search folder." };
        wizard_in(
            ui,
            WIZARD_HALF,
            &download,
            |ui| {
                paper(ui, false, |ui| {
                    setup_line(ui, &helper("1", KnotState::Done, "Folder made", "", LineEnd::Nothing), true);
                    let failed =
                        helper("2", KnotState::Failed, "Checking the download", "It does not match version 1.7.5.", LineEnd::Tag("No match", Tone::Closed));
                    setup_line(ui, &failed, false);
                    setup_line(ui, &helper("3", KnotState::Ahead, "Unpacking it into your folder", "", LineEnd::Note("Next")), false);
                });
                alert_slip(ui, "This download doesn't match. Nothing was saved.", "Your folder is still empty.");
            },
            |ui| {
                primary_button(ui, "Try again");
                link(ui, "Get help");
            },
            "",
        );
        let reading = WizardStep { steps: &WIZARD_STEPS, at: 2, title: "Building your profile", lead: "This can take a few minutes." };
        wizard_in(
            ui,
            WIZARD_HALF,
            &reading,
            |ui| {
                paper(ui, false, |ui| {
                    setup_line(ui, &helper("1", KnotState::Done, "Documents copied into your folder", "", LineEnd::Nothing), true);
                    let live = SetupLine {
                        live: Some((SweepForm::Working, 1.2)),
                        ..helper("2", KnotState::Working, "Reading your documents", "Last update 4s ago", LineEnd::Nothing)
                    };
                    setup_line(ui, &live, false);
                    setup_line(ui, &helper("3", KnotState::Ahead, "Writing your profile", "", LineEnd::Note("Next")), false);
                });
                slip(ui, |ui| {
                    panel_header(ui, "So far", None);
                    panel_body(ui, |ui| {
                        muted(ui, "No rounds answered yet.");
                    });
                });
            },
            |ui| {
                button(ui, "Stop setup");
            },
            "Elapsed 18:40",
        );
    });
    ui.add_space(14.0);
    desk_label(ui, "Step dots at each step", Type::NOTE, Ink::Muted);
    Grid::new("wizard_dots").spacing([24.0, 10.0]).show(ui, |ui| {
        for at in 0..WIZARD_STEPS.len() {
            step_dots(ui, &WIZARD_STEPS, at);
            if at % 2 == 1 {
                ui.end_row();
            }
        }
    });
    ui.add_space(14.0);
    desk_label(ui, "The setup stamp at each step, then the date on the welcome", Type::NOTE, Ink::Muted);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        for at in 0..WIZARD_STEPS.len() {
            setup_stamp(ui, at, WIZARD_STEPS.len());
        }
        day_stamp(ui, "Tue", "10", "Nov", 0, 0, "Tuesday 10 November");
    });
    ui.add_space(14.0);
    desk_label(ui, "Choice cards: picked with a tag, then not picked", Type::NOTE, Ink::Muted);
    ui.allocate_ui(egui::vec2(656.0, 200.0), |ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        choice_card(
            ui,
            true,
            "Set up from scratch",
            Some(("Recommended", Tone::Plain)),
            "Jobdesk makes your job search folder, gets this computer ready and builds your profile from your CV. About 40 minutes, most of it answering Claude's questions about you.",
        );
        choice_card(ui, false, "I already have an ai-job-search folder", None, "Choose it, and Jobdesk opens it as it is.");
    });
}

fn wizard_sheet(theme: Theme, name: &str) {
    let mut harness = with_renderer(Harness::builder().with_size(WIZARD_SHEET).with_pixels_per_point(1.0).with_theme(theme).with_context_setup(install_look))
        .build_ui(|ui| {
            jobdesk::ui_kit::desk(ui, |ui| jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, wizard));
        });
    harness.run_steps(3);
    compare(&mut harness, name);
}

#[test]
fn ui_kit_gallery_wizard_dark() {
    wizard_sheet(Theme::Dark, "ui_kit_gallery_wizard_dark");
}

#[test]
fn ui_kit_gallery_wizard_light() {
    wizard_sheet(Theme::Light, "ui_kit_gallery_wizard_light");
}

/// The Profile chapter's room: the window at the app's size, 1280x820, less its brand row.
const SETUP_ROOM: [f32; 2] = [1280.0, 778.0];
/// A Profile chapter card with little in it, such as the documents step.
const SETUP_SHORT: [f32; 2] = [1280.0, 470.0];
/// The documents step with three files listed, tall enough that its main column needs no scroll.
const SETUP_DROPPED: [f32; 2] = [1280.0, 530.0];
const SETUP_SHEET: [f32; 2] = [1328.0, 4390.0];

/// Claude's second round of questions: a lead, then 13 questions in five groups, question 3 with two sub-items.
const SETUP_QUESTIONS: [SheetPart<'static>; 19] = [
    SheetPart::Prose(
        "## Questions on what's missing\n\nYour profile now has the changes you approved. A few things your documents don't say yet. Answer in any order. Short answers are fine, and you can skip any you like.",
    ),
    SheetPart::Group("Career direction"),
    question(1, "What kind of role do you want next, and what would you rather leave behind?"),
    question(2, "Would you take a step into leading a small team, or stay hands-on for now?"),
    SheetPart::Group("How you work"),
    SheetPart::Question(Question {
        number: 3,
        text: "How would the people you worked with describe you, in a sentence each?",
        sub_items: &[SubItem { letter: 'a', text: "A former manager" }, SubItem { letter: 'b', text: "A designer on your team" }],
        optional: false,
    }),
    optional(4, "Which kind of team brings out your best? (optional)"),
    SheetPart::Group("Constraints"),
    question(5, "Which cities or regions work for you, and how many office days a week?"),
    optional(6, "Is there a salary range you want Jobdesk to filter by? (optional)"),
    question(7, "Is there a notice period or a start date to plan around?"),
    SheetPart::Group("Numbers for stronger CVs"),
    question(8, "For the checkout redesign at **Tessel Group**, do you know the effect on support tickets or sales?"),
    optional(9, "Roughly how many designers used the design system you built? (optional)"),
    optional(10, "Any talk, award or article worth naming? (optional)"),
    SheetPart::Group("Search setup"),
    question(11, "Which job sites do you use today, besides LinkedIn?"),
    question(12, "Which language should your CVs be in: English, German, or both?"),
    question(13, "Which job titles should Jobdesk search for? Three to eight is a good number."),
];

const fn question(number: u32, text: &'static str) -> SheetPart<'static> {
    SheetPart::Question(Question { number, text, sub_items: &[], optional: false })
}

const fn optional(number: u32, text: &'static str) -> SheetPart<'static> {
    SheetPart::Question(Question { number, text, sub_items: &[], optional: true })
}

/// The answers typed so far, one per question: 8 of 13.
const SETUP_ANSWERS: [&str; 13] = [
    "Senior product designer in a product company. Less agency work, more time with users.",
    "",
    "a. Calm and careful. b. Quick to test an idea with real users.",
    "",
    "Berlin, or remote in Germany. Two office days at most.",
    "",
    "Three months' notice.",
    "Support tickets went down about a third after launch.",
    "",
    "",
    "LinkedIn, and a local board for designers.",
    "English, and German when the ad is in German.",
    "Product designer, UX designer, Service designer",
];

/// Claude's proposal: 16 numbered changes in five groups, between its lead and its closing line.
const SETUP_PROPOSAL: [ProposalPart<'static>; 23] = [
    ProposalPart::Prose("## Proposed additive changes\n\nHere is what I would add to your profile. Nothing is removed unless you asked for it."),
    ProposalPart::Group("Profile summary"),
    change(1, "Lead with product design for payments and banking apps, which both documents stress."),
    change(2, "Use the title from your CV, Senior Product Designer, not the shorter one from LinkedIn."),
    change(3, "Add the years from your LinkedIn profile: 2016 to now."),
    ProposalPart::Group("Experience"),
    change(4, "Tessel Group: add the checkout redesign and the design system, from the reference letter."),
    change(5, "Harbor Studio: use the end date from your CV, March 2021, as you confirmed."),
    change(6, "Leave out the 2014 internship, as you asked."),
    change(7, "Lumen Telecom: add the research work listed only on LinkedIn."),
    change(8, "Group the two freelance years under one heading."),
    ProposalPart::Group("Skills and languages"),
    change(9, "Add usability testing, service blueprints and accessibility reviews."),
    change(10, "German at B2, as you confirmed. English stays fluent."),
    change(11, "Drop two tools your CV lists but LinkedIn no longer does."),
    change(12, "List prototyping tools in one line, not three."),
    ProposalPart::Group("Education"),
    change(13, "BA in Communication Design, with the year from your diploma."),
    change(14, "Add the accessibility course certificate."),
    ProposalPart::Group("Writing style"),
    change(15, "Keep cover letters short: one page, plain words."),
    change(16, "Write in the first person, without buzzwords."),
    ProposalPart::Prose("Reply yes to apply all, or list the numbers to skip."),
];

const fn change(number: u32, text: &'static str) -> ProposalPart<'static> {
    ProposalPart::Change(Change { number, text })
}

/// The change the pointer and the focus rest on in the snapshots, unticked as 3 is.
const FOCUSED_CHANGE: &str = "7. Lumen Telecom: add the research work listed only on LinkedIn.";

/// Claude's message with no numbered questions, for the fallback sheet.
const SETUP_FALLBACK: &str = "## A few more things\n\nThanks, that fills most of the gaps. Before I write the search setup, I'd like to know a bit more about where you want to work.\n\n- Which cities or regions work for you, and how many office days a week?\n- Is there a notice period or a start date to plan around?\n- Which job sites do you use today, besides LinkedIn?\n\nAny detail helps, but short answers are fine.";

const SETUP_STEPS: [&str; 4] = ["Folder", "Computer", "Profile", "Done"];

/// What the user typed and ticked on the setup sheet, kept across frames as a screen keeps it.
struct SetupBench {
    cache: CommonMarkCache,
    answers: Vec<String>,
    ticked: Vec<bool>,
    fallback: String,
}

impl SetupBench {
    fn new() -> Self {
        let mut ticked = vec![true; 16];
        ticked[2] = false;
        ticked[6] = false;
        Self {
            cache: CommonMarkCache::default(),
            answers: SETUP_ANSWERS.iter().map(|answer| (*answer).to_owned()).collect(),
            ticked,
            fallback: "Berlin, or remote in Germany. Two office days at most.\n\nThree months' notice.\n\nLinkedIn, and a local board for designers."
                .to_owned(),
        }
    }
}

/// One Profile chapter card in a room the size of `room`.
fn setup_in(ui: &mut Ui, room: [f32; 2], add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(egui::vec2(room[0], room[1]), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_min_size(egui::vec2(room[0], room[1]));
        add(ui);
    });
}

/// The steps a Profile chapter card stands at, under its title and lead.
fn profile_step(title: &'static str, lead: &'static str) -> WizardStep<'static> {
    WizardStep { steps: &SETUP_STEPS, at: 2, title, lead }
}

/// The Profile chapter's parts: the question sheet, the review card, the fallback sheet, the documents step before and
/// after files are added, and the CV check, each in its card.
fn setup(ui: &mut Ui, bench: &mut SetupBench) {
    desk_label(
        ui,
        "Split card at 1280x820: the question sheet, with a field under each question and sub-items inside 3, and the answers slip",
        Type::NOTE,
        Ink::Muted,
    );
    setup_in(ui, SETUP_ROOM, |ui| {
        let step = profile_step("Claude has 13 questions", "Writing your profile. Your second round of answers.");
        let answered: Vec<bool> = bench.answers.iter().map(|answer| !answer.trim().is_empty()).collect();
        split_card(
            ui,
            &step,
            |ui| question_sheet(ui, &mut bench.cache, &SETUP_QUESTIONS, &mut bench.answers),
            |ui| {
                answers_slip(ui, &AnswersSlip { answered: &answered, saved_at: Some("14:32"), short: false });
            },
            |ui| {
                primary_shortcut_button(ui, "Send answers", &command_chord("Enter"));
                button(ui, "Stop setup");
            },
            "Elapsed 18:40",
        );
    });
    ui.add_space(14.0);
    desk_label(ui, "The review card: a change_check per change, 3 and 7 unticked and 7 focused, then the review slip and its reply", Type::NOTE, Ink::Muted);
    setup_in(ui, SETUP_ROOM, |ui| {
        let step = profile_step("Claude proposes changes to your profile", "From your documents and your first answers. Nothing changes until you say so.");
        let numbers: Vec<(u32, bool)> = (1..=16).zip(bench.ticked.iter().copied()).collect();
        split_card(
            ui,
            &step,
            |ui| review_changes(ui, &mut bench.cache, &SETUP_PROPOSAL, &mut bench.ticked),
            |ui| {
                review_slip(ui, &numbers, "Apply all except 3 and 7.");
                side_slip(ui, "", "", |ui| slip_text(ui, "", "Something to fix? Change something opens a field. Say what to change in your own words."));
            },
            |ui| {
                primary_button(ui, "Apply these changes");
                button(ui, "Change something");
            },
            "Elapsed 11:05",
        );
    });
    ui.add_space(14.0);
    desk_label(ui, "The fallback sheet in a wide card: Claude's message beside one large field", Type::NOTE, Ink::Muted);
    setup_in(ui, SETUP_ROOM, |ui| {
        let step = profile_step("Claude has a few questions", "Writing your profile. Your third round of answers.");
        wide_card(
            ui,
            &step,
            |ui| {
                question_fallback(
                    ui,
                    &mut bench.cache,
                    SETUP_FALLBACK,
                    &mut bench.fallback,
                    "Claude asked in its own words this time, so your answer is one reply. Answer in any order; short answers are fine.\nDraft saved as you type, last at 14:51. You can close Jobdesk and come back.",
                )
            },
            |ui| {
                primary_shortcut_button(ui, "Send answer", &command_chord("Enter"));
                button(ui, "Stop setup");
            },
            "Elapsed 24:12",
        );
    });
    ui.add_space(14.0);
    let next = |ui: &mut Ui| {
        side_slip(ui, "What comes next", "", |ui| {
            slip_text(ui, "", "Claude reads your documents, then writes to you in a few rounds of questions. Answer them on your own time, in any order.");
            slip_text(ui, "", "Setup took one tester about 40 minutes, most of it writing answers. Your answers are saved as you type.");
        });
    };
    let lead = "Add your CV and anything else that says what you have done. Claude reads it all, then asks about what is missing.";
    let copied = |ui: &mut Ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            desk_label(ui, "Jobdesk copies them into the documents folder inside ", Type::NOTE, Ink::Muted);
            path_label(ui, "Documents/", "Job search");
            desk_label(ui, ". The originals stay where they are.", Type::NOTE, Ink::Muted);
        });
    };
    desk_label(ui, "The documents step: the full drop zone, then the file list over the slim zone", Type::NOTE, Ink::Muted);
    setup_in(ui, SETUP_SHORT, |ui| {
        split_card(
            ui,
            &profile_step("Tell Jobdesk about you", lead),
            |ui| {
                drop_zone(
                    ui,
                    DropForm::Full {
                        title: "Drop your documents here",
                        sentence: "Your CV, and any of these: a LinkedIn profile saved as a PDF, diplomas, reference letters. PDF or Word files.",
                    },
                );
                copied(ui);
            },
            next,
            |ui| {
                enabled_when(ui, false, "Add at least your CV first.", |ui| primary_button(ui, "Read my documents"));
                button(ui, "Back");
            },
            "Add at least your CV first.",
        );
    });
    ui.add_space(14.0);
    // Its own id, as the screen's next step would have, so the two cards of one title keep their own scroll.
    ui.push_id("dropped", |ui| {
        setup_in(ui, SETUP_DROPPED, |ui| {
            split_card(
                ui,
                &profile_step("Tell Jobdesk about you", lead),
                |ui| {
                    file_list(
                        ui,
                        &[
                            AddedFile { name: "Sample_Candidate_CV.pdf", pages: Some(2), kind: 0, copied: false },
                            AddedFile { name: "Profile.pdf", pages: Some(4), kind: 1, copied: false },
                            AddedFile { name: "Reference_letter_Tessel_Group_2023_signed_and_scanned_copy_final.pdf", pages: Some(1), kind: 3, copied: false },
                        ],
                        &["CV", "LinkedIn profile", "Diploma", "Reference letter", "Project"],
                    );
                    drop_zone(ui, DropForm::Slim { sentence: "Drop more here: diplomas, reference letters." });
                    copied(ui);
                },
                next,
                |ui| {
                    primary_button(ui, "Read my documents");
                    button(ui, "Back");
                },
                "3 documents added",
            );
        })
    });
    ui.add_space(14.0);
    desk_label(ui, "The CV check: the CV's pages, its slip with Open PDF, and the reply Looks good sends", Type::NOTE, Ink::Muted);
    // The pages load once and are kept, as the screen keeps a document's.
    let id = egui::Id::NULL.with("gallery_setup_cv_pages");
    let pages = ui.ctx().data(|data| data.get_temp::<Vec<egui::TextureHandle>>(id)).unwrap_or_else(|| {
        let loaded: Vec<egui::TextureHandle> =
            (0..2).map(|index| ui.ctx().load_texture(format!("gallery_setup_cv_page_{index}"), cv_page(index == 0), egui::TextureOptions::LINEAR)).collect();
        ui.ctx().data_mut(|data| data.insert_temp(id, loaded.clone()));
        loaded
    });
    setup_in(ui, [1280.0, 640.0], |ui| {
        split_card(
            ui,
            &profile_step("Check your CV", "Claude made it from your profile. Have a look before setup finishes."),
            |ui| cv_check(ui, PageSet::Ready { pages: &pages, of: 2, alt: "Sample_Candidate_CV" }),
            |ui| {
                cv_slip(ui, "Sample_Candidate_CV", "English, 2 pages. Saved in the cv folder of your job search folder.");
                side_slip(ui, "", "", |ui| {
                    slip_text(ui, "", "Spotted something? Ask for a change opens a field. Say it in your own words, such as \"Put the design system first\".")
                });
                reply_slip(ui, "Looks good sends Claude this reply:", "The CV looks good.");
            },
            |ui| {
                primary_button(ui, "Looks good");
                button(ui, "Ask for a change");
            },
            "Elapsed 31:12",
        );
    });
    ui.add_space(14.0);
    desk_label(ui, "The short answers slip of a round of follow-ups", Type::NOTE, Ink::Muted);
    ui.allocate_ui(egui::vec2(340.0, 120.0), |ui| {
        paper(ui, false, |ui| {
            egui::Frame::new().inner_margin(12).show(ui, |ui| {
                answers_slip(ui, &AnswersSlip { answered: &[true, true], saved_at: None, short: true });
            });
        });
    });
}

fn setup_sheet(theme: Theme, name: &str) {
    let mut bench = SetupBench::new();
    let mut harness = with_renderer(Harness::builder().with_size(SETUP_SHEET).with_pixels_per_point(1.0).with_theme(theme).with_context_setup(install_look))
        .build_ui(move |ui| {
            jobdesk::ui_kit::desk(ui, |ui| jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, |ui| setup(ui, &mut bench)));
        });
    harness.run_steps(3);
    // The focus rests on an unticked change, so the sheet shows its ring round the box.
    harness.get_by_label(FOCUSED_CHANGE).focus();
    harness.run_steps(2);
    compare(&mut harness, name);
}

#[test]
fn ui_kit_gallery_setup_dark() {
    setup_sheet(Theme::Dark, "ui_kit_gallery_setup_dark");
}

#[test]
fn ui_kit_gallery_setup_light() {
    setup_sheet(Theme::Light, "ui_kit_gallery_setup_light");
}

/// The settings sheet's size: room for every row and the open popover under the last one.
const SETTINGS_SHEET: [f32; 2] = [1000.0, 1730.0];
const CONFIRM_WIDTH: f32 = 340.0;
/// An obviously fake key for the secret fields.
const FAKE_KEY: &str = "sk-ant-fake-gallery-key-0000";
const SECRET_WIDTH: f32 = 360.0;
const CLEAR: Confirm = Confirm {
    question: "Clear 214 tray decisions?",
    note: "Today deals every card again, the ones you skipped too. This cannot be undone.",
    keep: "Keep them",
    act: "Clear decisions",
    esc_note: "Esc keeps them.",
};

/// The Settings controls that pick, hold an amount or ask first, on paper as Settings shows them. `confirm` is the
/// popover's state: `Some(true)` opens it with Keep them about to take the focus.
fn settings_widgets(ui: &mut Ui, confirm: &mut Option<bool>) {
    let note = "Jobs checks postings against these. The floor and the commute tiers come from your profile, and Jobdesk never changes them.";
    ruled_note_with_command(ui, note, "/setup --section goals", &mut true, "");
    setting_section(ui, "Tool permissions", Some("What happens when Claude wants to read, write or run something during a task."), "");
    let allow = "Tasks never stop for a permission. Each tool Claude used is listed in the task's activity, under its step.";
    setting_radio(ui, true, "Allow and note in the activity", allow, "");
    let ask = "The task waits for you each time, with what Claude wants to do. Safer with postings you do not trust, and slower.";
    setting_radio(ui, false, "Ask me before each tool", ask, "tool");
    ui.add_enabled_ui(false, |ui| {
        setting_radio(ui, false, "Ask a second reader", "Disabled: nothing to pick yet.", "");
    });
    setting_section(ui, "Monthly cap", None, "");
    let cap = "Once this month's tasks cost that much, Run in the launcher is disabled until the 1st and says why. A task that is running finishes.";
    setting_check_amount(ui, &mut true, &mut "60.00".to_owned(), "Stop at", "a month", cap, "");
    setting_check_amount(ui, &mut false, &mut String::new(), "Stop at", "a month", "Off, an empty amount.", "month");
    setting_check_number(ui, &mut true, &mut "60".to_owned(), "Hide jobs scored under", "", "A whole number, with no unit.", 3, "");
    setting_section(ui, "Text fields", None, "");
    setting_row(ui, "Empty", "The hint, and at most 40 characters.", "", |ui| {
        text_field(ui, &mut String::new(), 280.0, "A new reason", 40);
    });
    setting_row(ui, "Filled", "A reason typed in.", "", |ui| {
        text_field(ui, &mut "Agency, not the client".to_owned(), 280.0, "A new reason", 40);
    });
    setting_section(ui, "Amount fields", None, "");
    setting_row(ui, "Spend at most, USD", "Empty starts each command at its suggested budget.", "", |ui| {
        amount_field(ui, &mut String::new(), 120.0, "Suggested");
    });
    setting_row(ui, "Filled", "An amount typed in.", "", |ui| {
        amount_field(ui, &mut "3.00".to_owned(), 110.0, "Suggested");
    });
    ui.add_enabled_ui(false, |ui| {
        setting_row(ui, "Disabled", "Waits for the task that runs.", "", |ui| {
            amount_field(ui, &mut "5.00".to_owned(), 110.0, "Suggested");
        });
    });
    setting_section(ui, "Secret fields", None, "");
    let field = |ui: &mut Ui, secret: &mut Secret| {
        ui.allocate_ui(egui::vec2(SECRET_WIDTH, ui.available_height()), |ui| secret_field(ui, secret, "Paste a key from the Console"));
    };
    setting_row(ui, "Empty", "The hint, and Show off.", "", |ui| field(ui, &mut Secret::new()));
    setting_row(ui, "Filled", "Masked until Show is on.", "", |ui| field(ui, &mut Secret::from(FAKE_KEY.to_owned())));
    let mut shown = Secret::from(FAKE_KEY.to_owned());
    shown.set_shown(true);
    setting_row(ui, "Shown", "Show is on, so the key reads in full.", "", |ui| field(ui, &mut shown));
    setting_section(ui, "Either way", None, "");
    setting_row(ui, "Questions from Claude", "A question Claude asks you always reaches you, as Your turn on Today and Tasks.", "", |ui| {
        tag(ui, "Always on", Tone::Plain);
    });
    setting_row(ui, "Remembered rules", "A tool you allow for the rest of a task stays allowed only until that task ends.", "", |ui| {
        tag(ui, "This task only", Tone::Plain);
    });
    setting_section(ui, "Tray decisions", None, "");
    let mut button = None;
    setting_row(ui, "Clear tray decisions", "Forgets every decision and skip reason on Today, so Today deals those cards again.", "", |ui| {
        button = Some(small_danger_button(ui, "Clear tray decisions"));
    });
    if let Some(button) = button {
        if button.clicked() {
            *confirm = Some(true);
        }
        confirm_popover(&button, confirm, &CLEAR, CONFIRM_WIDTH);
    }
}

/// The settings sheet's page, `confirm` as `settings_widgets` takes it.
fn settings_page_of(ui: &mut Ui, confirm: &mut Option<bool>) {
    jobdesk::ui_kit::desk(ui, |ui| {
        jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, |ui| {
            paper(ui, false, |ui| {
                egui::Frame::new().inner_margin(22).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    settings_widgets(ui, confirm);
                });
            });
        });
    });
}

fn settings_sheet(theme: Theme, name: &str) {
    let mut confirm = Some(true);
    let mut harness = with_renderer(Harness::builder().with_size(SETTINGS_SHEET).with_pixels_per_point(1.0).with_theme(theme).with_context_setup(install_look))
        .build_ui(move |ui| settings_page_of(ui, &mut confirm));
    harness.run_steps(4);
    assert!(harness.get_by_label("Keep them").accesskit_node().is_focused(), "Keep them takes the focus first");
    compare(&mut harness, name);
}

/// The update sheet's size: the bar in each state, then two bars with their notes open above them.
const UPDATE_SHEET: [f32; 2] = [1000.0, 1640.0];
/// The room the open notes take above their bar, without and with the ready slip.
const NOTES_ROOM: f32 = 500.0;
const READY_NOTES_ROOM: f32 = 560.0;
const RESTART_WAITS: &str = "A task is running. Restart once it finishes, so it is not cut off halfway.";
/// Fictional notes, as git-cliff writes them, with a link, an image and raw HTML, which all show as plain words.
const RELEASE_NOTES: &str = "### Features\n\n- **settings**: keep run logs for 30 days, 90 days or forever ([#41](https://example.com/jobdesk/pull/41))\n- **today**: Later can set a card aside until Monday\n- **tasks**: reopen a stopped task with a reply ([#44](https://example.com/jobdesk/pull/44))\n\n![The new Data and privacy page](https://example.com/jobdesk/data.png)\n\n### Bug fixes\n\n- **jobs**: a cut title shows in full on hover\n- **profile**: Save keeps your draft when the file changed on disk\n- **shell**: the window keeps its size between launches\n- **settings**: search finds Spend at most again <img src=\"https://example.com/x.png\">\n";
const NOTES: ReleaseNotes = ReleaseNotes {
    title: "Jobdesk 0.2.0",
    released: "Released Fri 30 Oct. You have 0.1.0.",
    notes: RELEASE_NOTES,
    notes_label: "What changed in 0.2.0",
    trust: "From the GitHub release. Its links and images do not open here.",
    slip: None,
    primary: Some("Download 0.2.0"),
    primary_wait: None,
    size: Some("14.2 MB"),
    open_page: Some("Open the release page"),
    esc_note: "Nothing installs until you choose Restart to update. Esc closes.",
};
const READY_NOTES: ReleaseNotes = ReleaseNotes {
    slip: Some("Jobdesk closes, installs 0.2.0 and opens again with the same folder and settings. Restart waits while a task is open."),
    primary: Some("Restart to update"),
    primary_wait: Some(RESTART_WAITS),
    size: None,
    esc_note: "Esc closes",
    ..NOTES
};

/// A status strip as the shell lays it out: Claude on the left, then on the right Settings, the divider and `group`,
/// which lays out right to left. Returns what `group` returns.
fn status_strip<R>(ui: &mut Ui, group: impl FnOnce(&mut Ui) -> R) -> R {
    status_frame(ui)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                status_line(ui, true, "Claude is ready");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    status_button(ui, "Settings", &["Cmd", ","], false);
                    ui.add_space(12.0);
                    status_divider(ui);
                    ui.add_space(12.0);
                    group(ui)
                })
                .inner
            })
            .inner
        })
        .inner
}

/// The update group in each state, then two with their notes open: `open` and `cache` are the notes' state.
fn update_widgets(ui: &mut Ui, open: &mut [bool; 2], caches: &mut [CommonMarkCache; 2]) {
    ui.set_width(ui.available_width());
    ui.heading("Status bar");
    ui.add_space(6.0);
    status_strip(ui, |ui| {
        status_action(ui, "Download");
        status_mark_button(ui, Ink::Text, "Jobdesk 0.2.0 is out", false);
    });
    ui.add_space(8.0);
    status_strip(ui, |ui| {
        status_button(ui, "Cancel", &[], false);
        status_progress(ui, "Downloading 0.2.0", 0.38, "38%, 5.4 of 14.2 MB");
    });
    ui.add_space(8.0);
    status_strip(ui, |ui| {
        status_action(ui, "Restart to update");
        status_mark_button(ui, Tone::Hired, "0.2.0 is downloaded and checked", false);
    });
    ui.add_space(8.0);
    status_strip(ui, |ui| {
        enabled_when(ui, false, RESTART_WAITS, |ui| status_action(ui, "Restart to update"));
        status_mark_button(ui, Tone::Hired, "0.2.0 is downloaded and checked", false);
    });
    ui.add_space(8.0);
    status_strip(ui, |ui| {
        status_action(ui, "Retry");
        status_mark_button(ui, Tone::Closed, "Download of 0.2.0 stopped at 41%: the connection dropped", false);
    });
    ui.add_space(8.0);
    status_strip(ui, |ui| {
        status_action(ui, "Retry");
        status_mark_button(ui, Tone::Closed, "0.2.0 failed its signature check. It was deleted, not installed.", false);
    });
    ui.add_space(8.0);
    // In a narrow window the words are cut first.
    ui.allocate_ui(egui::vec2(560.0, 40.0), |ui| {
        status_strip(ui, |ui| {
            status_action(ui, "Retry");
            status_mark_button(ui, Tone::Closed, "Download of 0.2.0 stopped at 41%: the connection dropped", false);
        });
    });
    ui.add_space(24.0);
    ui.heading("Release notes");
    for (index, (notes, room)) in [(NOTES, NOTES_ROOM), (READY_NOTES, READY_NOTES_ROOM)].into_iter().enumerate() {
        ui.add_space(room);
        let (square, words) = if notes.slip.is_some() { (Tone::Hired.into(), "0.2.0 is downloaded and checked") } else { (Ink::Text, "Jobdesk 0.2.0 is out") };
        let mark = status_strip(ui, |ui| {
            status_action(ui, if notes.slip.is_some() { "Restart to update" } else { "Download" });
            status_mark_button(ui, square, words, open[index])
        });
        if mark.clicked() {
            open[index] = !open[index];
        }
        release_notes_popover(&mark, &mut open[index], &notes, &mut caches[index]);
    }
}

fn update_page(ui: &mut Ui, open: &mut [bool; 2], caches: &mut [CommonMarkCache; 2]) {
    jobdesk::ui_kit::desk(ui, |ui| jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, |ui| update_widgets(ui, open, caches)));
}

fn update_sheet(theme: Theme, name: &str) {
    let (mut open, mut caches) = ([true, true], [CommonMarkCache::default(), CommonMarkCache::default()]);
    let mut harness = with_renderer(Harness::builder().with_size(UPDATE_SHEET).with_pixels_per_point(1.0).with_theme(theme).with_context_setup(install_look))
        .build_ui(move |ui| update_page(ui, &mut open, &mut caches));
    harness.run_steps(4);
    // Both open on the same frame, and the focus goes to the last: Restart to update waits, so its notes take it.
    let notes = harness.get_all_by_label("What changed in 0.2.0").max_by(|a, b| a.rect().top().total_cmp(&b.rect().top())).unwrap();
    assert!(notes.accesskit_node().is_focused(), "the ready notes take the focus");
    compare(&mut harness, name);
}

#[test]
fn ui_kit_gallery_update_dark() {
    update_sheet(Theme::Dark, "ui_kit_gallery_update_dark");
}

#[test]
fn ui_kit_gallery_update_light() {
    update_sheet(Theme::Light, "ui_kit_gallery_update_light");
}

/// The notes open with their primary action focused and close on Esc, a click outside and the primary action. The
/// release page link keeps them open, and a primary action that waits is disabled, says why and takes no focus.
#[test]
fn release_notes_popover_opens_on_its_primary_action() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Shown {
        open: bool,
        wait: bool,
        blocked: bool,
        picked: Vec<NotesAction>,
    }
    let shown = Rc::new(RefCell::new(Shown::default()));
    let state = Rc::clone(&shown);
    let mut cache = CommonMarkCache::default();
    let mut harness = Harness::builder().with_size([900.0, 700.0]).with_context_setup(install_look).build_ui(move |ui| {
        let mut state = state.borrow_mut();
        ui.label("Outside");
        ui.add_space(600.0);
        let mark = status_mark_button(ui, Ink::Text, "Jobdesk 0.2.0 is out", state.open);
        if mark.clicked() {
            state.open = !state.open;
        }
        let primary = if state.blocked { None } else { NOTES.primary };
        let notes = ReleaseNotes { primary, primary_wait: state.wait.then_some(RESTART_WAITS), ..NOTES };
        let Shown { open, picked, .. } = &mut *state;
        picked.extend(release_notes_popover(&mark, open, &notes, &mut cache));
    });
    harness.run();
    let open = |harness: &Harness| harness.query_by_label("Download 0.2.0").is_some();
    assert!(!open(&harness));

    harness.get_by_label("Jobdesk 0.2.0 is out").click();
    harness.run();
    assert!(harness.get_by_label("Download 0.2.0").accesskit_node().is_focused(), "Download takes the focus first");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!open(&harness), "Esc closes");

    harness.get_by_label("Jobdesk 0.2.0 is out").click();
    harness.run();
    assert!(harness.get_by_label("Download 0.2.0").accesskit_node().is_focused(), "Download takes the focus again");
    harness.get_by_label("Outside").click();
    harness.run();
    assert!(!open(&harness), "a click outside closes");

    harness.get_by_label("Jobdesk 0.2.0 is out").click();
    harness.run();
    harness.get_by_label("Open the release page").click();
    harness.run();
    assert!(open(&harness), "the release page link keeps the notes open");
    harness.get_by_label("Download 0.2.0").click();
    harness.run();
    assert!(!open(&harness), "the primary action closes");
    assert_eq!(shown.borrow().picked, [NotesAction::OpenPage, NotesAction::Primary]);

    shown.borrow_mut().wait = true;
    harness.get_by_label("Jobdesk 0.2.0 is out").click();
    harness.run();
    let primary = harness.get_by_label("Download 0.2.0");
    assert!(primary.accesskit_node().is_disabled() && !primary.accesskit_node().is_focused(), "a waiting action takes no focus");
    assert!(harness.get_by_label("What changed in 0.2.0").accesskit_node().is_focused(), "the notes take it, inside the popover");
    let why = |harness: &Harness| harness.query_all_by_label(RESTART_WAITS).count();
    assert_eq!(why(&harness), 1, "a waiting action says why in the slip, for the keyboard");
    primary.hover();
    harness.run_steps(4);
    assert_eq!(why(&harness), 2, "and on hover");
    harness.key_press(egui::Key::Escape);
    harness.run();

    {
        let mut shown = shown.borrow_mut();
        (shown.wait, shown.blocked) = (false, true);
    }
    harness.get_by_label("Jobdesk 0.2.0 is out").click();
    harness.run();
    assert!(!open(&harness), "no primary action to show");
    assert!(harness.get_by_label("What changed in 0.2.0").accesskit_node().is_focused(), "with no action at all, the notes take the focus");
}

/// The notes take a Tab stop and scroll on the keys, and they start at the top each time they open and for each release.
#[test]
fn release_notes_scroll_from_the_keyboard() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let shown = Rc::new(RefCell::new((false, "Jobdesk 0.2.0")));
    let state = Rc::clone(&shown);
    let mut cache = CommonMarkCache::default();
    let mut harness = Harness::builder().with_size([900.0, 700.0]).with_context_setup(install_look).build_ui(move |ui| {
        let mut state = state.borrow_mut();
        ui.add_space(600.0);
        let mark = status_mark_button(ui, Ink::Text, "Jobdesk 0.2.0 is out", state.0);
        if mark.clicked() {
            state.0 = !state.0;
        }
        let notes = ReleaseNotes { title: state.1, ..NOTES };
        release_notes_popover(&mark, &mut state.0, &notes, &mut cache);
    });
    harness.run();
    let top = |harness: &Harness| harness.get_by_label("Features").rect().top();
    let focused = |harness: &Harness| harness.get_by_label("What changed in 0.2.0").accesskit_node().is_focused();
    let tab_to_notes = |harness: &mut Harness| {
        for _ in 0..8 {
            if focused(harness) {
                return;
            }
            harness.key_press(egui::Key::Tab);
            harness.run();
        }
        panic!("Tab never reached the notes");
    };
    let press = |harness: &mut Harness, key| {
        harness.key_press(key);
        harness.run();
    };

    harness.get_by_label("Jobdesk 0.2.0 is out").click();
    harness.run();
    let start = top(&harness);
    tab_to_notes(&mut harness);
    press(&mut harness, egui::Key::PageDown);
    let paged = top(&harness);
    assert!(paged < start - 100.0, "Page Down moved the notes from {start} to {paged}");
    press(&mut harness, egui::Key::ArrowUp);
    assert!(top(&harness) > paged, "Up moves back a line");
    assert!(focused(&harness), "the arrows keep the focus on the notes");
    press(&mut harness, egui::Key::End);
    assert!(top(&harness) <= paged, "End goes to the foot");
    press(&mut harness, egui::Key::Home);
    assert_eq!(top(&harness), start, "Home goes back to the top");

    press(&mut harness, egui::Key::PageDown);
    press(&mut harness, egui::Key::Escape);
    assert!(harness.query_by_label("Download 0.2.0").is_none(), "Esc closes");
    harness.get_by_label("Jobdesk 0.2.0 is out").click();
    harness.run();
    assert_eq!(top(&harness), start, "the notes open at the top again");

    tab_to_notes(&mut harness);
    press(&mut harness, egui::Key::PageDown);
    assert!(top(&harness) < start);
    shown.borrow_mut().1 = "Jobdesk 0.2.1";
    harness.run();
    assert_eq!(top(&harness), start, "another release's notes start at the top");
}

#[test]
fn ui_kit_gallery_settings_dark() {
    settings_sheet(Theme::Dark, "ui_kit_gallery_settings_dark");
}

#[test]
fn ui_kit_gallery_settings_light() {
    settings_sheet(Theme::Light, "ui_kit_gallery_settings_light");
}

#[test]
fn ui_kit_gallery_usage_dark() {
    usage_sheet(Theme::Dark, "ui_kit_gallery_usage_dark");
}

#[test]
fn ui_kit_gallery_usage_light() {
    usage_sheet(Theme::Light, "ui_kit_gallery_usage_light");
}

#[test]
fn ui_kit_gallery_dark() {
    render(Theme::Dark, "ui_kit_gallery_dark");
}

#[test]
fn ui_kit_gallery_light() {
    render(Theme::Light, "ui_kit_gallery_light");
}

/// The switch sheet's size: one row for each state, then the name lines and the fitted row tags, with the paper's foot
/// in view.
const SWITCH_SHEET: [f32; 2] = [520.0, 470.0];
/// A width too narrow for the name line's whole text, so it is cut.
const CUT_NAME: f32 = 150.0;
/// A width too narrow for the fitted row tag's whole tag, so its words are cut.
const CUT_TAG: f32 = 120.0;
/// A fictional portal for the switches.
const SWITCH_NAME: &str = "Northwind Jobs";

/// The switch on paper, as a portal row shows it: on, off, both disabled, and off with the focus ring. Then the name line a
/// portal row leads with: a name and its note, a name alone, and one cut at the right. Then the fitted row tag of a
/// community portal: whole where it fits, and its words cut where it does not.
fn switches(ui: &mut Ui) {
    paper(ui, false, |ui| {
        egui::Frame::new().inner_margin(22).show(ui, |ui| {
            ui.set_width(ui.available_width());
            Grid::new("switches").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                for (label, on, enabled, focus) in [
                    ("On", true, true, false),
                    ("Off", false, true, false),
                    ("On, disabled", true, false, false),
                    ("Off, disabled", false, false, false),
                    ("Off, focused", false, true, true),
                ] {
                    desk_label(ui, label, Type::NOTE, Ink::Muted);
                    let response = enabled_when(ui, enabled, "A task is running.", |ui| switch(ui, on, SWITCH_NAME));
                    if focus {
                        response.request_focus();
                    }
                    ui.end_row();
                }
                desk_label(ui, "Name line", Type::NOTE, Ink::Muted);
                name_line(ui, SWITCH_NAME, Some("Belgium"));
                ui.end_row();
                desk_label(ui, "Name alone", Type::NOTE, Ink::Muted);
                name_line(ui, SWITCH_NAME, None);
                ui.end_row();
                desk_label(ui, "Name line, cut", Type::NOTE, Ink::Muted);
                ui.allocate_ui(egui::vec2(CUT_NAME, 24.0), |ui| name_line(ui, "Northwind Regional Jobs", Some("Belgium")));
                ui.end_row();
                desk_label(ui, "Fitted row tag", Type::NOTE, Ink::Muted);
                fitted_row_tag(ui, "Unmaintained since 2026-05", Tone::Interview);
                ui.end_row();
                desk_label(ui, "Fitted row tag, cut", Type::NOTE, Ink::Muted);
                ui.allocate_ui_with_layout(egui::vec2(CUT_TAG, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    fitted_row_tag(ui, "Unmaintained since 2026-05", Tone::Interview)
                });
                ui.end_row();
            });
        });
    });
}

fn switch_page(ui: &mut Ui) {
    jobdesk::ui_kit::desk(ui, |ui| jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, switches));
}

fn switch_sheet(theme: Theme, name: &str) {
    let mut harness = with_renderer(Harness::builder().with_size(SWITCH_SHEET).with_pixels_per_point(1.0).with_theme(theme).with_context_setup(install_look))
        .build_ui(switch_page);
    harness.run_steps(3);
    compare(&mut harness, name);
}

#[test]
fn ui_kit_gallery_switch_dark() {
    switch_sheet(Theme::Dark, "ui_kit_gallery_switch_dark");
}

#[test]
fn ui_kit_gallery_switch_light() {
    switch_sheet(Theme::Light, "ui_kit_gallery_switch_light");
}

/// A click flips the switch through its caller, a screen reader hears its name with its state, and a disabled one
/// takes no click.
#[test]
fn switch_flips_on_a_click_and_says_its_state() {
    use std::cell::Cell;
    use std::rc::Rc;

    let on = Rc::new(Cell::new(false));
    let state = Rc::clone(&on);
    let mut harness = Harness::builder().with_size([400.0, 120.0]).with_context_setup(install_look).build_ui(move |ui| {
        if switch(ui, state.get(), SWITCH_NAME).clicked() {
            state.set(!state.get());
        }
        enabled_when(ui, false, "A task is running.", |ui| switch(ui, true, "Fabrikam Careers"));
    });
    harness.run();
    let toggled = |harness: &Harness, label: &str| harness.get_by_label(label).accesskit_node().toggled();
    let off = harness.get_by_label("Northwind Jobs, off");
    assert_eq!(off.accesskit_node().role(), Role::CheckBox);
    assert_eq!(toggled(&harness, "Northwind Jobs, off"), Some(egui::accesskit::Toggled::False));

    harness.get_by_label("Northwind Jobs, off").click();
    harness.run();
    assert!(on.get());
    assert_eq!(toggled(&harness, "Northwind Jobs, on"), Some(egui::accesskit::Toggled::True));
    harness.get_by_label("Northwind Jobs, on").click();
    harness.run();
    assert!(!on.get());

    let disabled = harness.get_by_label("Fabrikam Careers, on");
    assert!(disabled.accesskit_node().is_disabled());
    disabled.click();
    harness.run();
    assert_eq!(toggled(&harness, "Fabrikam Careers, on"), Some(egui::accesskit::Toggled::True));
    assert!(!on.get());
}

/// Every clickable kit widget in the galleries shows the pointing hand under the pointer, and a disabled one keeps the arrow.
#[test]
fn every_clickable_widget_shows_the_hand() {
    for (sheet, height, least) in [(gallery as fn(&mut Ui), SHEET[1], 20), (profile_gallery, PROFILE_HEIGHT, 10), (wizard, WIZARD_SHEET[1], 5)] {
        let mut harness = Harness::builder().with_size([1000.0, height]).with_pixels_per_point(1.0).with_context_setup(install_look).build_ui(move |ui| {
            jobdesk::ui_kit::desk(ui, |ui| {
                jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, |ui| {
                    ui.add_enabled_ui(false, |ui| button(ui, "Switched off"));
                    sheet(ui);
                });
            });
        });
        harness.run();
        let shown = hands(&mut harness);
        assert!(shown > least, "the sheet shows {shown} clickable widgets");
        harness.get_by_label("Switched off").hover();
        harness.step();
        assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::Default);
    }

    // The settings sheet, with its popover open, so its buttons are hovered too.
    let mut confirm = Some(false);
    let mut harness = Harness::builder()
        .with_size(SETTINGS_SHEET)
        .with_pixels_per_point(1.0)
        .with_context_setup(install_look)
        .build_ui(move |ui| settings_page_of(ui, &mut confirm));
    harness.run();
    // Hide the command, two radios, two cap checkboxes and the score's, three Show toggles, Clear tray decisions, and the
    // popover's Keep them and Clear decisions.
    assert_eq!(hands(&mut harness), 12, "the settings sheet's clickable widgets");
    harness.get_by_label("Ask a second reader").hover();
    harness.step();
    assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::Default, "a disabled radio keeps the arrow");

    // The update sheet, with both notes open, so their actions are hovered too.
    let (mut open, mut caches) = ([true, true], Default::default());
    let mut harness = Harness::builder()
        .with_size(UPDATE_SHEET)
        .with_pixels_per_point(1.0)
        .with_context_setup(install_look)
        .build_ui(move |ui| update_page(ui, &mut open, &mut caches));
    harness.run();
    // Nine Settings, eight enabled actions and Cancel, eight mark buttons, and in the notes Download and two release
    // page links. Both Restart buttons that wait keep the arrow.
    assert_eq!(hands(&mut harness), 28, "the update sheet's clickable widgets");

    // The switch sheet: on, off and focused. Both disabled switches keep the arrow.
    let mut harness = Harness::builder().with_size(SWITCH_SHEET).with_pixels_per_point(1.0).with_context_setup(install_look).build_ui(switch_page);
    harness.run();
    assert_eq!(hands(&mut harness), 3, "the switch sheet's clickable widgets");
}

/// Hovers every enabled clickable widget and checks each shows the pointing hand. Returns how many it hovered.
fn hands(harness: &mut Harness) -> usize {
    let mut clickable = Vec::new();
    for role in [Role::Button, Role::Tab, Role::RadioButton, Role::CheckBox, Role::Link] {
        for node in harness.query_all_by_role(role) {
            if !node.accesskit_node().is_disabled() {
                clickable.push(node.rect().center());
            }
        }
    }
    for at in &clickable {
        harness.hover_at(*at);
        harness.step();
        assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::PointingHand, "the pointer at {at:?}");
    }
    clickable.len()
}

/// A click on a setting radio's words picks it, a click in the cap's field types there and leaves the box as it was,
/// and a click on the cap's words flips the box.
#[test]
fn settings_controls_take_their_own_clicks() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Shown {
        permission: usize,
        cap_on: bool,
        cap: String,
    }
    let shown = Rc::new(RefCell::new(Shown::default()));
    let state = Rc::clone(&shown);
    let mut harness = Harness::builder().with_size([800.0, 400.0]).with_context_setup(install_look).build_ui(move |ui| {
        let mut state = state.borrow_mut();
        for (at, label) in ["Allow and note", "Ask first"].into_iter().enumerate() {
            if setting_radio(ui, state.permission == at, label, "", "").clicked() {
                state.permission = at;
            }
        }
        let Shown { cap_on, cap, .. } = &mut *state;
        setting_check_amount(ui, cap_on, cap, "Stop at", "a month", "Run waits at the cap.", "");
    });
    harness.run();
    let on = |harness: &Harness, label: &str| harness.get_by_label(label).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True);
    assert!(on(&harness, "Allow and note") && !on(&harness, "Ask first"));
    harness.get_by_label("Ask first").click();
    harness.run();
    assert_eq!(shown.borrow().permission, 1);
    assert!(on(&harness, "Ask first") && !on(&harness, "Allow and note"));

    harness.get_by_label("Stop at a month");
    let field = harness.get_by_role(Role::TextInput);
    assert_eq!(field.accesskit_node().label().as_deref(), Some("Stop at"));
    field.click();
    harness.run();
    harness.get_by_role(Role::TextInput).type_text("60");
    harness.run();
    assert!(!shown.borrow().cap_on, "a click in the field flipped the box");
    assert_eq!(shown.borrow().cap, "60");
    harness.get_by_label("Stop at $60 a month").click();
    harness.run();
    assert!(shown.borrow().cap_on);
    // The words after the field and the help take a click too.
    let help = harness.get_by_label("Stop at $60 a month").rect().left_bottom() + egui::vec2(40.0, 12.0);
    harness.hover_at(help);
    harness.step();
    assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::PointingHand, "the help shows the hand");
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: help,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
    }
    assert!(!shown.borrow().cap_on, "a click on the help left the box as it was");
}

/// The popover asks on a click and keeps on Keep, on Esc and on a click outside. Only the danger button acts.
#[test]
fn confirm_popover_keeps_unless_told_to_act() {
    use std::cell::Cell;
    use std::rc::Rc;

    let acted = Rc::new(Cell::new(0));
    let seen = Rc::clone(&acted);
    let mut confirm = None;
    let mut harness = Harness::builder().with_size([600.0, 400.0]).with_context_setup(install_look).build_ui(move |ui| {
        let button = small_danger_button(ui, "Clear tray decisions");
        if button.clicked() {
            confirm = Some(true);
        }
        if confirm_popover(&button, &mut confirm, &CLEAR, CONFIRM_WIDTH) {
            seen.set(seen.get() + 1);
        }
        ui.add_space(200.0);
        ui.label("Outside");
    });
    harness.run();
    let open = |harness: &Harness| harness.query_by_label("Keep them").is_some();
    assert!(!open(&harness));

    harness.get_by_label("Clear tray decisions").click();
    harness.run();
    assert!(harness.get_by_label("Keep them").accesskit_node().is_focused(), "Keep them takes the focus first");
    harness.get_by_label("Keep them").click();
    harness.run();
    assert!(!open(&harness));

    harness.get_by_label("Clear tray decisions").click();
    harness.run();
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!open(&harness), "Esc keeps");

    harness.get_by_label("Clear tray decisions").click();
    harness.run();
    harness.get_by_label("Outside").click();
    harness.run();
    assert!(!open(&harness), "a click outside keeps");
    assert_eq!(acted.get(), 0);

    harness.get_by_label("Clear tray decisions").click();
    harness.run();
    harness.get_by_label("Clear decisions").click();
    harness.run();
    assert!(!open(&harness));
    assert_eq!(acted.get(), 1);
}
