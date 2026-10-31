//! One sheet showing every Ledger component, in both themes.

use egui::{Grid, ScrollArea, Theme, Ui};
use egui_kittest::{Harness, SnapshotOptions};
use jobdesk::shell::install_look;
use jobdesk::ui_kit::{
    Ink, Palette, Tone, Type, alert, bar, button, card_tab, clear_panel, code_box, command_button, danger_button, deal, desk_frame, divider, doughnut, faint_mono, fat_tally, figure,
    folder_tab, horizon, horizon_day, horizon_mark, index_card, key_button, key_hint, lane, lane_title, list_row, measure, muted, note, page_frame, panel, panel_body, panel_header,
    pick_row, primary_button, primary_key_button, prompt_box, rich, score, segmented, select_row, side_tab, slip, slip_bar, small_button, stack_edges, stamp, status_segmented,
    step_rail, strong, table_head, tag, tally, undo_bar, undo_button,
};

const TONES: [(&str, Tone); 7] = [
    ("Plain", Tone::Plain),
    ("Drafted", Tone::Drafted),
    ("Active", Tone::Active),
    ("Interview", Tone::Interview),
    ("Offer", Tone::Offer),
    ("Hired", Tone::Hired),
    ("Closed", Tone::Closed),
];

const SWATCHES_PER_ROW: usize = 7;

fn swatches(ui: &mut Ui) {
    let p = Palette::of(ui);
    let colors = [
        ("bg", p.bg),
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
        ("page", p.page),
        ("page_line", p.page_line),
        ("page_head", p.page_head),
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
        key_button(ui, "Later", "L");
        primary_key_button(ui, "Send", "Y");
        command_button(ui, "/rank");
        ui.add_enabled_ui(false, |ui| {
            button(ui, "Off");
            primary_button(ui, "Off");
        });
    });
}

fn marks(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        for (name, tone) in TONES {
            tag(ui, name, tone);
        }
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
    desk_frame(ui).inner_margin(12).show(ui, |ui| {
        ui.horizontal(|ui| {
            folder_tab(ui, "In-tray", Some("3"), true);
            folder_tab(ui, "Jobs", None, false);
            folder_tab(ui, "Runs", Some("1 waiting"), false);
        });
    });
    ui.add_space(10.0);
    ui.columns_const(|[left, right]| {
        desk_frame(left).inner_margin(12).show(left, |ui| {
            ui.set_clip_rect(ui.max_rect());
            side_tab(ui, "Tracker", None, true);
            side_tab(ui, "Insights", Some("2"), false);
        });
        segmented(right, 1, &[(0, "Documents".to_owned()), (1, "Evaluation".to_owned()), (2, "Posting".to_owned())]);
        right.add_space(8.0);
        status_segmented(right, 1, &[(0, "Dark".to_owned()), (1, "Light".to_owned()), (2, "System".to_owned())]);
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
    code_box(ui, |ui| {
        ui.label(rich(ui, "cd cv && lualatex main_harbor.tex", Type::MONO, Ink::Text));
    });
    ui.add_space(10.0);
    prompt_box(ui, |ui| {
        strong(ui, "A prompt that needs an answer");
    });
    ui.add_space(10.0);
    step_rail(ui, &["Fetch", "Evaluate", "Draft", "Verify"], 2, true);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        page_frame(ui, |ui| {
            ui.allocate_exact_size(egui::vec2(120.0, 80.0), egui::Sense::hover());
        });
        clear_panel(ui, |ui| {
            ui.label(rich(ui, "Tray clear", Type::CLEAR, Ink::Text));
        });
    });
}

fn board(ui: &mut Ui) {
    ui.columns_const(|[left, right]| {
        lane(left, |ui| {
            lane_title(ui, "Interview", Tone::Interview);
            index_card(ui, true, |ui| {
                strong(ui, "Picked card");
                muted(ui, "Lead Java Engineer");
            });
            index_card(ui, false, |ui| {
                strong(ui, "Index card");
                ui.label(rich(ui, "78", Type::FIT, Ink::Text));
            });
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
    let days = ["Sat 31", "Sun 1", "Mon 2", "Tue 3", "Wed 4", "Thu 5", "Fri 6"];
    horizon(ui, &days, |ui, day, today| {
        horizon_day(ui, day, today);
        if today {
            horizon_mark(ui, "Harbor deadline", Some(Tone::Drafted));
        } else if *day == "Mon 2" {
            horizon_mark(ui, "Set aside", None);
        }
    });
    ui.add_space(10.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    card_tab(ui, "Send", Tone::Drafted, "Due today");
    deal(ui, false, |ui| {
        ui.label(rich(ui, "A dealt card", Type::DEAL, Ink::Text));
    });
    stack_edges(ui, 3);
    ui.add_space(10.0);
    card_tab(ui, "Permission", Tone::Interview, "Paused run");
    deal(ui, true, |ui| {
        ui.label(rich(ui, "A card that waits", Type::DEAL, Ink::Text));
    });
    stack_edges(ui, 9);
    undo_bar(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Skipped");
            undo_button(ui, "Domain", true, None);
            undo_button(ui, "Rate", false, None);
            undo_button(ui, "Undo", false, Some("U"));
        });
    });
}

fn gallery(ui: &mut Ui) {
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let sections_of_gallery: [(&str, fn(&mut Ui)); 9] = [
            ("Palette", swatches),
            ("Type scale", type_scale),
            ("Text", text_styles),
            ("Buttons", buttons),
            ("Marks", marks),
            ("Tabs", tabs),
            ("Sections and rows", sections),
            ("Board", board),
            ("In-tray", tray),
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
    let mut harness = Harness::builder()
        .with_size([1000.0, 3300.0])
        .with_pixels_per_point(1.5)
        .with_theme(theme)
        .with_context_setup(|ctx| install_look(ctx))
        .wgpu()
        .build_ui(|ui| {
            jobdesk::ui_kit::sheet_frame(ui).inner_margin(24).show(ui, gallery);
        });
    harness.run_steps(3);
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
