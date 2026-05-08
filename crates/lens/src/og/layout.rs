pub const CANVAS_W: u32 = 1200;
pub const CANVAS_H: u32 = 630;

// Left column — grade letter (centered at x=240, vertically centered)
pub const GRADE_FONT_SIZE: f32 = 260.0;
pub const GRADE_CENTER_X: f32 = 240.0;
pub const GRADE_BASELINE_Y: f32 = 400.0;

// Right column — shared X origin
pub const RIGHT_X: f32 = 500.0;

// Domain (top of right column)
pub const DOMAIN_FONT_SIZE: f32 = 44.0;
pub const DOMAIN_BASELINE_Y: f32 = 175.0;
pub const DOMAIN_MAX_CHARS: usize = 40;
pub const DOMAIN_COLOR: &str = "#dce6f5";

// Horizontal separator between domain and score
pub const SEP_Y: f32 = 212.0;
pub const SEP_X2: f32 = 1140.0;
pub const SEP_COLOR: &str = "#1a2740";

// Score percentage (large, grade-colored)
pub const SCORE_FONT_SIZE: f32 = 130.0;
pub const SCORE_BASELINE_Y: f32 = 385.0;

// Timestamp ("May 8, 2026 · 17:44 UTC")
pub const TIMESTAMP_FONT_SIZE: f32 = 26.0;
pub const TIMESTAMP_BASELINE_Y: f32 = 495.0;

// Footer (bottom right)
pub const FOOTER_FONT_SIZE: f32 = 30.0;
pub const FOOTER_BASELINE_Y: f32 = 560.0;
pub const FOOTER_COLOR: &str = "#4d6480";

// Shared colors
pub const BG_COLOR: &str = "#0d1220";
pub const UNKNOWN_GRADE_COLOR: &str = "#4d6480";

pub const MAX_LABEL_LEN: usize = 32;
pub const DEFAULT_LABEL: &str = "lens";
pub const OG_TTL_SECONDS: u64 = 3600;
