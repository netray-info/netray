//! Every catalogue guide link points at a listed guide (v2-pages requirement 19, criteria C3, C9).

use netray_web::pages::guides;

/// `"<check id>: <path>"` for every path that is not `/guide/<slug>` of a listed guide.
fn unlisted(links: &[(&str, &str)]) -> Vec<String> {
    links
        .iter()
        .filter(|(_, path)| {
            !path
                .strip_prefix("/guide/")
                .is_some_and(|slug| guides::find(slug).is_some())
        })
        .map(|(id, path)| format!("{id}: {path}"))
        .collect()
}

fn catalogue_links() -> Vec<(String, String)> {
    netray_engine::catalogue()
        .entries()
        .iter()
        .map(|c| (c.id.to_string(), c.guide.clone()))
        .collect()
}

#[test]
fn every_catalogue_guide_link_is_a_listed_guide() {
    let owned = catalogue_links();
    let links: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert!(!links.is_empty(), "the catalogue lists no checks");
    let bad = unlisted(&links);
    assert!(
        bad.is_empty(),
        "guide links to unlisted guides:\n{}",
        bad.join("\n")
    );
}

#[test]
fn a_planted_link_to_a_missing_guide_is_reported() {
    let owned = catalogue_links();
    let mut links: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    links.push(("dns.planted", "/guide/missing"));
    let bad = unlisted(&links);
    assert_eq!(bad, vec!["dns.planted: /guide/missing".to_string()]);
}
