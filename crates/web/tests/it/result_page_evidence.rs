//! Result page evidence section (spec v2-result-page, Phase 2, requirement 14; criteria C6, C13):
//! `GET /{domain}` holds one `evidence-<protocol>` element per protocol, without a coverage line
//! (v2-result-fidelity requirement 33), and renders no evidence block itself.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;
const REASON: &str = "no mail exchanger published for this domain";
const BLOCK_TITLE: &str = "Certificate chain of the stored run";

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

fn section(protocol: Protocol, outcome: SectionKind, reason: Option<&str>) -> RunSection {
    RunSection {
        protocol,
        outcome,
        reason: reason.map(String::from),
    }
}

fn run() -> Run {
    let block = BlockId::parse("tls.chain@192.0.2.1:443").unwrap();
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![
            section(Protocol::Dns, SectionKind::Measured, None),
            section(Protocol::Tls, SectionKind::Measured, None),
            section(Protocol::Http, SectionKind::Measured, None),
            section(Protocol::Email, SectionKind::NotApplicable, Some(REASON)),
            section(Protocol::Ip, SectionKind::Measured, None),
        ],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![block.clone()],
        }],
        blocks: vec![Block {
            id: block,
            title: BLOCK_TITLE.into(),
            shape: Shape::Text("example.com issued by Example CA".to_string()),
        }],
    }
}

async fn app_with_entry() -> (Router, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let id = match store
        .record("example.com", &run(), &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    let state = ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600);
    (router(state), id, dir)
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// The markup from the element `id="evidence-<protocol>"` up to the next evidence element or the
/// end of the section.
fn element<'a>(body: &'a str, protocol: &str) -> &'a str {
    let marker = format!("id=\"evidence-{protocol}\"");
    let start = body
        .find(&marker)
        .unwrap_or_else(|| panic!("no {marker} in: {body}"));
    let rest = &body[start + marker.len()..];
    let end = [rest.find("id=\"evidence-"), rest.find("</section>")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(rest.len());
    &rest[..end]
}

#[tokio::test]
async fn c6_evidence_section_has_one_bare_element_per_protocol() {
    let (app, id, _dir) = app_with_entry().await;

    let (status, body) = get(&app, "/example.com").await;

    assert_eq!(status, StatusCode::OK);
    for protocol in ["dns", "tls", "http", "email", "ip"] {
        let el = element(&body, protocol);
        let href = format!("href=\"/r/{id}/{protocol}\"");
        assert!(!el.contains(&href), "{protocol} still links {href}: {el}");
        assert!(
            !el.contains(REASON),
            "{protocol} names a section reason: {el}"
        );
    }
}

#[tokio::test]
async fn c13_page_renders_no_evidence_block_markup() {
    let (app, _id, _dir) = app_with_entry().await;

    let (status, body) = get(&app, "/example.com").await;

    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("evidence-block"), "block markup in: {body}");
    assert!(!body.contains(BLOCK_TITLE), "block title in: {body}");
}
