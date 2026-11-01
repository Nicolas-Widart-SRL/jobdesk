//! One sheet showing every Ledger component, in both themes.

use egui::{CursorIcon, Grid, Role, ScrollArea, Theme, Ui};
use egui_commonmark::CommonMarkCache;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, SnapshotOptions};
use jobdesk::shell::install_look;
use jobdesk::ui_kit::tokens::{GUTTER, KNOT_SIZE, KNOT_SIZE_SMALL, TRAY_RAIL};
use jobdesk::ui_kit::{
    AgendaEnd, BandCell, CardLook, CellLook, CheckMark, DealLook, FlowNode, Ink, KnotState, Palette, Subtitle, TabMark, Title, Tone, Type, agenda_return, agenda_row, alert, attention_count, bar, blank_card, brand, button, card_skeleton, card_tab,
    check_tag, code_box, command_button, command_row, copy_button, danger_button, day_stamp, deal, deal_footer, divider, document, doughnut, empty_state, faint_mono, fat_tally, figure,
    flow_band, folder_tab, index_card, key_button, key_hint, knot, lane, lane_title, lead_line, link, list_row, measure, muted, note,
    page_frame, page_head, panel, panel_body, panel_header, panel_header_tone, paper, path_label, pick_row, primary_button, primary_key_button, prompt_box, prompt_box_tone, reveal_command, rich,
    score, segmented, select_row, shell_top_frame, shortcut_button, skeleton, skeleton_lines, slip, slip_bar, small_button, small_danger_button, small_key_button, small_toggle, solid_tag, stamp,
    stamp_square, stamped_head, status_dot, status_frame, status_line, status_segmented, step_line, strong, table_head, tag, tally, tally_strip, turn_band, undo_row,
};

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
    ui.add_space(8.0);
    status_frame(ui).show(ui, |ui| {
        status_segmented(ui, 1, &[(0, "System".to_owned()), (1, "Light".to_owned()), (2, "Dark".to_owned())]);
    });
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
    let stages = [("Scrape", "15", vec![(9, Some(Ink::Text)), (6, None)], "new of 64 seen"), ("Rank", "5", vec![(5, Some(Ink::Text))], "on shortlist"), ("Apply", "2", vec![(2, Some(Ink::Text))], "drafted, not sent")];
    let nodes: Vec<FlowNode> = stages.into_iter().map(|(label, figure, marks, note)| FlowNode { label, figure: figure.to_owned(), marks, note: note.to_owned(), hidden: None }).collect();
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
    empty_state(ui, "Nothing here yet", "A sentence on what the screen holds and how to fill it, at most 560 px wide, then the actions that open the launcher.", |ui| {
        primary_button(ui, "Find things…");
        button(ui, "Something else…");
    });
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
        deal(ui, tab, DealLook { key: "send", frame: None, behind: 3 }, |ui| {
            ui.label(rich(ui, "A dealt card", Type::DEAL, Ink::Text));
        }, |ui| {
            primary_key_button(ui, "Mark as sent…", "Y");
            key_button(ui, "Later", "L");
        });
    });
    ui.add_space(10.0);
    dealt(ui, "asks", |ui| {
        let tab = card_tab(ui, "Claude asks", Tone::Turn, "Paused task");
        deal(ui, tab, DealLook { key: "asks", frame: Some(Tone::Interview), behind: 9 }, |ui| {
            ui.label(rich(ui, "A card that waits", Type::DEAL, Ink::Text));
        }, |ui| {
            key_button(ui, "Later", "L");
        });
    });
    ui.add_space(10.0);
    dealt(ui, "turn", |ui| {
        let tab = card_tab(ui, "Your turn", Tone::Turn, "Task waits");
        deal(ui, tab, DealLook { key: "turn", frame: Some(Tone::Turn), behind: 1 }, |ui| {
            ui.label(rich(ui, "A card for a task that waits", Type::DEAL, Ink::Text));
        }, |ui| {
            key_button(ui, "Later", "L");
        });
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
            BandCell { label: "What next", value: "Nothing for now", note: "You can leave. The Tasks tab marks it when it needs you.", look: CellLook::Plain, marks: &[] },
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
            stamped_head(ui, |ui| {
                day_stamp(ui, "Sat", "31", "Oct", 5, 0, "Saturday 31 October. 0 of 5 decided today.");
            }, "5 to decide", "Start with Meridian Rail. It closes Thursday.");
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

type Section = (&'static str, fn(&mut Ui));

fn gallery(ui: &mut Ui) {
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let sections_of_gallery: [Section; 13] = [
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

fn render(theme: Theme, name: &str) {
    // One point per pixel: at more, the sheet is taller than the 8192px a texture holds.
    let mut harness = Harness::builder()
        .with_size([1000.0, 7450.0])
        .with_pixels_per_point(1.0)
        .with_theme(theme)
        .with_context_setup(install_look)
        .wgpu()
        .build_ui(|ui| {
            jobdesk::ui_kit::desk(ui, |ui| jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, gallery));
        });
    harness.run_steps(3);
    // The pointer rests on one row, so the sheet shows hover beside picked.
    harness.get_by_label(HOVERED_ROW).hover();
    harness.run_steps(2);
    harness.snapshot_options(name, &SnapshotOptions::new().threshold(2.0).max_failed_pixels(200));
}

#[test]
fn ui_kit_gallery_dark() {
    render(Theme::Dark, "ui_kit_gallery_dark");
}

#[test]
fn ui_kit_gallery_light() {
    render(Theme::Light, "ui_kit_gallery_light");
}

/// Every clickable kit widget in the gallery shows the pointing hand under the pointer, and a disabled one keeps the arrow.
#[test]
fn every_clickable_widget_shows_the_hand() {
    let mut harness = Harness::builder().with_size([1000.0, 7450.0]).with_pixels_per_point(1.0).with_context_setup(install_look).build_ui(|ui| {
        jobdesk::ui_kit::desk(ui, |ui| {
            jobdesk::ui_kit::sheet_frame().inner_margin(24).show(ui, |ui| {
                ui.add_enabled_ui(false, |ui| button(ui, "Switched off"));
                gallery(ui);
            });
        });
    });
    harness.run();
    let mut clickable = Vec::new();
    for role in [Role::Button, Role::Tab, Role::RadioButton, Role::Link] {
        for node in harness.query_all_by_role(role) {
            if !node.accesskit_node().is_disabled() {
                clickable.push(node.rect().center());
            }
        }
    }
    assert!(clickable.len() > 20, "the gallery shows {} clickable widgets", clickable.len());
    for at in clickable {
        harness.hover_at(at);
        harness.step();
        assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::PointingHand, "the pointer at {at:?}");
    }
    harness.get_by_label("Switched off").hover();
    harness.step();
    assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::Default);
}
