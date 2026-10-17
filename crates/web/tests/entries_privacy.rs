//! V2 spec v2-entries, requirement 5 (C5, C13): the privacy page states what the history stores,
//! how long it keeps it, and that limiters and the throttle live in memory only (SDD §4.7).
use netray_web::entries::render::privacy;

/// The visible text of a page: `<style>`/`<script>` elements dropped, tags replaced by a space,
/// whitespace collapsed, lowercased.
fn visible_text(html: &str) -> String {
    let mut s = html.to_string();
    for tag in ["style", "script"] {
        let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
        while let Some(start) = s.find(&open) {
            let end = s[start..]
                .find(&close)
                .map(|i| start + i + close.len())
                .expect("unclosed element");
            s.replace_range(start..end, " ");
        }
    }
    let mut text = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                text.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[test]
fn privacy_page_states_the_history_statements_and_nothing_foreign() {
    let html = privacy().expect("privacy renders");
    let text = visible_text(&html);

    for phrase in [
        "domain",
        "check results",
        "evidence",
        "nothing about",
        "visitor",
        "90 days",
        "last seen",
        "predecessor",
        "deleted",
        "in memory",
        "rate limit",
        "throttle",
        "disk",
    ] {
        assert!(
            text.contains(phrase),
            "privacy text lacks {phrase:?}: {text}"
        );
    }

    for word in ["cookie", "geolite2"] {
        assert!(
            !text.contains(word),
            "privacy text mentions {word:?}: {text}"
        );
    }
    let has_log = text
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w == "log");
    assert!(!has_log, "privacy text contains the word \"log\": {text}");
}
