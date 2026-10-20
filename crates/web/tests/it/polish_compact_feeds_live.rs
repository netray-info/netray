//! Spec v2-result-polish, requirements 2 and 3 (C2, C19, C3, C21): the feeds text of a compact
//! tab names only the lines whose checks reference the table's source blocks, and a live run's
//! streamed panel holds the compact table plus a closed disclosure with every block.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::views::line_code;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const T: i64 = 1_800_000_000;
const CHAIN: &str = "tls.chain@192.0.2.10:443";
const HANDSHAKE: &str = "tls.handshake@192.0.2.10:443";

fn text(v: &str) -> Value {
    Value::Text(v.into())
}

fn chain_block() -> Block {
    Block {
        id: BlockId::parse(CHAIN).unwrap(),
        title: "Certificate chain 192.0.2.10:443".into(),
        shape: Shape::Chain(vec![vec![
            ("position".into(), text("leaf")),
            ("subject".into(), text("CN=shop.example.com")),
            (
                "issuer".into(),
                text("C=US, O=Example CA, CN=Example Issuing CA 1"),
            ),
            (
                "not_after".into(),
                Value::Time("2026-12-25T22:56:35Z".into()),
            ),
            ("key".into(), text("ECDSA P-256 256")),
            ("fingerprint_sha256".into(), text("aa11bb22")),
        ]]),
    }
}

fn handshake_block() -> Block {
    Block {
        id: BlockId::parse(HANDSHAKE).unwrap(),
        title: "Handshake 192.0.2.10:443".into(),
        shape: Shape::Fields(vec![("version".into(), text("TLSv1.3"))]),
    }
}

// ---- stored entry (C2, C19) ----

struct NoRuns;

impl DomainRuns for NoRuns {
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

fn fixture_run() -> Run {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/shop.example.com-3.json");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    serde_json::from_value(v["run"].clone()).unwrap()
}

async fn stored_page(run: &Run) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record(DOMAIN, run, &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app = router(ResultState::new(Arc::new(store), Arc::new(NoRuns), 3600));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn c2_c19_the_feeds_text_names_only_lines_of_the_compact_source_blocks() {
    let body = stored_page(&fixture_run()).await;
    let ev = &body[body.find(r#"id="evidence""#).expect("#evidence missing")..];
    let open = r#"<div id="evidence-tls" class="evidence-tabs__panel" data-panel="tls""#;
    let rest = &ev[ev.find(open).expect("tls panel missing") + open.len()..];
    let panel = &rest[..rest
        .find(r#"class="evidence-tabs__panel""#)
        .unwrap_or(rest.len())];
    let feeds_at = panel
        .find(r#"<p class="evidence-tabs__feeds">"#)
        .unwrap_or_else(|| panic!("no feeds line: {panel}"));
    let feeds = &panel[feeds_at..feeds_at + panel[feeds_at..].find("</p>").unwrap()];
    let code = |id: &str| line_code(&CheckId::parse(id).unwrap()).expect("line code");

    let trusted = format!("<code>{}</code>", code("tls.chain_trusted"));
    assert!(feeds.contains(&trusted), "chain_trusted missing: {feeds}");
    for other in ["tls.tls_version", "tls.ct_logged"] {
        let needle = format!("<code>{}</code>", code(other));
        assert!(
            !feeds.contains(&needle),
            "{other} must not feed the table: {feeds}"
        );
    }
}

// ---- live run (C3, C21) ----

struct StubRuns;

fn tls_check() -> CheckResult {
    CheckResult {
        id: CheckId::parse("tls.chain_trusted").unwrap(),
        status: Status::Pass,
        findings: vec![],
        evidence: vec![
            BlockId::parse(CHAIN).unwrap(),
            BlockId::parse(HANDSHAKE).unwrap(),
        ],
    }
}

fn done() -> Run {
    let check = tls_check();
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: netray_model::evidence::EVIDENCE_SCHEMA_VERSION,
        grade: Grade::Incomplete,
        complete: false,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: check.id,
            status: check.status,
            findings: vec![],
            evidence: check.evidence,
        }],
        blocks: vec![chain_block(), handshake_block()],
    }
}

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
        let events = vec![
            RunEvent::Section(SectionEvent {
                protocol: Protocol::Tls,
                outcome: SectionOutcome::Measured {
                    checks: vec![tls_check()],
                    evidence: vec![chain_block(), handshake_block()],
                },
            }),
            RunEvent::Done(done()),
            RunEvent::Recorded(Some(Recorded::NotStored { diff: None })),
        ];
        Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
    }
}

/// The html of the streamed fragment `id`.
async fn fragment(id: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let app: Router = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 300));
    let resp = app
        .oneshot(
            Request::post(format!("/{DOMAIN}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::from("trigger=visit"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut name = String::new();
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        if name == "fragment" && j["id"].as_str() == Some(id) {
            return j["html"].as_str().unwrap_or_default().to_owned();
        }
    }
    panic!("fragment {id} missing in {text}");
}

#[tokio::test]
async fn c3_c21_a_live_fragment_holds_the_compact_table_and_a_closed_all_evidence_disclosure() {
    let html = fragment("evidence-tls").await;
    let details = r#"<details class="evidence-tabs__blocks"><summary>All evidence</summary>"#;
    let at = html
        .find(details)
        .unwrap_or_else(|| panic!("no closed disclosure: {html}"));
    let (before, inside) = html.split_at(at);
    assert!(
        before.contains(r#"<table class="evidence-tabs__compact">"#),
        "no compact table before the disclosure: {html}"
    );
    assert!(!before.contains(r#"class="evidence-block""#), "{html}");
    assert!(inside.contains(r#"class="evidence-block""#), "{html}");
    assert!(inside.contains("</details>"), "{html}");
    assert!(
        !html.contains(r#"<details class="evidence-tabs__blocks" open"#),
        "{html}"
    );
    assert!(!html.contains("evidence-tabs__all"), "{html}");
}
