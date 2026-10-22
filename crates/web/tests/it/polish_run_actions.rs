//! V2 spec v2-ui-polish, requirement 4 (C3, C10, C11): at 480 px the run actions put the back link
//! on its own line and Copy link plus the export buttons in one non-wrapping row; the stored
//! evidence view heads its h1 and its tools row in one flex container.
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing file: {}", path.display()))
}

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = match rest[i..].find("*/") {
            Some(j) => &rest[i + j + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// The matching `}` index for the `{` at `open`.
fn close_of(s: &str, open: usize) -> usize {
    let mut depth = 0;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces");
}

/// Flat (selector list, body) rules of `css`, without descending into at-rules.
fn rules(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(o) = css[pos..].find('{').map(|i| i + pos) {
        let c = close_of(css, o);
        out.push((css[pos..o].trim().to_string(), css[o + 1..c].to_string()));
        pos = c + 1;
    }
    out
}

/// Every rule inside an `@media` block whose condition contains `max-width: 480px`.
fn rules_at_480(css: &str) -> Vec<(String, String)> {
    let css = strip_comments(css);
    rules(&css)
        .into_iter()
        .filter(|(sel, _)| sel.starts_with("@media") && sel.contains("max-width: 480px"))
        .flat_map(|(_, body)| rules(&body))
        .collect()
}

/// The value the media blocks give `prop` on a rule whose selector list holds `selector`.
fn declared(rs: &[(String, String)], selector: &str, prop: &str) -> Option<String> {
    let mut found = None;
    for (sel, body) in rs {
        if !sel.split(',').any(|s| s.trim() == selector) {
            continue;
        }
        for decl in body.split(';') {
            if let Some((k, v)) = decl.split_once(':')
                && k.trim() == prop
            {
                found = Some(v.trim().to_string());
            }
        }
    }
    found
}

fn declared_in(css: &str, selector: &str, prop: &str) -> Option<String> {
    let css = strip_comments(css);
    declared(&rules(&css), selector, prop)
}

#[test]
fn c3_c10_run_actions_at_480px() {
    let rs = rules_at_480(&read("static/inspector_shell.css"));
    let want = [
        (".inspector-backlink", "flex-basis", "100%"),
        (".inspector-export", "flex-wrap", "nowrap"),
        (".inspector-run__actions", "column-gap", "0"),
    ];
    for (selector, prop, value) in want {
        assert_eq!(
            declared(&rs, selector, prop).as_deref(),
            Some(value),
            "{selector} {{ {prop} }} inside @media (max-width: 480px)"
        );
    }
}

fn run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Email,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("email.mx").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

#[tokio::test]
async fn c11_evidence_view_head_wraps_h1_and_tools() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record("example.com", &run(), &[], 1_800_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let app = evidence_router(EvidenceDeps {
        history: Arc::new(store),
    });
    let resp = app
        .oneshot(
            Request::get("/example.com/email")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&bytes).into_owned();

    let head = r#"<div class="evidence-view__head">"#;
    let tools = r#"<div class="evidence-view__tools">"#;
    let at = body
        .find(head)
        .unwrap_or_else(|| panic!("no {head}: {body}"));
    let inner = &body[at + head.len()..];
    assert!(inner.trim_start().starts_with("<h1"), "h1 first: {inner}");
    let h1_end = inner.find("</h1>").expect("h1 closes");
    assert!(
        inner[h1_end + 5..].trim_start().starts_with(tools),
        "tools right after the h1: {inner}"
    );

    let pages = read("static/pages.css");
    assert_eq!(
        declared_in(&pages, ".evidence-view__head", "display").as_deref(),
        Some("flex")
    );
    assert_eq!(
        declared_in(&pages, ".evidence-view__head", "justify-content").as_deref(),
        Some("space-between")
    );
}
