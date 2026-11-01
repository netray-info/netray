//! On the stored result page every clean-list row's evidence link resolves (spec
//! pustina-findings, reader finding 2): an entry evidence URL, as attention rows carry, or a
//! fragment naming an id the page holds. `#ev-<block>` anchors are never rendered there.
use std::net::IpAddr;
use std::sync::Arc;

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

const DOMAIN: &str = "example.com";

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
    let block = BlockId::parse("dns.ns").unwrap();
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.ns").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![block.clone()],
        }],
        blocks: vec![Block {
            id: block,
            title: "Name servers".into(),
            shape: Shape::Text("ns1.example.net".into()),
        }],
    }
}

#[tokio::test]
async fn stored_page_pass_row_evidence_links_resolve() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let id = match store
        .record(DOMAIN, &run(), &[], 1_700_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
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

    let marker = "<a class=\"check-row__link\" href=\"";
    let hrefs: Vec<&str> = body
        .match_indices(marker)
        .filter_map(|(at, _)| {
            let rest = &body[at + marker.len()..];
            let end = rest.find('"')?;
            rest[end..]
                .starts_with("\">evidence")
                .then_some(&rest[..end])
        })
        .collect();
    assert!(!hrefs.is_empty(), "no evidence link on the page: {body}");
    for href in hrefs {
        match href.strip_prefix('#') {
            Some(fragment) => assert!(
                body.contains(&format!("id=\"{fragment}\"")),
                "fragment {href} names no element on the stored page"
            ),
            None => assert!(
                href.starts_with(&format!("/r/{id}/")),
                "evidence href {href} is no entry evidence URL of entry {id}"
            ),
        }
    }
}
