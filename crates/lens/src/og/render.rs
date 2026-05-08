use std::sync::Arc;
use std::time::SystemTime;

use fontdb::Database;

use crate::badge::palette::color_for_grade;
use crate::og::layout::{
    BG_COLOR, CANVAS_H, CANVAS_W, DOMAIN_BASELINE_Y, DOMAIN_COLOR, DOMAIN_FONT_SIZE,
    DOMAIN_MAX_CHARS, FOOTER_BASELINE_Y, FOOTER_COLOR, FOOTER_FONT_SIZE, GRADE_BASELINE_Y,
    GRADE_CENTER_X, GRADE_FONT_SIZE, RIGHT_X, SCORE_BASELINE_Y, SCORE_FONT_SIZE, SEP_COLOR,
    SEP_X2, SEP_Y, TIMESTAMP_BASELINE_Y, TIMESTAMP_FONT_SIZE, UNKNOWN_GRADE_COLOR,
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

/// Format a UTC timestamp as "May 8, 2026 · 17:44 UTC".
pub fn format_utc_timestamp(t: SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let hour = (secs % 86400) / 3600;
    let minute = (secs % 3600) / 60;
    let (year, month, day) = epoch_days_to_ymd(secs / 86400);
    format!(
        "{} {}, {} \u{00b7} {:02}:{:02} UTC",
        MONTHS[(month - 1) as usize],
        day,
        year,
        hour,
        minute
    )
}

fn epoch_days_to_ymd(mut days: u64) -> (u64, u8, u8) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: [u8; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 0u8;
    for (i, &dim) in month_days.iter().enumerate() {
        if days < dim as u64 {
            month = i as u8 + 1;
            break;
        }
        days -= dim as u64;
    }
    (year, month, days as u8 + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn svg_for_grade(
    domain: &str,
    grade: &str,
    label: &str,
    score_pct: f64,
    checked_at: SystemTime,
) -> String {
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

    let score_display = if grade_letter == "?" {
        String::new()
    } else {
        format!("{score_pct:.1}%")
    };

    let timestamp_display = format_utc_timestamp(checked_at);

    let footer_text = if label.is_empty() {
        xml_escape(" \u{00b7} netray.info")
    } else {
        format!("{} \u{00b7} netray.info", xml_escape(label))
    };

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{CANVAS_W}" height="{CANVAS_H}">
  <rect width="{CANVAS_W}" height="{CANVAS_H}" fill="{BG_COLOR}"/>
  <text x="{GRADE_CENTER_X}" y="{GRADE_BASELINE_Y}" text-anchor="middle" font-family="Inter" font-size="{GRADE_FONT_SIZE}" font-weight="700" fill="{grade_color}">{grade_display}</text>
  <text x="{RIGHT_X}" y="{DOMAIN_BASELINE_Y}" font-family="Inter" font-size="{DOMAIN_FONT_SIZE}" font-weight="400" fill="{DOMAIN_COLOR}">{domain_display}</text>
  <line x1="{RIGHT_X}" y1="{SEP_Y}" x2="{SEP_X2}" y2="{SEP_Y}" stroke="{SEP_COLOR}" stroke-width="2"/>
  <text x="{RIGHT_X}" y="{SCORE_BASELINE_Y}" font-family="Inter" font-size="{SCORE_FONT_SIZE}" font-weight="700" fill="{grade_color}">{score_display}</text>
  <text x="{RIGHT_X}" y="{TIMESTAMP_BASELINE_Y}" font-family="Inter" font-size="{TIMESTAMP_FONT_SIZE}" font-weight="400" fill="{FOOTER_COLOR}">{timestamp_display}</text>
  <text x="{RIGHT_X}" y="{FOOTER_BASELINE_Y}" font-family="Inter" font-size="{FOOTER_FONT_SIZE}" font-weight="400" fill="{FOOTER_COLOR}">{footer_text}</text>
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

    fn ts() -> SystemTime {
        // 2026-05-08 17:44:00 UTC → unix 1778262240
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_778_262_240)
    }

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
    fn format_utc_timestamp_known_value() {
        let result = format_utc_timestamp(ts());
        assert_eq!(result, "May 8, 2026 \u{00b7} 17:44 UTC");
    }

    #[test]
    fn format_utc_timestamp_epoch() {
        let result = format_utc_timestamp(SystemTime::UNIX_EPOCH);
        assert_eq!(result, "Jan 1, 1970 \u{00b7} 00:00 UTC");
    }

    #[test]
    fn svg_contains_grade_letter() {
        for grade in &["A+", "A", "B", "C", "D", "F"] {
            let svg = svg_for_grade("example.com", grade, "lens", 80.0, ts());
            assert!(
                svg.contains(grade),
                "SVG for grade {grade} should contain the grade letter"
            );
        }
    }

    #[test]
    fn svg_error_grade_renders_question_mark() {
        let svg = svg_for_grade("example.com", "error", "lens", 0.0, ts());
        assert!(svg.contains(">?<"), "error grade should render as ?");
    }

    #[test]
    fn svg_unknown_grade_renders_question_mark() {
        let svg = svg_for_grade("example.com", "X", "lens", 0.0, ts());
        assert!(svg.contains(">?<"), "unknown grade should render as ?");
    }

    #[test]
    fn svg_domain_is_escaped() {
        let svg = svg_for_grade("a&b.com", "A", "lens", 80.0, ts());
        assert!(svg.contains("a&amp;b.com"));
        assert!(!svg.contains("a&b.com"));
    }

    #[test]
    fn svg_label_is_escaped() {
        let svg = svg_for_grade("example.com", "A", "<test>", 80.0, ts());
        assert!(svg.contains("&lt;test&gt;"));
    }

    #[test]
    fn svg_empty_label_renders_bullet_only() {
        let svg = svg_for_grade("example.com", "A", "", 80.0, ts());
        assert!(svg.contains("\u{00b7} netray.info"));
    }

    #[test]
    fn svg_dimensions_present() {
        let svg = svg_for_grade("example.com", "A", "lens", 80.0, ts());
        assert!(svg.contains("width=\"1200\""));
        assert!(svg.contains("height=\"630\""));
    }

    #[test]
    fn svg_score_shown_for_known_grade() {
        let svg = svg_for_grade("example.com", "A", "lens", 91.7, ts());
        assert!(svg.contains("91.7%"), "score percentage should appear in SVG");
    }

    #[test]
    fn svg_score_hidden_for_unknown_grade() {
        let svg = svg_for_grade("example.com", "?", "lens", 0.0, ts());
        assert!(!svg.contains('%'), "score % must not appear for unknown grade");
    }

    #[test]
    fn svg_timestamp_present() {
        let svg = svg_for_grade("example.com", "A", "lens", 80.0, ts());
        assert!(svg.contains("May 8, 2026"), "timestamp date must appear in SVG");
        assert!(svg.contains("17:44 UTC"), "timestamp time must appear in SVG");
    }
}
