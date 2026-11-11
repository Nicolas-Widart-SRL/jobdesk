//! Ledger's fixed measures, from `project/app.css`: spacing, radii, strokes and type.

use egui::{CornerRadius, FontId};

use crate::ui_kit::fonts::Face;

pub const ITEM_GAP: f32 = 6.0;
pub const ROW_GAP: f32 = 8.0;
pub const INSET: i8 = 14;
pub const GUTTER: i8 = 12;

pub const HAIRLINE: f32 = 1.0;

pub const HEADER_PADDING: i8 = 8;
pub const BODY_PADDING: i8 = 12;
pub const ROW_PADDING: i8 = 9;
pub const PICK_ROW_HEIGHT: f32 = 34.0;
pub const NOTE_PADDING: (i8, i8) = (14, 10);
pub const CODE_PADDING: (i8, i8) = (11, 9);
pub const PROMPT_PADDING: (i8, i8) = (16, 14);
pub const PICK_PADDING: (i8, i8) = (10, 7);
/// The room above and below a group's title in a rail of pick rows.
pub const RAIL_TITLE_PADDING: (i8, i8) = (10, 3);

pub const BUTTON_HEIGHT: f32 = 34.0;
pub const BUTTON_PADDING: f32 = 12.0;
pub const BUTTON_GAP: f32 = 6.0;
pub const SMALL_BUTTON_HEIGHT: f32 = 28.0;
pub const SMALL_BUTTON_PADDING: f32 = 8.0;
pub const TOGGLE_GAP: f32 = 4.0;
pub const FOCUS_RING: f32 = 2.0;
pub const FOCUS_OFFSET: f32 = 1.0;
/// The least height of anything that takes a click, so a link's text is never the whole target.
pub const MIN_HIT: f32 = 24.0;

pub const TAG_HEIGHT: f32 = 22.0;
pub const TAG_PADDING: f32 = 7.0;
/// A tag inside a pick row, such as Updated on a topic, is a step smaller, so a long title still fits beside it.
pub const ROW_TAG_HEIGHT: f32 = 20.0;
pub const ROW_TAG_PADDING: f32 = 6.0;
pub const KEY_SIZE: f32 = 18.0;
pub const KEY_PADDING: f32 = 4.0;

pub const NAV_TAB_HEIGHT: f32 = 38.0;
pub const NAV_TAB_PADDING: f32 = 10.0;
pub const NAV_TAB_GAP: f32 = 6.0;
/// The highlighter under the open tab: its height, the rule along its bottom, and how far it reaches past the label.
pub const MARK_HEIGHT: f32 = 14.0;
pub const MARK_RULE: f32 = 2.0;
pub const MARK_OVERHANG: f32 = 6.0;
/// The line box of a tab's label, centred in the tab. The swipe's bottom sits 1px under it.
pub const MARK_LINE: f32 = 20.0;
/// A search hit's swipe: how far it reaches past the matched glyphs, and where it starts down the line, as a share of it.
pub const HIT_OVERHANG: f32 = 2.0;
pub const HIT_TOP: f32 = 0.45;
pub const SEGMENT_HEIGHT: f32 = 34.0;
pub const SEGMENT_GAP: f32 = 3.0;
pub const SEGMENT_PADDING: f32 = 12.0;
pub const STATUS_SEGMENT_HEIGHT: f32 = 24.0;
pub const STATUS_SEGMENT_PADDING: f32 = 8.0;

pub const STAMP_HEIGHT: f32 = 50.0;
pub const STAMP_MIN_WIDTH: f32 = 62.0;
pub const STAMP_PADDING: f32 = 8.0;
pub const STAMP_RULE_GAP: f32 = 3.0;
pub const TALLY_GAP_Y: f32 = 3.0;
pub const TALLY_THIN: TallySize = TallySize { width: 3.0, height: 14.0, gap: 2.0 };
pub const TALLY_FAT: TallySize = TallySize { width: 6.0, height: 18.0, gap: 3.0 };
pub const BAR_HEIGHT: f32 = 8.0;

pub const CARD_PADDING: (i8, i8) = (11, 9);
/// The room between grey lines while a screen loads.
pub const SKELETON_GAP: f32 = 8.0;
/// A grey line's height while a screen loads.
pub const SKELETON_LINE: f32 = 12.0;
pub const LANE_PADDING: (i8, i8, i8) = (8, 10, 12);
pub const LANE_GAP: f32 = 6.0;

pub const CHIP_HEIGHT: f32 = 20.0;
pub const CHIP_PADDING: f32 = 6.0;
pub const CARD_TAB_HEIGHT: f32 = 30.0;
pub const CARD_TAB_PADDING: (i8, i8) = (14, 6);
pub const CARD_TAB_GAP: f32 = 10.0;
pub const DEAL_PADDING: (i8, i8) = (22, 20);
pub const DEAL_GAP: f32 = 16.0;
pub const DEAL_FOOT_PADDING: (i8, i8) = (22, 12);
/// The least width the note beside a card's actions takes before it moves under them.
pub const DEAL_ASIDE_MIN: f32 = 240.0;
/// The day stamp's size, its two ring radii, and a tick's inner radius, outer radius and width.
pub const DAY_STAMP_SIZE: f32 = 84.0;
pub const DAY_STAMP_RINGS: (f32, f32) = (40.4, 28.4);
pub const DAY_STAMP_TICK: (f32, f32, f32) = (31.0, 37.8, 4.7);
pub const DAY_STAMP_EMPTY_TICKS: usize = 12;
pub const DAY_STAMP_MAX_TICKS: usize = 40;
/// The day stamp's size the moment its last tick fills, before it settles to its own.
pub const DAY_STAMP_SETTLE: f32 = 1.04;
/// How far a decided card slides toward the rail. How long it moves for is `motion_time`, which reduced motion zeroes.
pub const MOTION_SLIDE: f32 = 12.0;
/// The live sweep's segment, as a share of its edge's width.
pub const SWEEP_SHARE: f32 = 0.3;
/// The long-quiet edge's dashes: a dash, then a gap.
pub const SWEEP_DASH: (f32, f32) = (6.0, 4.0);
pub const AGENDA_DAY_WIDTH: f32 = 50.0;
pub const AGENDA_TIP_WIDTH: f32 = 250.0;
/// The stamp-ink edge on a card that waits on the user.
pub const WAITS_EDGE: f32 = 3.0;
/// The ink edge at the leading side of a picked row.
pub const PICK_EDGE: f32 = 3.0;
/// The opacity of a card that is decided or set aside.
pub const FADED_OPACITY: f32 = 0.8;
/// The opacity of a choice that is not available yet, such as Cloud providers: fainter than a faded card, as on the canvas.
pub const LATER_OPACITY: f32 = 0.55;
/// How much of its ink and border a disabled control keeps. egui's own fade is off, so the kit paints this.
pub const DISABLED_OPACITY: f32 = 0.5;
/// The room between a step's knot and its words.
pub const KNOT_GAP: f32 = 20.0;
pub const WELCOME_WIDTH: f32 = 620.0;
/// The panel that offers new work when Today is clear: its inset, the width of its text and the gap between its parts.
pub const OFFER_PADDING: i8 = 32;
pub const OFFER_WIDTH: f32 = 480.0;
pub const OFFER_GAP: f32 = 14.0;
/// The empty state of a screen: its inset, the width of its sentence and the gap between its parts.
pub const EMPTY_PADDING: i8 = 24;
pub const EMPTY_WIDTH: f32 = 560.0;
pub const EMPTY_GAP: f32 = 12.0;
/// The welcome card's room above it, its padding, and the padding above and below each step.
pub const WELCOME_TOP: f32 = 36.0;
pub const WELCOME_PADDING: (i8, i8) = (27, 26);
pub const WELCOME_STEP_PADDING: f32 = 16.0;
/// The width of the empty tab over a card that is still loading.
pub const BLANK_TAB_WIDTH: f32 = 180.0;
pub const OFFSET_SHADOW: i8 = 5;
pub const WINDOW_SHADOW: i8 = 6;
pub const STACK_EDGE_HEIGHT: f32 = 5.0;
pub const STACK_EDGE_STEP: f32 = 5.0;
pub const STACK_EDGES_SHOWN: usize = 6;
/// Top and bottom padding of the undo row.
pub const UNDO_ROW_PADDING: (i8, i8) = (10, 12);

/// The run screen's side column.
pub const RUN_SIDE: f32 = 280.0;
pub const STEP_LINE_HEIGHT: f32 = 48.0;
pub const STEP_LINE_GAP: f32 = 12.0;
pub const STEP_NAME_WIDTH: f32 = 74.0;
pub const KNOT_SIZE: f32 = 22.0;
pub const KNOT_SIZE_SMALL: f32 = 18.0;
pub const KNOT_PADDING: f32 = 4.0;
/// Below this window height, each band cell keeps to one line.
pub const BAND_COMPACT_HEIGHT: f32 = 700.0;
/// The band's three cells share its width in these parts.
pub const BAND_COLUMNS: [f32; 3] = [1.0, 1.0, 1.4];
pub const BAND_PADDING: (f32, f32, f32) = (16.0, 10.0, 12.0);
pub const BAND_COMPACT_PADDING: (f32, f32, f32) = (16.0, 9.0, 10.0);
pub const BAND_LABEL_HEIGHT: f32 = 20.0;
pub const BAND_ROW_GAP: f32 = 2.0;
pub const BAND_GAP: f32 = 10.0;
pub const TALLY_BAND: TallySize = TallySize { width: 5.0, height: 12.0, gap: 2.0 };

/// The side of a square on the desk's grid.
pub const GRID_PITCH: f32 = 20.0;
pub const SHELL_PADDING: (f32, f32, f32) = (28.0, 14.0, 4.0);
pub const SHELL_GAP: (f32, f32) = (20.0, 8.0);
pub const HEAD_PADDING: (f32, f32, f32) = (32.0, 26.0, 6.0);
pub const HEAD_GAP: (f32, f32) = (16.0, 10.0);
pub const HEAD_TITLE_MIN: f32 = 240.0;
pub const SCREEN_PADDING: (f32, f32, f32) = (32.0, 18.0, 32.0);
pub const SCREEN_GAP: f32 = 28.0;
pub const STATUS_PADDING: (f32, f32) = (28.0, 6.0);
pub const STATUS_GAP: (f32, f32) = (18.0, 4.0);
pub const STATUS_DOT: f32 = 8.0;
/// The room between a status mark button's square and its words.
pub const STATUS_MARK_GAP: f32 = 6.0;
/// The hairline between Jobdesk's own group and Settings in the status strip.
pub const STATUS_DIVIDER_HEIGHT: f32 = 16.0;
/// A download's bar in the status strip, and the room either side of it.
pub const STATUS_PROGRESS_BAR: f32 = 120.0;
pub const STATUS_PROGRESS_GAP: f32 = 10.0;
/// The release notes popover: its outer width, its inline padding, its head's padding above and below, the height of
/// the notes, the notes' padding (left, top, right, bottom), the room between its parts, and the ready slip's padding.
pub const NOTES_WIDTH: f32 = 468.0;
pub const NOTES_INLINE: i8 = 18;
pub const NOTES_HEAD_PADDING: (i8, i8) = (14, 12);
pub const NOTES_HEIGHT: f32 = 288.0;
pub const NOTES_BODY_PADDING: (i8, i8, i8, i8) = (16, 4, 22, 12);
pub const NOTES_GAP: f32 = 12.0;
pub const NOTES_SLIP_PADDING: (i8, i8) = (12, 9);
pub const BRAND_MARK_WIDTH: f32 = 26.0;
pub const BRAND_GAP: f32 = 8.0;
pub const KEYS_GAP: f32 = 3.0;
pub const HEADER_GAP: (f32, f32) = (10.0, 2.0);
pub const NOTE_GAP: (f32, f32) = (12.0, 8.0);
pub const LEADING_TAG_WIDTH: f32 = 94.0;
pub const ROW_ITEM_GAP: f32 = 10.0;
pub const PICK_ROW_INLINE: f32 = 8.0;
pub const COMMAND_NAME_WIDTH: f32 = 115.0;
pub const TABLE_HEAD_PADDING: (f32, f32) = (12.0, 8.0);
pub const TABLE_HEAD_HEIGHT: f32 = 36.0;
pub const TABLE_CELL_PADDING: (f32, f32) = (12.0, 9.0);
pub const SPLIT_GAP: f32 = 28.0;
pub const SPLIT_MAIN_BASIS: f32 = 520.0;
pub const SPLIT_SIDE_BASIS: f32 = 340.0;
/// The first-run wizard's card from the Profile chapter on, in the window grown to the app's size.
pub const WIZARD_WIDE: f32 = 1120.0;
/// The room a scrolling column keeps at its right, so its floating bar never runs over the words.
pub const WELL_GUTTER: i8 = 22;
/// The answer column beside Claude's message in the fallback sheet, as a share of the message column's width.
pub const FALLBACK_SHARE: f32 = 0.86;
/// The least width of a numbered item's number in Claude's message, and of a lettered sub-item's letter.
pub const ITEM_NUMBER: f32 = 28.0;
pub const SUB_ITEM_LETTER: f32 = 22.0;
/// The room above a group's title in Claude's message after something else, and under it.
pub const GROUP_GAP: (f32, f32) = (14.0, 6.0);
/// The room under one question with its field, and under a block of Claude's prose.
pub const QUESTION_GAP: f32 = 12.0;
pub const PROSE_GAP: f32 = 10.0;
/// A proposed change's row: the room above and below its words, and between two rows.
pub const CHANGE_PADDING: f32 = 2.0;
pub const CHANGE_GAP: f32 = 4.0;
/// The drop zone, full and slim: its padding (inline, block), and the room between its parts.
pub const DROP_PADDING: (i8, i8) = (26, 28);
pub const DROP_SLIM_PADDING: (i8, i8) = (16, 12);
pub const DROP_GAP: f32 = 20.0;
pub const DROP_SLIM_GAP: f32 = 14.0;
/// The two pages stacked in the full drop zone: each page, and how far the front one sits right of and above the back.
pub const DROP_PAGE: (f32, f32) = (66.0, 92.0);
pub const DROP_PAGE_OFFSET: (f32, f32) = (22.0, 10.0);
/// The page at the start of a row in a file list.
pub const FILE_PAGE: (f32, f32) = (26.0, 34.0);
/// The menu in a file list's row that says what the file is: wide enough for its longest kind.
pub const KIND_MENU_WIDTH: f32 = 140.0;
/// The in-tray's side column. Its slips sit `GUTTER` in from its left edge.
pub const TRAY_RAIL: f32 = 292.0;
/// The room between the slips in the rail.
pub const RAIL_GAP: f32 = 14.0;
pub const CARD_GRID_MIN: f32 = 150.0;
pub const FLOW_MIN: f32 = 170.0;
pub const NODE_PADDING: (f32, f32, f32) = (16.0, 12.0, 14.0);
pub const NODE_GAP: f32 = 3.0;
pub const NODE_TALLY_HEIGHT: f32 = 31.0;
pub const NODE_TALLY_MARGIN: (f32, f32) = (6.0, 4.0);
pub const NODE_ARROW: (f32, f32) = (14.0, 8.0);
/// The room above and below the bar under a figure band's figure.
pub const FIGURE_BAR_MARGIN: (f32, f32) = (4.0, 2.0);
/// A loading figure band cell's grey blocks: the label's width and the figure's width and height,
/// then the room above and below the figure, and the note's width in each cell in turn.
pub const FIGURE_SKELETON: (f32, f32, f32) = (60.0, 110.0, 30.0);
pub const FIGURE_SKELETON_MARGIN: (f32, f32) = (6.0, 4.0);
pub const FIGURE_SKELETON_NOTES: [f32; 4] = [150.0, 120.0, 160.0, 130.0];
/// A day heatmap's column of weekday names and its row of month names.
pub const HEAT_LABELS: (f32, f32) = (30.0, 16.0);
/// The room between a day heatmap and its notes, the notes' room above, and the gap between their lines.
pub const HEAT_SIDE: (f32, f32, f32) = (32.0, 14.0, 5.0);
pub const EVENT_PADDING: f32 = 8.0;
pub const EVENT_GAP: f32 = 10.0;
pub const EVENT_WHO_WIDTH: f32 = 56.0;
pub const EVENT_NESTED: f32 = 24.0;
pub const PROMPT_MARGIN: (f32, f32, f32) = (14.0, 8.0, 18.0);
pub const BOARD_GAP: f32 = 8.0;
pub const BOARD_LANE_MIN: f32 = 210.0;
pub const PAGE_SIZE: (f32, f32) = (178.0, 252.0);
pub const PAGE_PADDING: (f32, f32) = (14.0, 16.0);
pub const PAGE_LINE_HEIGHTS: (f32, f32) = (3.0, 6.0);
pub const PAGE_GAP: f32 = 12.0;
pub const MINI_PAGE_SIZE: (f32, f32) = (92.0, 130.0);
pub const MINI_PAGE_PADDING: (f32, f32) = (7.0, 8.0);
pub const GATE_PADDING: f32 = 7.0;
pub const GATE_COLUMN_GAP: f32 = 20.0;
pub const GATE_MIN: f32 = 210.0;
pub const SCRIM_TOP: f32 = 110.0;
pub const SCRIM_OPACITY: f32 = 0.72;
/// The widest a run of prose gets, so a line stays readable on a wide screen. A document's top-level tables take the
/// whole width instead.
pub const DOC_MEASURE: f32 = 620.0;
pub const ASK_WIDTH: f32 = 640.0;
pub const ASK_PADDING: f32 = 16.0;
pub const ASK_GAP: f32 = 12.0;
pub const CALL_WIDTH: f32 = 520.0;
pub const CALL_TOP_PADDING: (f32, f32) = (16.0, 10.0);
pub const CALL_BODY_GAP: f32 = 20.0;
pub const TICKET_PADDING: (f32, f32) = (4.0, 9.0);
pub const TICKET_BOX: f32 = 16.0;
pub const CHECK_BOX: f32 = 18.0;
/// A radio button's ring, and the dot in it while it is on.
pub const RADIO: f32 = 16.0;
pub const RADIO_DOT: f32 = 6.0;
pub const ORDER_PADDING: f32 = 12.0;
pub const ORDER_GAP: (f32, f32) = (16.0, 6.0);
pub const FIELD_GAP: f32 = 4.0;
pub const FIELD_HEIGHT: f32 = 34.0;
pub const FIELD_PADDING: f32 = 8.0;
pub const FIELD_AREA_HEIGHT: f32 = 72.0;
pub const FIELD_TINY_WIDTH: f32 = 92.0;
/// An amount field set in a line of words, such as the monthly cap's: its width and height, and the room either side.
pub const FIELD_INLINE: (f32, f32) = (96.0, 28.0);
pub const FIELD_INLINE_MARGIN: f32 = 4.0;
/// A whole number set in a line of words, such as the score the Jobs view hides jobs under: its width and height.
pub const FIELD_INLINE_NUMBER: (f32, f32) = (64.0, 28.0);
/// A settings page's head padding (inline, top, bottom), its body padding (inline, top, bottom),
/// and how far its actions sit above the lead's bottom edge.
pub const SETTINGS_HEAD_PADDING: (i8, i8, i8) = (22, 18, 14);
pub const SETTINGS_BODY_PADDING: (i8, i8, i8) = (22, 14, 18);
pub const SETTINGS_ACTIONS_LIFT: f32 = 2.0;
/// The room above a settings section after another, and between its heading or note and its rule.
pub const SETTING_SECTION_GAP: (f32, f32) = (20.0, 6.0);
/// A setting row's padding above and below, its least height, the gap before its control, and the gap after a checkbox.
pub const SETTING_ROW_PADDING: f32 = 10.0;
pub const SETTING_ROW_MIN: f32 = 58.0;
pub const SETTING_CONTROL_GAP: f32 = 32.0;
pub const SETTING_CHECK_GAP: f32 = 12.0;
/// The room between a setting's label and its help, and between a section's heading and its note.
pub const SETTING_HELP_GAP: f32 = 2.0;
pub const SETTING_NOTE_GAP: f32 = 1.0;
/// A role on a profile's paper: the room above and below it, and its dates column, which fits "2021 – present".
pub const ROLE_PADDING: (f32, f32) = (14.0, 12.0);
pub const ROLE_DATES: f32 = COMMAND_NAME_WIDTH;
/// The block editor: its padding, the gap between its parts, and how many lines its field shows before it grows.
pub const EDITOR_PADDING: (i8, i8, i8) = (14, 12, 14);
pub const EDITOR_GAP: f32 = 10.0;
pub const EDITOR_ROWS: usize = 7;
/// Under the warning box of a change on disk, the field takes only the lines of its draft.
pub const EDITOR_ROWS_CHANGED: usize = 1;

pub const RADIUS_TAB: u8 = 6;
pub const RADIUS_PANEL: u8 = 4;
pub const RADIUS_BUTTON: u8 = 3;
pub const RADIUS_CARD: u8 = 2;
pub const RADIUS_STAMP: u8 = 1;

pub const TAB_TOP: CornerRadius = CornerRadius { nw: RADIUS_TAB, ne: RADIUS_TAB, sw: 0, se: 0 };
pub const SEGMENT_TOP: CornerRadius = CornerRadius { nw: RADIUS_PANEL, ne: RADIUS_PANEL, sw: 0, se: 0 };
pub const DEAL_CORNERS: CornerRadius = CornerRadius { nw: 0, ne: RADIUS_PANEL, sw: RADIUS_PANEL, se: RADIUS_PANEL };
pub const EDGE_CORNERS: CornerRadius = CornerRadius { nw: 0, ne: 0, sw: RADIUS_BUTTON, se: RADIUS_BUTTON };
pub const FOOT_CORNERS: CornerRadius = CornerRadius { nw: 0, ne: 0, sw: RADIUS_PANEL, se: RADIUS_PANEL };

/// One mark in a tally.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TallySize {
    pub width: f32,
    pub height: f32,
    pub gap: f32,
}

/// A named type style. Each one is a rule in the design, not a free size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Type {
    pub face: Face,
    pub size: f32,
}

impl Type {
    pub const BODY: Self = Self { face: Face::Sans, size: 14.0 };
    pub const STRONG: Self = Self { face: Face::Semibold, size: 14.0 };
    pub const TAKE: Self = Self { face: Face::Sans, size: 15.0 };
    /// A bold line in Claude's message read at `TAKE`, such as a group of questions' title.
    pub const TAKE_STRONG: Self = Self { face: Face::Semibold, size: 15.0 };
    pub const SMALL: Self = Self { face: Face::Sans, size: 12.5 };
    pub const NOTE: Self = Self { face: Face::Sans, size: 13.0 };

    pub const BUTTON: Self = Self { face: Face::Medium, size: 14.0 };
    pub const SMALL_BUTTON: Self = Self { face: Face::Medium, size: 13.0 };
    pub const TAB: Self = Self { face: Face::Medium, size: 14.0 };
    pub const TAB_ON: Self = Self { face: Face::Semibold, size: 14.0 };
    pub const TAG: Self = Self { face: Face::Semibold, size: 12.5 };
    pub const KEY: Self = Self { face: Face::Semibold, size: 11.5 };
    pub const KEYS: Self = Self { face: Face::Medium, size: 13.0 };
    pub const TABLE_HEAD: Self = Self { face: Face::Semibold, size: 13.0 };

    pub const MONO: Self = Self { face: Face::Typed, size: 13.5 };
    pub const COMMAND: Self = Self { face: Face::TypedBold, size: 18.0 };
    pub const PATH: Self = Self { face: Face::TypedBold, size: 21.0 };
    /// A run's command line as its screen's title, smaller than a path so a long line fits.
    pub const HEAD_COMMAND: Self = Self { face: Face::TypedBold, size: 16.0 };

    pub const BRAND: Self = Self { face: Face::Slab, size: 23.0 };
    pub const H1: Self = Self { face: Face::Slab, size: 32.0 };
    pub const LEAD: Self = Self { face: Face::Slab, size: 44.0 };
    pub const SECTION: Self = Self { face: Face::Slab, size: 17.0 };
    pub const SLIP_TITLE: Self = Self { face: Face::Slab, size: 19.0 };
    pub const SCORE: Self = Self { face: Face::Slab, size: 21.0 };
    /// A band value on its one line, in a short window.
    pub const BAND_LINE: Self = Self { face: Face::Slab, size: 19.0 };
    pub const FIT: Self = Self { face: Face::Slab, size: 16.0 };
    pub const STAMP: Self = Self { face: Face::Slab, size: 32.0 };
    pub const FIGURE: Self = Self { face: Face::Slab, size: 34.0 };
    pub const DEAL: Self = Self { face: Face::Slab, size: 28.0 };
    pub const CLEAR: Self = Self { face: Face::Slab, size: 34.0 };
    pub const CLOCK: Self = Self { face: Face::Slab, size: 26.0 };
    pub const CALL_TITLE: Self = Self { face: Face::Slab, size: 26.0 };

    pub fn font(self) -> FontId {
        FontId::new(self.size, self.face.family())
    }
}
