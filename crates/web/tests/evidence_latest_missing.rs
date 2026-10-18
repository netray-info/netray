//! The latest-evidence route (V2 spec v2-inspectors, Phase 4, C4): `GET /{domain}/dns` with no
//! entry for the domain answers 404 with an HTML body linking the domain page.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
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
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

async fn open() -> (Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (Arc::new(store), dir)
}

async fn record(store: &HistoryStore, domain: &str) {
    match store
        .record(domain, &run(), &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { .. } => {}
        other => panic!("expected First, got {other:?}"),
    }
}

#[tokio::test]
async fn c4_no_entry_for_the_domain_answers_404_linking_the_domain_page() {
    // (label, domains that hold an entry)
    let stores: &[(&str, &[&str])] = &[
        ("empty store", &[]),
        ("entry of another domain only", &["a.example.com"]),
    ];
    for (label, domains) in stores {
        let (store, _dir) = open().await;
        for d in *domains {
            record(&store, d).await;
        }
        let app: Router = evidence_router(EvidenceDeps {
            history: store,
            v1: None,
        });

        let res = app
            .oneshot(
                Request::get("/example.com/dns")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let content_type = res
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_owned())
            .unwrap_or_default();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&bytes);

        assert_eq!(status, StatusCode::NOT_FOUND, "{label}: status");
        assert!(
            content_type.starts_with("text/html"),
            "{label}: {content_type}"
        );
        assert!(body.contains(r#"href="/example.com""#), "{label}: {body}");
    }
}
