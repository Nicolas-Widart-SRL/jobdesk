//! One sheet showing every Ledger component, in both themes.

use egui::{CursorIcon, Grid, Role, ScrollArea, Theme, Ui};
use egui_commonmark::CommonMarkCache;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder, SnapshotOptions};
use jobdesk::shared_kernel::placeholders::{self, Piece};
use jobdesk::shell::install_look;
use jobdesk::ui_kit::tokens::{GUTTER, KNOT_SIZE, KNOT_SIZE_SMALL, TRAY_RAIL};
use jobdesk::ui_kit::{
    AgendaEnd, BandCell, BlockEditor, CardLook, CellLook, ChangedOnDisk, CheckMark, Confirm, DayHeatmap, DealLook, EditorBusy, Figure, FlowNode, HeatDay, Ink,
    KnotState, NodeMarks, NotesAction, Palette, ReleaseNotes, RoleEntry, Secret, Subtitle, TabMark, Title, Tone, Type, agenda_return, agenda_row, alert,
    amount_field, attention_count, bar, blank_card, block_editor, brand, button, card_skeleton, card_tab, check_tag, code_box, command_button, command_row,
    confirm_popover, copy_button, danger_button, danger_shortcut_button, day_heatmap, day_heatmap_loading, day_stamp, deal, deal_footer, divider, document,
    doughnut, empty_state, enabled_when, faint_mono, fat_tally, figure, figure_band, figure_band_loading, flow_band, folder_tab, index_card, key_button,
    key_hint, knot, lane, lane_title, lead_line, link, list_row, measure, muted, note, page_frame, page_head, panel, panel_body, panel_header,
    panel_header_tone, paper, path_label, pick_row, primary_button, primary_key_button, profile_markdown, prompt_box, prompt_box_tone, rail_title,
    release_notes_popover, reveal_command, rich, role_entry, ruled_note_with_command, score, search_field, secret_field, segmented, select_row, setting_check,
    setting_check_amount, setting_check_number, setting_radio, setting_row, setting_section, settings_page, shell_top_frame, shortcut_button, skeleton,
    skeleton_lines, slip, slip_bar, small_button, small_danger_button, small_key_button, small_toggle, solid_tag, stamp, stamp_square, stamped_head,
    status_action, status_button, status_divider, status_dot, status_frame, status_line, status_mark_button, status_progress, status_waits, step_line, strong,
    table_head, tag, tally, tally_strip, text_field, toggle_group, turn_band, undo_row,
};

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
            BandCell { label: "Whose turn", value: "Claude's", note: "Working for 1m 50s", look: CellLook::Ink(Tone::Active), marks: &[] },
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
            BandCell { label: "Where", value: "All 7 steps", note: "Finished in 14m 20s", look: CellLook::Plain, marks: &finished },
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

fn profile_gallery(ui: &mut Ui) {
    ui.set_width(ui.available_width());
    ui.heading("Profile");
    ui.add_space(6.0);
    profile(ui);
}

/// The height of the Profile sheet. The main sheet is near the 8192px a texture holds, so these parts take a sheet of their own.
const PROFILE_HEIGHT: f32 = 2200.0;

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
    assert!(harness.get_by_label("Download 0.2.0").accesskit_node().is_focused(), "Download takes the focus first");
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
        let notes = ReleaseNotes { primary_wait: state.wait.then_some(RESTART_WAITS), ..NOTES };
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
    primary.hover();
    harness.run_steps(4);
    assert!(harness.query_by_label_contains("A task is running").is_some(), "a waiting action says why");
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

/// Every clickable kit widget in the galleries shows the pointing hand under the pointer, and a disabled one keeps the arrow.
#[test]
fn every_clickable_widget_shows_the_hand() {
    for (sheet, height, least) in [(gallery as fn(&mut Ui), SHEET[1], 20), (profile_gallery, PROFILE_HEIGHT, 10)] {
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
