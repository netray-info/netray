//! Result page header fidelity (spec v2-result-fidelity, Phase 1, R3 / C3): the hero is a flex row
//! of grade block | title column | actions group, as on the board.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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
use tower::ServiceExt;

const DOMAIN: &str = "example.com";

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.trim().to_lowercase())
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

async fn page() -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 60;
    store
        .record(DOMAIN, &run(), &[], at, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app = router(ResultState::new(store, Arc::new(StubRuns), 300));
    let resp = app
        .oneshot(Request::get(format!("/{DOMAIN}")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn pos(hay: &str, needle: &str) -> usize {
    hay.find(needle)
        .unwrap_or_else(|| panic!("{needle} missing in {hay}"))
}

/// The declarations of the rule whose selector is exactly `selector`, whitespace-collapsed.
fn rule(css: &str, selector: &str) -> String {
    let mut from = 0;
    while let Some(i) = css[from..].find(selector) {
        let at = from + i;
        let rest = &css[at + selector.len()..];
        let before_ok = css[..at]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || c == '}');
        if before_ok && rest.trim_start().starts_with('{') {
            let open = rest.find('{').unwrap();
            let close = rest.find('}').unwrap();
            return rest[open + 1..close]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
        }
        from = at + selector.len();
    }
    panic!("no rule `{selector}` in pages.css");
}

/// The value of declaration `prop` in `decls`, with a `var(--token)` resolved from the token CSS.
fn decl(decls: &str, prop: &str) -> String {
    let key = format!("{prop}:");
    let start = decls
        .split(';')
        .map(str::trim)
        .find_map(|d| d.strip_prefix(&key))
        .unwrap_or_else(|| panic!("`{prop}` missing in `{decls}`"))
        .trim()
        .to_string();
    if let Some(name) = start.strip_prefix("var(").and_then(|v| v.strip_suffix(')')) {
        let def = format!("{name}:");
        let tokens = netray_web::tokens::CSS;
        let at = pos(tokens, &def) + def.len();
        return tokens[at..tokens[at..].find(';').unwrap() + at].trim().to_string();
    }
    start
}

#[tokio::test]
async fn r3_header_is_grade_then_title_column_then_actions() {
    let body = page().await;
    let head = &body[pos(&body, r#"<header id="result-header" class="result-hero">"#)..];
    let head = &head[..pos(head, "</header>")];

    let grade = pos(head, r#"id="result-grade""#);
    let title = pos(head, r#"<div class="result-hero__title">"#);
    let actions = pos(head, r#"<div class="result-hero__actions">"#);
    assert!(grade < title && title < actions, "order in {head}");

    let title_box = &head[title..actions];
    assert!(
        pos(title_box, r#"id="run-line""#) < pos(title_box, "<h1"),
        "run title before h1 in {title_box}"
    );

    let actions_box = &head[actions..];
    let form = pos(actions_box, r#"<form id="recheck""#);
    assert!(form < pos(actions_box, r#"id="recheck-button""#));
    assert!(actions_box.contains(r#"id="result-share""#), "{actions_box}");
    for id in ["result-grade", "run-line", "progress", "result-share", "recheck-button"] {
        assert!(head.contains(&format!(r#"id="{id}""#)), "{id} missing");
    }
}

#[test]
fn r3_hero_styles_are_a_wrapping_flex_row_with_a_column_title() {
    let css = include_str!("../static/pages.css");
    let hero = rule(css, ".result-hero");
    assert_eq!(decl(&hero, "display"), "flex");
    assert_eq!(decl(&hero, "align-items"), "center");
    assert_eq!(decl(&hero, "flex-wrap"), "wrap");
    assert_eq!(decl(&hero, "gap"), "28px");

    let title = rule(css, ".result-hero__title");
    assert_eq!(decl(&title, "flex"), "999 1 360px");
    assert_eq!(decl(&title, "display"), "flex");
    assert_eq!(decl(&title, "flex-direction"), "column");
    assert_eq!(decl(&title, "gap"), "6px");
}
