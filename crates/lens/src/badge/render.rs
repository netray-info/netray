use super::BadgeStyle;
use super::palette::color_for_grade;

pub fn width_px(text_len: usize, style: BadgeStyle) -> u32 {
    match style {
        BadgeStyle::Flat => (text_len * 7 + 10) as u32,
        BadgeStyle::ForTheBadge => (text_len * 8 + 16) as u32,
    }
}

pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

pub fn svg_for_grade(label: &str, grade: &str, style: BadgeStyle) -> String {
    let value_text = if grade == "error" || grade.is_empty() {
        "?"
    } else {
        grade
    };
    let value_color = color_for_grade(grade);

    match style {
        BadgeStyle::Flat => {
            let label_width = width_px(label.len(), BadgeStyle::Flat);
            let value_width = width_px(value_text.len(), BadgeStyle::Flat);
            let total_width = label_width + value_width;
            let label_cx = label_width / 2;
            let value_cx = label_width + value_width / 2;
            let label_escaped = escape_xml(label);
            let value_escaped = escape_xml(value_text);

            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_width}" height="20">
  <linearGradient id="s" x2="0" y2="100%">
    <stop offset="0" stop-color="#bbb" stop-opacity=".1"/>
    <stop offset="1" stop-opacity=".1"/>
  </linearGradient>
  <rect rx="3" width="{total_width}" height="20" fill="#555"/>
  <rect rx="3" x="{label_width}" width="{value_width}" height="20" fill="{value_color}"/>
  <rect x="{label_width}" width="4" height="20" fill="{value_color}"/>
  <rect rx="3" width="4" height="20" fill="#555"/>
  <rect width="{total_width}" height="20" fill="url(#s)"/>
  <g fill="#fff" text-anchor="middle" font-family="Verdana,Geneva,sans-serif" font-size="11">
    <text x="{label_cx}" y="15" fill="#010101" fill-opacity=".3">{label_escaped}</text>
    <text x="{label_cx}" y="14">{label_escaped}</text>
    <text x="{value_cx}" y="15" fill="#010101" fill-opacity=".3">{value_escaped}</text>
    <text x="{value_cx}" y="14">{value_escaped}</text>
  </g>
</svg>"##
            )
        }

        BadgeStyle::ForTheBadge => {
            let label_upper = label.to_uppercase();
            let value_upper = value_text.to_uppercase();
            let label_width = width_px(label_upper.len(), BadgeStyle::ForTheBadge);
            let value_width = width_px(value_upper.len(), BadgeStyle::ForTheBadge);
            let total_width = label_width + value_width;
            let label_cx = label_width / 2;
            let value_cx = label_width + value_width / 2;
            let label_upper_escaped = escape_xml(&label_upper);
            let value_upper_escaped = escape_xml(&value_upper);

            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_width}" height="28">
  <rect width="{total_width}" height="28" fill="#555"/>
  <rect x="{label_width}" width="{value_width}" height="28" fill="{value_color}"/>
  <g fill="#fff" text-anchor="middle" font-family="Verdana,Geneva,sans-serif"
     font-size="11" font-weight="bold" letter-spacing="1">
    <text x="{label_cx}" y="19">{label_upper_escaped}</text>
    <text x="{value_cx}" y="19">{value_upper_escaped}</text>
  </g>
</svg>"##
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_width_formula() {
        assert_eq!(width_px(4, BadgeStyle::Flat), 38); // 4*7+10
        assert_eq!(width_px(1, BadgeStyle::Flat), 17); // 1*7+10
        assert_eq!(width_px(0, BadgeStyle::Flat), 10);
    }

    #[test]
    fn for_the_badge_width_formula() {
        assert_eq!(width_px(4, BadgeStyle::ForTheBadge), 48); // 4*8+16
        assert_eq!(width_px(2, BadgeStyle::ForTheBadge), 32); // 2*8+16
    }

    #[test]
    fn escape_xml_all_entities() {
        assert_eq!(escape_xml("a&b"), "a&amp;b");
        assert_eq!(escape_xml("<script>"), "&lt;script&gt;");
        assert_eq!(escape_xml("\"hello\""), "&quot;hello&quot;");
        assert_eq!(escape_xml("it's"), "it&#39;s");
        assert_eq!(escape_xml("clean"), "clean");
    }

    #[test]
    fn escape_xml_ampersand_and_less_than() {
        let s = escape_xml("a&b");
        assert!(!s.contains("&b"), "bare & must be escaped");
        assert!(s.contains("&amp;b"));
    }

    #[test]
    fn flat_svg_has_height_20() {
        let svg = svg_for_grade("lens", "A", BadgeStyle::Flat);
        assert!(svg.contains("height=\"20\""), "flat badge must have height=20");
    }

    #[test]
    fn for_the_badge_svg_has_height_28() {
        let svg = svg_for_grade("lens", "A", BadgeStyle::ForTheBadge);
        assert!(svg.contains("height=\"28\""), "for-the-badge must have height=28");
    }

    #[test]
    fn error_grade_renders_question_mark() {
        let svg = svg_for_grade("lens", "error", BadgeStyle::Flat);
        assert!(svg.contains(">?<"), "error grade must render ?");
        assert!(!svg.contains(">error<"));
    }

    #[test]
    fn label_is_escaped_in_output() {
        let svg = svg_for_grade("a&b", "A", BadgeStyle::Flat);
        assert!(svg.contains("a&amp;b"), "label must be HTML-escaped");
        assert!(!svg.contains("a&b"), "bare & must not appear in SVG");
    }
}
