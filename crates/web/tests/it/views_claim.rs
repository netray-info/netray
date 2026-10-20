//! v2-http-ui Phase 1 (C2, C3, C8, C12): a protocol view claims a block whose keys it knows;
//! a block with any other key stays a generic evidence block.
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

fn hsts_block(extra: bool) -> Block {
    let mut fields = vec![
        ("max_age".to_string(), Value::Int(31_536_000)),
        ("include_subdomains".to_string(), Value::Bool(true)),
        ("preload".to_string(), Value::Bool(false)),
    ];
    if extra {
        fields.push(("extra".to_string(), Value::Text("x".into())));
    }
    Block {
        id: BlockId::parse("http.hsts").unwrap(),
        title: "HSTS".into(),
        shape: Shape::Fields(fields),
    }
}

fn run(extra: bool) -> Run {
    Run {
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
        blocks: vec![hsts_block(extra)],
    }
}

async fn page(extra: bool) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let id = match store
        .record("a.example.com", &run(extra), &[], T, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    let app: Router = evidence_router(EvidenceDeps {
        history: Arc::new(store),
        v1: None,
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
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The element opening that carries `id="ev-http.hsts"`: from its `<section` to the tag end.
fn open_tag(body: &str) -> &str {
    let at = body
        .find(r#"id="ev-http.hsts""#)
        .expect("ev-http.hsts missing");
    let start = body[..at].rfind('<').unwrap();
    let end = at + body[at..].find('>').unwrap();
    &body[start..end]
}

#[tokio::test]
async fn c2_c8_known_keys_render_one_protocol_view_with_feeds_and_facts_grid() {
    let body = page(false).await;
    assert_eq!(body.matches(r#"id="ev-http.hsts""#).count(), 1, "{body}");
    let tag = open_tag(&body);
    assert!(
        tag.starts_with(r#"<section class="protocol-view""#),
        "{tag}"
    );
    let section = &body[body.find(r#"id="ev-http.hsts""#).unwrap()..];
    let section = &section[..section.find("</section>").expect("section closes")];
    assert!(section.contains("protocol-view__feeds"), "{section}");
    assert!(section.contains("Feeds lines:"), "{section}");
    assert!(section.contains("facts-grid"), "{section}");
}

#[tokio::test]
async fn c3_c12_extra_key_leaves_a_generic_evidence_block() {
    let body = page(true).await;
    assert_eq!(body.matches(r#"id="ev-http.hsts""#).count(), 1, "{body}");
    let tag = open_tag(&body);
    assert!(
        tag.starts_with(r#"<section class="evidence-block""#),
        "{tag}"
    );
    assert!(!body.contains("protocol-view"), "{body}");
}

#[test]
fn c3_render_claims_known_keys_and_refuses_an_extra_key() {
    let statuses = [(CheckId::parse("http.hsts").unwrap(), Status::Pass)];
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    let known = render(&hsts_block(false), &ctx).expect("known keys are claimed");
    assert!(known.contains(r#"id="ev-http.hsts""#), "{known}");
    assert_eq!(render(&hsts_block(true), &ctx), None);
}
