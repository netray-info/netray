//! v2-http-ui Phase 3 (C2, C8): the http.cdn view and its absence.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::pages::views::{ViewContext, render};
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;

fn block(fields: Vec<(&str, Value)>) -> Block {
    Block {
        id: BlockId::parse("http.cdn").unwrap(),
        title: "CDN".into(),
        shape: Shape::Fields(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        ),
    }
}

fn view(block: &Block) -> Option<String> {
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    render(block, &ctx)
}

#[test]
fn c2_the_cdn_view_shows_provider_cache_status_and_indicator_chips() {
    let b = block(vec![
        ("provider", Value::Text("Cloudflare".into())),
        ("cache_status", Value::Text("HIT".into())),
        ("indicator", Value::Text("cf-ray".into())),
        ("indicator", Value::Text("cf-cache-status".into())),
    ]);
    let html = view(&b).expect("claimed");
    assert!(
        html.contains(r#"<h3 class="protocol-view__title">CDN</h3>"#),
        "{html}"
    );
    for text in [
        ">Provider<",
        ">Cloudflare<",
        ">Cache status<",
        ">HIT<",
        ">cf-ray<",
        ">cf-cache-status<",
    ] {
        assert!(html.contains(text), "{text}: {html}");
    }
    assert_eq!(html.matches("facts-grid__chip").count(), 2, "{html}");
    assert!(!html.contains("data-tone"), "{html}");
}

#[test]
fn c2_without_a_cache_status_there_is_no_cache_status_cell() {
    let b = block(vec![("provider", Value::Text("Cloudflare".into()))]);
    let html = view(&b).expect("claimed");
    assert!(!html.contains("Cache status"), "{html}");
}

#[tokio::test]
async fn c8_a_run_without_http_cdn_renders_no_cdn_view() {
    let run = Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Http,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("http.hsts").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![BlockId::parse("http.hsts").unwrap()],
        }],
        blocks: vec![Block {
            id: BlockId::parse("http.hsts").unwrap(),
            title: "HSTS".into(),
            shape: Shape::Fields(vec![("max_age".to_string(), Value::Int(31_536_000))]),
        }],
    };
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let id = match store
        .record("a.example.com", &run, &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    let app: Router = evidence_router(EvidenceDeps {
        history: Arc::new(store),
    });
    let resp = app
        .oneshot(
            Request::get(format!("/r/{id}/http"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains(r#"id="ev-http.hsts""#), "{body}");
    assert!(!body.contains(r#"id="ev-http.cdn""#), "{body}");
}
