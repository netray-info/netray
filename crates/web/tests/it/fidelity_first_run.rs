//! Result fidelity, Phase 3 (spec v2-result-fidelity; C8 first-run box styles, C18 first-run markup).
use netray_web::tokens::CSS;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use std::net::IpAddr;
use tower::ServiceExt;

const DOMAIN: &str = "example.com";
const BOX_OPEN: &str = r#"<section id="diff" class="first-run">"#;
const TITLE: &str = r#"<h2 class="first-run__title">First run recorded.</h2>"#;
const TEXT_OPEN: &str = r#"<p class="first-run__text">"#;
const SENTENCE: &str = "This is the first spectrum of example.com. Re-check after your next change and netray stacks the runs into a history: a line that turns neutral is a fix, a line that lights up is a regression.";
const GHOST_OPEN: &str = r#"<div class="first-run__ghost" aria-hidden="true">"#;
const ROW_OPEN: &str = r#"<div class="first-run__row">"#;
const CELL_OPEN: &str = r#"<span class="first-run__cell" data-tone=""#;
const DASHED: &str = r#"<div class="first-run__dashed">"#;

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn rules(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        out.push((
            squash(&rest[..open]),
            rest[open + 1..open + close].to_string(),
        ));
        rest = &rest[open + close + 1..];
    }
    out
}

fn raw(css: &str, selector: &str, prop: &str) -> Option<String> {
    let wanted = squash(selector);
    let mut found = None;
    for (prelude, body) in rules(css) {
        if prelude.split(',').any(|s| s == wanted) {
            for decl in body.split(';') {
                if let Some((k, v)) = decl.split_once(':')
                    && k.trim() == prop
                {
                    found = Some(v.trim().to_string());
                }
            }
        }
    }
    found
}

fn root() -> Vec<(String, String)> {
    rules(CSS)
        .into_iter()
        .filter(|(p, _)| p == ":root")
        .flat_map(|(_, body)| {
            body.split(';')
                .filter_map(|d| d.split_once(':'))
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn resolve_vars(value: &str) -> String {
    let root = root();
    let mut v = value.to_string();
    for _ in 0..5 {
        let Some(start) = v.find("var(--") else { break };
        let end = start + v[start..].find(')').expect("unclosed var(");
        let name = &v[start + 4..end];
        let token = root
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, val)| val.clone())
            .unwrap_or_else(|| format!("<unknown {name}>"));
        v.replace_range(start..=end, &token);
    }
    v
}

fn all_static_css() -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let mut out = String::new();
    for e in fs::read_dir(dir).unwrap().flatten() {
        if e.path().extension().is_some_and(|x| x == "css") {
            out.push_str(&fs::read_to_string(e.path()).unwrap());
            out.push('\n');
        }
    }
    out
}

#[test]
fn fidelity_first_run_box_styles_resolve_to_the_board_values() {
    let css = all_static_css();
    let get = |sel: &str, prop: &str| {
        raw(&css, sel, prop)
            .map(|v| resolve_vars(&v))
            .unwrap_or_else(|| "<missing>".into())
    };
    let rows: Vec<(&str, String, &str)> = vec![
        (
            ".first-run border",
            get(".first-run", "border"),
            "1px solid #e0e3e8",
        ),
        (
            ".first-run radius",
            get(".first-run", "border-radius"),
            "2px",
        ),
        (
            ".first-run background",
            get(".first-run", "background"),
            "#ffffff",
        ),
        (
            ".first-run padding",
            get(".first-run", "padding"),
            "20px 22px",
        ),
        (
            ".first-run__title font-size",
            get(".first-run__title", "font-size"),
            "17px",
        ),
        (
            ".first-run__title font-weight",
            get(".first-run__title", "font-weight"),
            "650",
        ),
        (
            ".first-run__ghost width",
            get(".first-run__ghost", "width"),
            "240px",
        ),
        (
            ".first-run__dashed border-style",
            get(".first-run__dashed", "border"),
            "1px dashed #e0e3e8",
        ),
        (
            ".first-run__dashed height",
            get(".first-run__dashed", "height"),
            "10px",
        ),
    ];
    let bad: Vec<String> = rows
        .iter()
        .filter(|(_, got, want)| got != want)
        .map(|(what, got, want)| format!("{what}: got {got:?}, want {want:?}"))
        .collect();
    assert!(
        bad.is_empty(),
        "{} rows differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
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
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

#[tokio::test]
async fn fidelity_first_entry_renders_the_first_run_box() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    store
        .record(DOMAIN, &run(), &[], now - 60, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app = router(ResultState::new(store, Arc::new(StubRuns), 300));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body =
        String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned();

    let start = body
        .find(BOX_OPEN)
        .unwrap_or_else(|| panic!("no {BOX_OPEN} in {body}"));
    let end = start + body[start..].find("</section>").expect("unclosed box");
    let boxed = &body[start..end];
    assert!(boxed.contains(TITLE), "title missing: {boxed}");
    assert!(
        boxed.contains(&format!("{TEXT_OPEN}{SENTENCE}</p>")),
        "sentence missing: {boxed}"
    );
    assert!(boxed.contains(GHOST_OPEN), "ghost missing: {boxed}");
    assert_eq!(boxed.matches(ROW_OPEN).count(), 1, "rows: {boxed}");
    assert_eq!(boxed.matches(DASHED).count(), 2, "dashed rows: {boxed}");

    let lines = body.matches(r#"class="spectrum__target""#).count();
    assert!(lines > 0, "no spectrum lines in {body}");
    assert_eq!(boxed.matches(CELL_OPEN).count(), lines, "cells: {boxed}");

    assert!(!body.contains("Since run"), "since heading remains");
    assert!(
        !body.contains(r#"class="diff-block""#),
        "diff-block remains"
    );
}
