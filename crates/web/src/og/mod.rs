//! The OG card: its layout, its fonts, its SVG and the PNG rasterization. Copied from lens.

pub mod fonts;
pub mod layout;
pub mod render;

pub use render::{RenderError, svg_for_entry, svg_for_grade, svg_to_png};
