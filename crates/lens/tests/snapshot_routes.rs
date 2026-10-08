/// Integration tests for GET /r/:shortid snapshot route.
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use tempfile::NamedTempFile;
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::routes::snapshot_router;
use lens::snapshot::store::SNAPSHOT_TTL_SECS;
use lens::snapshot::{Snapshot, SnapshotFinding, SnapshotSection, SnapshotStore};
use lens::state::AppState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base_config() -> Config {
    Config {
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            dns: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19999".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            dns_servers: Vec::new(),
            tls: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19998".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            ip: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19997".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            http: None,
            email: None,
        },
        ecosystem: EcosystemConfig::default(),
        cache: CacheConfig {
            enabled: false,
            ttl_seconds: 300,
        },
        telemetry: Default::default(),
        rate_limit: RateLimitConfig {
            per_ip_per_minute: 60,
            per_ip_burst: 10,
            global_per_minute: 1000,
            global_burst: 100,
        },
        scoring: ScoringConfig::default(),
        site: SiteConfig::default(),
        badges: BadgesConfig::default(),
        og_cards: OgCardsConfig::default(),
        snapshots: SnapshotsConfig::default(),
    }
}

fn make_snapshot(domain: &str, grade: &str) -> Snapshot {
    Snapshot {
        shortid: String::new(),
        domain: domain.to_string(),
        grade: grade.to_string(),
        score: 95.0,
        sections: vec![SnapshotSection {
            name: "DNS".to_string(),
            grade: grade.to_string(),
            passes: 5,
            warns: 0,
            fails: 0,
            skips: 0,
            findings: vec![SnapshotFinding {
                check_name: "dnssec".to_string(),
                verdict: "pass".to_string(),
                message: "DNSSEC enabled".to_string(),
                earned: 5,
                possible: 5,
                fix_hint: None,
                fix_owner: None,
                guide_url: None,
            }],
        }],
        server_addresses: vec![],
        created_at: Utc::now(),
        lens_version: "0.9.1".to_string(),
    }
}

async fn make_app_with_store() -> (Router, Arc<SnapshotStore>, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let store = SnapshotStore::new(tmp.path()).await.unwrap();
    store.migrate().await.unwrap();
    let arc_store = Arc::new(store);

    let mut state = AppState::new(base_config()).unwrap();
    state.snapshot_store = Some(Arc::clone(&arc_store));

    let app = snapshot_router().with_state(state);
    (app, arc_store, tmp)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn existing_snapshot_returns_200_html() {
    let (app, store, _tmp) = make_app_with_store().await;
    let id = store
        .insert(make_snapshot("example.com", "A"))
        .await
        .unwrap();

    let req = Request::builder()
        .uri(format!("/r/{id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        ct.contains("text/html"),
        "content-type must be text/html, got: {ct}"
    );
}

#[tokio::test]
async fn existing_snapshot_has_correct_cache_control() {
    let (app, store, _tmp) = make_app_with_store().await;
    let id = store
        .insert(make_snapshot("example.com", "B"))
        .await
        .unwrap();

    let req = Request::builder()
        .uri(format!("/r/{id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let cc = resp
        .headers()
        .get("cache-control")
        .expect("cache-control must be present")
        .to_str()
        .unwrap();
    assert!(
        cc.contains("public"),
        "cache-control must be public, got: {cc}"
    );
    assert!(
        cc.contains("max-age=86400"),
        "cache-control must have max-age=86400, got: {cc}"
    );
    assert!(
        cc.contains("immutable"),
        "cache-control must include immutable, got: {cc}"
    );
}

#[tokio::test]
async fn existing_snapshot_body_contains_domain() {
    let (app, store, _tmp) = make_app_with_store().await;
    let id = store
        .insert(make_snapshot("example.com", "A"))
        .await
        .unwrap();

    let req = Request::builder()
        .uri(format!("/r/{id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("example.com"), "body must contain domain");
    assert!(
        html.contains("<!DOCTYPE html>"),
        "body must be full HTML page"
    );
}

/// Assert the "expired or unknown" contract: 404, text/html, body names expiry/unknown.
async fn assert_expired_or_unknown_page(app: Router, uri: &str) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND, "uri: {uri}");
    let ct = resp
        .headers()
        .get("content-type")
        .expect("content-type must be present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        ct.contains("text/html"),
        "content-type must be text/html for {uri}, got: {ct}"
    );
    let body = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap().to_lowercase();
    assert!(
        html.contains("expired") && html.contains("unknown"),
        "body must say the snapshot is expired or unknown for {uri}, got: {html}"
    );
}

#[tokio::test]
async fn unknown_shortid_returns_404_expired_or_unknown_page() {
    let (app, _store, _tmp) = make_app_with_store().await;
    assert_expired_or_unknown_page(app, "/r/AAAAAAAA").await;
}

#[tokio::test]
async fn expired_snapshot_returns_404_expired_or_unknown_page() {
    let (app, store, _tmp) = make_app_with_store().await;
    let mut snap = make_snapshot("example.com", "A");
    snap.created_at = Utc::now() - chrono::Duration::seconds(SNAPSHOT_TTL_SECS + 3600);
    let id = store.insert(snap).await.unwrap();
    assert_expired_or_unknown_page(app, &format!("/r/{id}")).await;
}

#[tokio::test]
async fn malformed_shortids_return_404_expired_or_unknown_page() {
    for uri in [
        "/r/x",
        "/r/short",
        "/r/AAAA-AAA",
        "/r/AAAA%2DAAA",
        "/r/AA%20AAAAA",
    ] {
        let (app, _store, _tmp) = make_app_with_store().await;
        assert_expired_or_unknown_page(app, uri).await;
    }
}

#[tokio::test]
async fn no_store_returns_404() {
    let state = AppState::new(base_config()).unwrap();
    // snapshot_store is None by default
    let app = snapshot_router().with_state(state);

    let req = Request::builder()
        .uri("/r/AAAAAAAA")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
