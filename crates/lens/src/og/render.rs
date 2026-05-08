use std::sync::Arc;

use fontdb::Database;

use crate::badge::palette::color_for_grade;
use crate::og::layout::{
    BG_COLOR, CANVAS_H, CANVAS_W, DOMAIN_BASELINE_Y, DOMAIN_COLOR, DOMAIN_FONT_SIZE,
    DOMAIN_MAX_CHARS, DOMAIN_X, FOOTER_BASELINE_Y, FOOTER_COLOR, FOOTER_FONT_SIZE, FOOTER_X,
    GRADE_BASELINE_Y, GRADE_CENTER_X, GRADE_FONT_SIZE, UNKNOWN_GRADE_COLOR,
};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("SVG parse error: {0}")]
    ParseFailed(String),
    #[error("pixmap allocation failed")]
    PixmapAlloc,
    #[error("PNG encode error: {0}")]
    EncodeFailed(String),
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

pub fn truncate_domain(domain: &str) -> String {
    if domain.len() > DOMAIN_MAX_CHARS {
        let truncated: String = domain.chars().take(37).collect();
        format!("{truncated}\u{2026}")
    } else {
        domain.to_owned()
    }
}

pub fn svg_for_grade(domain: &str, grade: &str, label: &str) -> String {
    let grade_letter = match grade {
        "A+" | "A" | "B" | "C" | "D" | "F" => grade,
        _ => "?",
    };
    let grade_color = if grade_letter == "?" {
        UNKNOWN_GRADE_COLOR
    } else {
        color_for_grade(grade)
    };

    let domain_display = xml_escape(&truncate_domain(domain));
    let grade_display = xml_escape(grade_letter);

    let footer_text = if label.is_empty() {
        xml_escape(" \u{00b7} netray.info")
    } else {
        format!("{} \u{00b7} netray.info", xml_escape(label))
    };

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{CANVAS_W}" height="{CANVAS_H}">
  <rect width="{CANVAS_W}" height="{CANVAS_H}" fill="{BG_COLOR}"/>
  <text x="{DOMAIN_X}" y="{DOMAIN_BASELINE_Y}" font-family="Inter" font-size="{DOMAIN_FONT_SIZE}" font-weight="400" fill="{DOMAIN_COLOR}">{domain_display}</text>
  <text x="{GRADE_CENTER_X}" y="{GRADE_BASELINE_Y}" text-anchor="middle" font-family="Inter" font-size="{GRADE_FONT_SIZE}" font-weight="700" fill="{grade_color}">{grade_display}</text>
  <text x="{FOOTER_X}" y="{FOOTER_BASELINE_Y}" font-family="Inter" font-size="{FOOTER_FONT_SIZE}" font-weight="400" fill="{FOOTER_COLOR}">{footer_text}</text>
</svg>"#
    )
}

pub fn svg_to_png(svg: &str, font_db: Arc<Database>) -> Result<Vec<u8>, RenderError> {
    use resvg::{tiny_skia, usvg};

    let opt = usvg::Options {
        fontdb: font_db,
        ..Default::default()
    };
    let tree =
        usvg::Tree::from_str(svg, &opt).map_err(|e| RenderError::ParseFailed(e.to_string()))?;

    let mut pixmap = tiny_skia::Pixmap::new(CANVAS_W, CANVAS_H).ok_or(RenderError::PixmapAlloc)?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    pixmap
        .encode_png()
        .map_err(|e| RenderError::EncodeFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_all_entities() {
        assert_eq!(xml_escape("a&b"), "a&amp;b");
        assert_eq!(xml_escape("<script>"), "&lt;script&gt;");
        assert_eq!(xml_escape("\"hi\""), "&quot;hi&quot;");
        assert_eq!(xml_escape("it's"), "it&apos;s");
        assert_eq!(xml_escape("clean"), "clean");
    }

    #[test]
    fn truncate_domain_short_unchanged() {
        assert_eq!(truncate_domain("example.com"), "example.com");
    }

    #[test]
    fn truncate_domain_exactly_40_unchanged() {
        let s = "a".repeat(40);
        assert_eq!(truncate_domain(&s).len(), 40);
        assert!(!truncate_domain(&s).contains('\u{2026}'));
    }

    #[test]
    fn truncate_domain_41_chars_truncated() {
        let s = "a".repeat(41);
        let result = truncate_domain(&s);
        // 37 chars + ellipsis (3 bytes UTF-8) = 40 chars display, 39 bytes
        assert!(result.ends_with('\u{2026}'));
        let without_ellipsis: String = result.chars().filter(|&c| c != '\u{2026}').collect();
        assert_eq!(without_ellipsis.len(), 37);
    }

    #[test]
    fn truncate_domain_50_chars() {
        let s = "a".repeat(50);
        let result = truncate_domain(&s);
        assert!(result.ends_with('\u{2026}'));
        let char_count = result.chars().count();
        assert_eq!(char_count, 38); // 37 + ellipsis char
    }

    #[test]
    fn svg_contains_grade_letter() {
        for grade in &["A+", "A", "B", "C", "D", "F"] {
            let svg = svg_for_grade("example.com", grade, "lens");
            assert!(
                svg.contains(grade),
                "SVG for grade {grade} should contain the grade letter"
            );
        }
    }

    #[test]
    fn svg_error_grade_renders_question_mark() {
        let svg = svg_for_grade("example.com", "error", "lens");
        assert!(svg.contains(">?<"), "error grade should render as ?");
    }

    #[test]
    fn svg_unknown_grade_renders_question_mark() {
        let svg = svg_for_grade("example.com", "X", "lens");
        assert!(svg.contains(">?<"), "unknown grade should render as ?");
    }

    #[test]
    fn svg_domain_is_escaped() {
        let svg = svg_for_grade("a&b.com", "A", "lens");
        assert!(svg.contains("a&amp;b.com"));
        assert!(!svg.contains("a&b.com"));
    }

    #[test]
    fn svg_label_is_escaped() {
        let svg = svg_for_grade("example.com", "A", "<test>");
        assert!(svg.contains("&lt;test&gt;"));
    }

    #[test]
    fn svg_empty_label_renders_bullet_only() {
        let svg = svg_for_grade("example.com", "A", "");
        assert!(svg.contains("\u{00b7} netray.info"));
    }

    #[test]
    fn svg_dimensions_present() {
        let svg = svg_for_grade("example.com", "A", "lens");
        assert!(svg.contains("width=\"1200\""));
        assert!(svg.contains("height=\"630\""));
    }
}
