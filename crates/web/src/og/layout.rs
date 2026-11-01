use crate::tokens::values::{
    COLOR_LIGHT_BG, COLOR_LIGHT_GRADE_A, COLOR_LIGHT_GRADE_B, COLOR_LIGHT_GRADE_C,
    COLOR_LIGHT_GRADE_D, COLOR_LIGHT_GRADE_F, COLOR_LIGHT_INK, COLOR_LIGHT_LINE, COLOR_LIGHT_MUTED,
};

/// The card's rendering version: both OG ETags include it, so a deploy that changes the card
/// invalidates the ETags clients hold. Bump it with any change to the rendered card.
pub const CARD_RENDERING_VERSION: u32 = 2;

pub const CANVAS_W: u32 = 1200;
pub const CANVAS_H: u32 = 630;

// Left column — grade letter (centered at x=240, vertically centered)
pub const GRADE_FONT_SIZE: f32 = 260.0;
pub const GRADE_CENTER_X: f32 = 240.0;
pub const GRADE_BASELINE_Y: f32 = 400.0;

// Plate (the spectrum of the run, 1104 x 56 px, labels rising above it)
pub const PLATE_X: f32 = 48.0;
pub const PLATE_Y: f32 = 72.0;

// Right column — shared X origin
pub const RIGHT_X: f32 = 500.0;

// Domain (top of right column)
pub const DOMAIN_FONT_SIZE: f32 = 44.0;
pub const DOMAIN_BASELINE_Y: f32 = 175.0;
pub const DOMAIN_MAX_CHARS: usize = 40;
pub const DOMAIN_COLOR: &str = COLOR_LIGHT_INK;

// Horizontal separator between domain and score
pub const SEP_Y: f32 = 212.0;
pub const SEP_X2: f32 = 1140.0;
pub const SEP_COLOR: &str = COLOR_LIGHT_LINE;

// The slot below the separator: the plate's protocol grades sit on this baseline
pub const SCORE_BASELINE_Y: f32 = 385.0;

// The run's protocol grades (`DNS A+ · TLS A+ · …`), in the score's slot; five entries fit the
// 640 px between RIGHT_X and SEP_X2.
pub const GRADES_FONT_SIZE: f32 = 22.0;

// Timestamp ("May 8, 2026 · 17:44 UTC")
pub const TIMESTAMP_FONT_SIZE: f32 = 26.0;
pub const TIMESTAMP_BASELINE_Y: f32 = 495.0;

// Footer (bottom right)
pub const FOOTER_FONT_SIZE: f32 = 30.0;
pub const FOOTER_BASELINE_Y: f32 = 560.0;
pub const FOOTER_COLOR: &str = COLOR_LIGHT_MUTED;

// Shared colors
pub const BG_COLOR: &str = COLOR_LIGHT_BG;
pub const UNKNOWN_GRADE_COLOR: &str = COLOR_LIGHT_MUTED;

/// The grade letter's colour on the light card; `UNKNOWN_GRADE_COLOR` for anything else.
pub fn grade_color(grade: &str) -> &'static str {
    match grade {
        "A+" | "A" => COLOR_LIGHT_GRADE_A,
        "B" => COLOR_LIGHT_GRADE_B,
        "C" => COLOR_LIGHT_GRADE_C,
        "D" => COLOR_LIGHT_GRADE_D,
        "F" => COLOR_LIGHT_GRADE_F,
        _ => UNKNOWN_GRADE_COLOR,
    }
}

pub const MAX_LABEL_LEN: usize = 32;
/// The card's label without `?label=`: none, so the footer reads "netray.info".
pub const DEFAULT_LABEL: &str = "";
