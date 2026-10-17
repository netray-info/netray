//! The gone page (V2 spec v2-entries, requirement 4, C4 and C12): a fixed heading, a fixed
//! sentence and the query input, with no retention period, no domain, no ID and no og metas.
use netray_web::entries::render::gone;

/// The markup with the `<style>...</style>` element removed (the inlined CSS holds file names
/// and numbers that are not page content).
fn without_style(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find("<style") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</style>") {
            Some(end) => rest = &rest[start + end + "</style>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Every hostname-looking token (`label.tld`, tld of two or more letters) outside the style element.
fn hostnames(html: &str) -> Vec<String> {
    without_style(html)
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
        .map(|t| t.trim_matches(|c| c == '.' || c == '-'))
        .filter(|t| {
            let Some((head, tld)) = t.rsplit_once('.') else {
                return false;
            };
            !head.is_empty() && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn gone_page_holds_heading_sentence_and_query_input() {
    let html = gone().unwrap();
    assert!(
        html.contains("This entry has expired"),
        "heading in: {html}"
    );
    assert!(
        html.contains("This entry has expired and is no longer stored."),
        "sentence in: {html}"
    );
    assert!(html.contains("<h1"), "a heading element in: {html}");
    assert!(
        html.contains("class=\"query-input\""),
        "query-input in: {html}"
    );
    assert!(html.contains("action=\"/\""), "action in: {html}");
    assert!(html.contains("name=\"d\""), "input name in: {html}");
    assert!(html.contains("Check a domain"), "label in: {html}");
    assert!(
        html.contains("placeholder=\"example.com\""),
        "placeholder in: {html}"
    );
    assert!(html.contains(">Check</button>"), "submit in: {html}");
}

#[test]
fn gone_page_states_no_retention_domain_or_og_meta() {
    let html = gone().unwrap();
    let page = without_style(&html);
    assert!(!page.contains("og:"), "og meta in: {page}");
    assert!(!page.contains("90"), "retention period in: {page}");
    assert!(!page.contains("days"), "retention period in: {page}");
    assert!(!page.contains("/r/"), "entry path or ID in: {page}");
    let found = hostnames(&html);
    assert!(
        !found.is_empty(),
        "scanner must see the placeholder in: {page}"
    );
    assert!(
        found.iter().all(|h| h == "example.com"),
        "domain other than example.com: {found:?}"
    );
}
