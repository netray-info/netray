//! The latest-evidence route (spec v2-inspectors, Phase 4; C2, C3): `GET /{domain}/tls` answers
//! the TLS view of the domain's latest entry, and the domain is normalised before the lookup.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;

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
    let app = evidence_router(EvidenceDeps {
        history: Arc::new(store),
    });
    (app, id, dir)
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

#[tokio::test]
async fn c2_latest_tls_view_of_the_entry() {
    let (app, id, _dir) = app_with_entry().await;

    let (status, body) = get(&app, "/example.com/tls").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("data-protocol=\"tls\""), "{body}");
    assert!(body.contains(&format!("entry {id}")), "{body}");
    let title = &catalogue()
        .get(&CheckId::parse("tls.chain_trusted").unwrap())
        .unwrap()
        .title;
    assert!(body.contains(title.as_str()), "{body}");
    assert!(body.contains("/inspect/tls?h=example.com:443"), "{body}");
}

#[tokio::test]
async fn c3_domain_is_normalised_before_the_lookup() {
    let (app, _id, _dir) = app_with_entry().await;

    let (want_status, want_body) = get(&app, "/example.com/tls").await;
    let (status, body) = get(&app, "/Example.COM./tls").await;

    assert_eq!(want_status, StatusCode::OK);
    assert_eq!(status, want_status);
    assert_eq!(body, want_body);
}
