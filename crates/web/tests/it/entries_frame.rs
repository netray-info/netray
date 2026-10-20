//! Contract tests for the page frame (V2 spec v2-entries, requirement 1, criteria C1 and C11):
//! `templates/base.html` and the three pages that override its blocks.
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::Entry;
use netray_web::entries::render;
use netray_web::entries::view::entry_view;
use netray_web::tokens;

const TAGLINE: &str = "Every check of a domain, in one spectrum.";
const COMPONENT_CSS: [&str; 10] = [
    "grade",
    "run_line",
    "progress_ray",
    "spectrum",
    "trace",
    "check_row",
    "evidence_block",
    "diff_block",
    "grouping_switch",
    "query_input",
];

fn crate_file(rel: &str) -> String {
    let path = format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn entry() -> Entry {
    Entry {
        id: "aB3dE5gH".into(),
        domain: "example.com".into(),
        created_at: 1_778_262_240,
        last_seen_at: 1_778_262_240,
        predecessor_id: None,
        profile_version: 3,
        grade: Grade::A,
        fingerprint: "f".into(),
        run: Run {
            profile_version: 3,
            engine_version: "0.30.0".into(),
            evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
            grade: Grade::A,
            complete: true,
            sections: vec![RunSection {
                protocol: Protocol::Dns,
                outcome: SectionKind::Measured,
                reason: None,
            }],
            checks: vec![RunCheck {
                id: CheckId::parse("dns.dnssec_valid").unwrap(),
                status: Status::Fail,
                findings: vec![],
                evidence: vec![],
            }],
            blocks: vec![],
        },
        diff: None,
        engine_version: "0.30.0".into(),
        duration_ms: None,
    }
}

fn pages() -> Vec<(&'static str, String)> {
    vec![
        (
            "entry",
            render::entry(&entry_view(&entry())).expect("render entry"),
        ),
        ("gone", render::gone().expect("render gone")),
        ("privacy", render::privacy().expect("render privacy")),
    ]
}

fn inlined_pages() -> Vec<(&'static str, String)> {
    pages()
        .into_iter()
        .filter(|(page, _)| *page != "entry")
        .collect()
}

#[test]
fn base_template_declares_the_four_blocks_in_frame_order() {
    let base = crate_file("templates/base.html");
    let pos = |needle: &str| {
        base.find(needle)
            .unwrap_or_else(|| panic!("base.html lacks {needle}"))
    };
    let styles = pos("{% block styles %}");
    let header = pos("{% block header %}");
    let main = pos("{% block main %}");
    let footer = pos("<footer");
    let scripts = pos("{% block scripts %}");
    assert!(
        styles < header,
        "styles block belongs in the head, before header"
    );
    assert!(header < main, "header before main");
    assert!(main < footer, "main before footer");
    assert!(footer < scripts, "footer before scripts");
    assert!(
        base.starts_with("<!DOCTYPE html>") || base.trim_start().starts_with("<!DOCTYPE html>")
    );
    assert!(base.contains("<html lang=\"en\">"));
    assert!(base.contains("charset"));
    assert!(base.contains("viewport"));
    assert!(base.contains("<title>"));
    assert!(base.contains("/privacy"));
}

#[test]
fn pages_inline_tokens_base_and_component_css_in_order_in_one_style_element() {
    let mut sources: Vec<(String, String)> = vec![
        ("tokens".into(), tokens::CSS.trim().to_string()),
        (
            "base.css".into(),
            crate_file("static/base.css").trim().to_string(),
        ),
    ];
    for name in COMPONENT_CSS {
        sources.push((
            format!("{name}.css"),
            crate_file(&format!("static/{name}.css")).trim().to_string(),
        ));
    }
    for (page, html) in inlined_pages() {
        let start = html
            .find("<style")
            .unwrap_or_else(|| panic!("{page}: no <style>"));
        let end = html
            .find("</style>")
            .unwrap_or_else(|| panic!("{page}: no </style>"));
        let style = &html[start..end];
        let mut at = 0;
        for (name, css) in &sources {
            let found = style[at..]
                .find(css.as_str())
                .unwrap_or_else(|| panic!("{page}: {name} missing or out of order in <style>"));
            at += found + css.len();
        }
    }
}

#[test]
fn pages_are_self_contained_documents_with_a_privacy_footer() {
    for (page, html) in pages() {
        assert!(html.starts_with("<!DOCTYPE html>"), "{page}: doctype first");
        assert!(html.contains("<html lang=\"en\">"), "{page}: html lang");
        let f0 = html
            .find("<footer")
            .unwrap_or_else(|| panic!("{page}: no footer"));
        let f1 = html
            .find("</footer>")
            .unwrap_or_else(|| panic!("{page}: no </footer>"));
        let footer = &html[f0..f1];
        assert!(
            footer.contains("href=\"/privacy\""),
            "{page}: footer links /privacy"
        );
        assert!(footer.contains(TAGLINE), "{page}: footer holds the tagline");
    }
    for (page, html) in inlined_pages() {
        assert_eq!(
            html.matches("<style").count(),
            1,
            "{page}: exactly one <style>"
        );
        let style_start = html.find("<style").unwrap();
        let style = &html[style_start..html.find("</style>").unwrap()];
        assert!(style.contains("--"), "{page}: a tokens custom property");
        assert!(style.contains(".check-row"), "{page}: the .check-row rule");
        assert!(!html.contains("<script"), "{page}: no script");
        assert!(!html.contains("/static/"), "{page}: no /static/ link");
        let f0 = html.find("<footer").expect("footer");
        let f1 = html.find("</footer>").expect("footer end");
        assert!(
            !html[..f0].contains("<nav") && !html[f1..].contains("<nav"),
            "{page}: no navigation outside the frame's footer"
        );
    }
}

#[test]
fn pages_override_header_and_scripts_with_nothing() {
    for page in ["gone", "privacy"] {
        let src = crate_file(&format!("templates/pages/{page}.html"));
        for block in ["header", "scripts"] {
            let empty = format!("{{% block {block} %}}{{% endblock %}}");
            assert!(
                src.contains(&empty),
                "{page}.html does not override {block} with nothing"
            );
        }
    }
}
