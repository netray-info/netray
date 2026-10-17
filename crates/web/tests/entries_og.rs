//! The entry OG card route (V2 spec v2-entries, requirement 15): `GET /r/{id}/og.png` through
//! `entries::router` with a recording card stub.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, RETENTION_SECS, Recorded};
use netray_web::entries::{CardRenderer, EntriesDeps, EntryCard, NOT_FOUND_HTML, V1Lookup, router};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;
const PNG: &[u8] = b"\x89PNG-STUB-BYTES";

fn run(grade: Grade) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
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

async fn record(store: &HistoryStore, domain: &str, grade: Grade, now: i64) -> String {
    match store
        .record(domain, &run(grade), &[], now, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    }
}

/// (domain, grade, created_at) of every card the stub received.
type Seen = Arc<Mutex<Vec<EntryCard>>>;

fn stub(
    result: impl Fn() -> Result<Vec<u8>, Box<Response>> + Send + Sync + 'static,
) -> (CardRenderer, Seen, Arc<AtomicUsize>) {
    let seen: Seen = Arc::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let (s, c) = (seen.clone(), calls.clone());
    let card: CardRenderer = Arc::new(move |card: &EntryCard| {
        c.fetch_add(1, Ordering::SeqCst);
        s.lock().unwrap().push(EntryCard {
            domain: card.domain.clone(),
            grade: card.grade.clone(),
            created_at: card.created_at,
        });
        result()
    });
    (card, seen, calls)
}

fn ok_stub() -> (CardRenderer, Seen, Arc<AtomicUsize>) {
    stub(|| Ok(PNG.to_vec()))
}

fn app(store: Arc<HistoryStore>, v1: Option<V1Lookup>, card: CardRenderer) -> Router {
    router(EntriesDeps {
        history: store,
        v1,
        card,
    })
}

async fn send(
    app: &Router,
    path: &str,
    inm: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut req = Request::get(path);
    if let Some(v) = inm {
        req = req.header(header::IF_NONE_MATCH, v);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes.to_vec())
}

fn etag_of(h: &axum::http::HeaderMap) -> String {
    h.get(header::ETAG)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default()
}

#[tokio::test]
async fn c1_entry_answers_a_png_with_headers_and_hands_the_card_fields_to_the_renderer() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com", Grade::A, T).await;
    let (card, seen, _) = ok_stub();

    let (status, headers, body) =
        send(&app(store, None, card), &format!("/r/{id}/og.png"), None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/png");
    assert_eq!(
        headers.get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=3600"
    );
    assert!(!etag_of(&headers).is_empty(), "no etag header");
    assert_eq!(body, PNG);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].domain, "a.example.com");
    assert_eq!(seen[0].grade, "A");
    assert_eq!(
        seen[0].created_at,
        UNIX_EPOCH + Duration::from_secs(T as u64)
    );
}

#[tokio::test]
async fn c1_etag_is_a_function_of_id_and_grade() {
    let (store, _dir) = open().await;
    let a = record(&store, "a.example.com", Grade::A, T).await;
    let b = record(&store, "b.example.com", Grade::A, T).await;
    let (card, _, _) = ok_stub();
    let app = app(store, None, card);

    let (_, h1, _) = send(&app, &format!("/r/{a}/og.png"), None).await;
    let (_, h2, _) = send(&app, &format!("/r/{a}/og.png"), None).await;
    let (_, h3, _) = send(&app, &format!("/r/{b}/og.png"), None).await;

    assert!(!etag_of(&h1).is_empty(), "no etag header");
    assert_eq!(etag_of(&h1), etag_of(&h2));
    assert_ne!(etag_of(&h1), etag_of(&h3));
}

#[tokio::test]
async fn c5_matching_if_none_match_answers_304_without_calling_the_renderer() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com", Grade::A, T).await;
    let (card, _, calls) = ok_stub();
    let app = app(store, None, card);
    let path = format!("/r/{id}/og.png");

    let (_, headers, _) = send(&app, &path, None).await;
    let etag = etag_of(&headers);
    assert!(!etag.is_empty(), "no etag header");
    let before = calls.load(Ordering::SeqCst);

    let (status, _, body) = send(&app, &path, Some(&etag)).await;

    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(body.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn c6_swept_never_issued_and_v1_ids_answer_410_empty_and_call_nothing() {
    let (store, _dir) = open().await;
    let swept = record(&store, "a.example.com", Grade::A, T).await;
    assert_eq!(store.sweep(T + RETENTION_SECS + 86_400).await.unwrap(), 1);
    let v1_calls = Arc::new(AtomicUsize::new(0));
    let c = v1_calls.clone();
    let v1: V1Lookup = Arc::new(move |_id: String| {
        c.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Some((StatusCode::OK, "V1-PAGE-BODY").into_response()) })
    });
    let (card, _, card_calls) = ok_stub();
    let app = app(store, Some(v1), card);

    for id in [swept.as_str(), "AAAAAAAA", "BBBBBBBB"] {
        let (status, _, body) = send(&app, &format!("/r/{id}/og.png"), None).await;
        assert_eq!(status, StatusCode::GONE, "{id}");
        assert!(body.is_empty(), "{id}");
    }
    assert_eq!(v1_calls.load(Ordering::SeqCst), 0);
    assert_eq!(card_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn c7_malformed_id_and_other_subpaths_answer_404() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com", Grade::A, T).await;
    let (card, _, calls) = ok_stub();
    let app = app(store, None, card);

    let (status, _, _) = send(&app, "/r/x/og.png", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, body) = send(&app, &format!("/r/{id}/other"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, NOT_FOUND_HTML.as_bytes());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn c8_a_render_failure_answers_the_renderers_response_unchanged() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com", Grade::A, T).await;
    let (card, _, _) = stub(|| {
        let mut resp: Response =
            (StatusCode::INTERNAL_SERVER_ERROR, "RENDER-FAILED-BODY").into_response();
        resp.headers_mut()
            .insert("x-render", "failed".parse().unwrap());
        Err(Box::new(resp))
    });

    let (status, headers, body) =
        send(&app(store, None, card), &format!("/r/{id}/og.png"), None).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(headers.get("x-render").unwrap(), "failed");
    assert_eq!(body, b"RENDER-FAILED-BODY");
}

#[tokio::test]
async fn c5_failed_history_lookup_answers_500_not_410() {
    let (store, dir) = open().await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("history.db"));
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query("DROP TABLE entries")
        .execute(&pool)
        .await
        .unwrap();
    let (card, _, calls) = ok_stub();

    let (status, _, _) = send(&app(store, None, card), "/r/AAAAAAAA/og.png", None).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
