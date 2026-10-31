//! Ledger's fixed measures, from `project/app.css`: spacing, radii, strokes and type.

use egui::{CornerRadius, FontId};

use crate::ui_kit::fonts::Face;

pub const ITEM_GAP: f32 = 6.0;
pub const ROW_GAP: f32 = 8.0;
pub const INSET: i8 = 14;
pub const GUTTER: i8 = 12;

pub const SECTION_RULE: f32 = 2.0;
pub const HAIRLINE: f32 = 1.0;

pub const HEADER_PADDING: i8 = 8;
pub const BODY_PADDING: i8 = 12;
pub const ROW_PADDING: i8 = 9;
pub const PICK_ROW_HEIGHT: f32 = 34.0;
pub const NOTE_PADDING: (i8, i8) = (14, 10);
pub const CLEAR_PADDING: (i8, i8) = (24, 48);
pub const CODE_PADDING: (i8, i8) = (11, 9);
pub const PROMPT_PADDING: (i8, i8) = (16, 14);
pub const PICK_PADDING: (i8, i8) = (10, 7);

pub const BUTTON_HEIGHT: f32 = 34.0;
pub const BUTTON_PADDING: f32 = 12.0;
pub const BUTTON_GAP: f32 = 6.0;
pub const SMALL_BUTTON_HEIGHT: f32 = 28.0;
pub const SMALL_BUTTON_PADDING: f32 = 8.0;
pub const FOCUS_RING: f32 = 2.0;
pub const FOCUS_OFFSET: f32 = 1.0;

pub const TAG_HEIGHT: f32 = 22.0;
pub const TAG_PADDING: f32 = 7.0;
pub const KEY_SIZE: f32 = 18.0;
pub const KEY_PADDING: f32 = 4.0;

pub const NAV_TAB_HEIGHT: f32 = 38.0;
pub const NAV_TAB_PADDING: f32 = 18.0;
pub const NAV_TAB_GAP: f32 = 3.0;
pub const SEGMENT_HEIGHT: f32 = 34.0;
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
pub const LANE_PADDING: (i8, i8, i8) = (8, 10, 12);
pub const LANE_GAP: f32 = 6.0;

pub const HORIZON_CELL_HEIGHT: f32 = 72.0;
pub const HORIZON_PADDING: (i8, i8, i8) = (7, 7, 9);
pub const CARD_TAB_HEIGHT: f32 = 30.0;
pub const CARD_TAB_PADDING: (i8, i8) = (14, 6);
pub const CARD_TAB_GAP: f32 = 10.0;
pub const DEAL_PADDING: (i8, i8) = (22, 20);
pub const DEAL_GAP: f32 = 16.0;
pub const OFFSET_SHADOW: i8 = 5;
pub const WINDOW_SHADOW: i8 = 6;
pub const STACK_EDGE_HEIGHT: f32 = 5.0;
pub const STACK_EDGE_STEP: f32 = 5.0;
pub const STACK_EDGES_SHOWN: usize = 6;
pub const UNDO_PADDING: (i8, i8) = (14, 8);
pub const UNDO_GAP: f32 = 12.0;

pub const STEP_HEIGHT: f32 = 34.0;
pub const STEP_PADDING: f32 = 14.0;
pub const STEP_GAP: f32 = 3.0;

pub const SHELL_PADDING: (f32, f32, f32) = (28.0, 12.0, 0.0);
pub const SHELL_GAP: (f32, f32) = (20.0, 8.0);
pub const NAV_TOP: f32 = 4.0;
pub const HEAD_PADDING: (f32, f32, f32) = (32.0, 26.0, 6.0);
pub const HEAD_GAP: (f32, f32) = (16.0, 10.0);
pub const HEAD_TITLE_MIN: f32 = 240.0;
pub const SCREEN_PADDING: (f32, f32, f32) = (32.0, 18.0, 32.0);
pub const SCREEN_GAP: f32 = 28.0;
pub const STATUS_PADDING: (f32, f32) = (28.0, 6.0);
pub const STATUS_GAP: (f32, f32) = (18.0, 4.0);
pub const STATUS_DOT: f32 = 8.0;
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
pub const CARD_GRID_MIN: f32 = 150.0;
pub const FLOW_MIN: f32 = 170.0;
pub const NODE_PADDING: (f32, f32, f32) = (16.0, 12.0, 14.0);
pub const NODE_GAP: f32 = 3.0;
pub const NODE_TALLY_HEIGHT: f32 = 31.0;
pub const NODE_TALLY_MARGIN: (f32, f32) = (6.0, 4.0);
pub const NODE_ARROW: (f32, f32) = (14.0, 8.0);
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
pub const HORIZON_CELL_MIN: f32 = 68.0;
pub const HORIZON_DAYS: usize = 14;
pub const GATE_PADDING: f32 = 7.0;
pub const GATE_COLUMN_GAP: f32 = 20.0;
pub const GATE_MIN: f32 = 210.0;
pub const SCRIM_TOP: f32 = 110.0;
pub const SCRIM_OPACITY: f32 = 0.72;
pub const ASK_WIDTH: f32 = 640.0;
pub const ASK_PADDING: f32 = 16.0;
pub const ASK_GAP: f32 = 12.0;
pub const CALL_WIDTH: f32 = 520.0;
pub const CALL_TOP_PADDING: (f32, f32) = (16.0, 10.0);
pub const CALL_BODY_GAP: f32 = 20.0;
pub const TICKET_PADDING: (f32, f32) = (4.0, 9.0);
pub const TICKET_BOX: f32 = 16.0;
pub const CHECK_BOX: f32 = 18.0;
pub const ORDER_PADDING: f32 = 12.0;
pub const ORDER_GAP: (f32, f32) = (16.0, 6.0);
pub const FIELD_GAP: f32 = 4.0;
pub const FIELD_HEIGHT: f32 = 34.0;
pub const FIELD_PADDING: f32 = 8.0;
pub const FIELD_AREA_HEIGHT: f32 = 72.0;
pub const FIELD_TINY_WIDTH: f32 = 92.0;

pub const RADIUS_TAB: u8 = 6;
pub const RADIUS_PANEL: u8 = 4;
pub const RADIUS_BUTTON: u8 = 3;
pub const RADIUS_CARD: u8 = 2;
pub const RADIUS_STAMP: u8 = 1;

pub const TAB_TOP: CornerRadius = CornerRadius { nw: RADIUS_TAB, ne: RADIUS_TAB, sw: 0, se: 0 };
pub const SEGMENT_TOP: CornerRadius = CornerRadius { nw: RADIUS_PANEL, ne: RADIUS_PANEL, sw: 0, se: 0 };
pub const DEAL_CORNERS: CornerRadius = CornerRadius { nw: 0, ne: RADIUS_PANEL, sw: RADIUS_PANEL, se: RADIUS_PANEL };
pub const EDGE_CORNERS: CornerRadius = CornerRadius { nw: 0, ne: 0, sw: RADIUS_BUTTON, se: RADIUS_BUTTON };

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
    pub const SMALL: Self = Self { face: Face::Sans, size: 12.5 };
    pub const NOTE: Self = Self { face: Face::Sans, size: 13.0 };

    pub const BUTTON: Self = Self { face: Face::Medium, size: 14.0 };
    pub const SMALL_BUTTON: Self = Self { face: Face::Medium, size: 13.0 };
    pub const TAB: Self = Self { face: Face::Medium, size: 14.0 };
    pub const TAB_ON: Self = Self { face: Face::Semibold, size: 14.0 };
    pub const STEP: Self = Self { face: Face::Sans, size: 13.0 };
    pub const STEP_DONE: Self = Self { face: Face::Medium, size: 13.0 };
    pub const STEP_NOW: Self = Self { face: Face::Semibold, size: 13.0 };
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
