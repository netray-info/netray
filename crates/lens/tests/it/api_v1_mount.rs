//! `/api/v1` mount in `lens::app` (spec v2-api, requirements 7, 8, 32).
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;
use netray_web::api::ApiConfig;

fn base_config(enabled: bool) -> Config {
    Config {
        modules: Default::default(),
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            resolve_timeout_ms: 2000,
            dns: lens::config::BackendConfig {
                timeout_ms: 100,
                ..Default::default()
            },
            tls: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19998".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            ip: lens::config::BackendConfig {
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
        history: lens::config::HistoryConfig::default(),
        admission: lens::config::AdmissionConfig::default(),
        inspect: Default::default(),
        api: ApiConfig {
            enabled,
            ..Default::default()
        },
    }
}

fn build(enabled: bool) -> Router {
    lens::app(AppState::new(base_config(enabled)).unwrap())
}

async fn get(app: &Router, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn assert_error_shape(body: &str, code: &str, label: &str) {
    let v: serde_json::Value =
        serde_json::from_str(body).unwrap_or_else(|e| panic!("{label}: not JSON ({e}): {body}"));
    assert_eq!(v["error"]["code"], code, "{label}: {body}");
    assert!(
        v["error"]["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "{label}: {body}"
    );
}

#[tokio::test]
async fn v1_errors_carry_the_error_shape() {
    let app = build(true);
    for (path, status, code) in [
        ("/api/v1/nope", StatusCode::NOT_FOUND, "NOT_FOUND"),
        (
            "/api/v1/profiles/x",
            StatusCode::BAD_REQUEST,
            "INVALID_INPUT",
        ),
        ("/api/v1/profiles/99", StatusCode::NOT_FOUND, "NOT_FOUND"),
    ] {
        let (s, h, body) = get(&app, path).await;
        assert_eq!(s, status, "{path}");
        assert!(
            h["content-type"]
                .to_str()
                .unwrap()
                .starts_with("application/json"),
            "{path}"
        );
        assert_error_shape(&body, code, path);
    }
}

#[tokio::test]
async fn unknown_v1_path_is_json_and_other_paths_stay_spa() {
    let app = build(true);
    let (s, h, body) = get(&app, "/api/v1/nope").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(
        h["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
    assert_error_shape(&body, "NOT_FOUND", "/api/v1/nope");

    let off = build(false);
    let (want_s, want_h, want_body) = get(&off, "/nope").await;
    let (s, h, body) = get(&app, "/nope").await;
    assert_eq!(s, want_s);
    assert_eq!(h.get("content-type"), want_h.get("content-type"));
    assert_eq!(body, want_body);
}

#[tokio::test]
async fn disabled_api_mounts_nothing_and_leaves_v1_checks_alone() {
    let off = build(false);
    let on = build(true);
    let (_, h, body) = get(&off, "/api/v1/catalogue").await;
    let json_ct = h
        .get("content-type")
        .is_some_and(|c| c.to_str().unwrap().starts_with("application/json"));
    assert!(!json_ct, "disabled api answered JSON: {body}");
    assert!(
        serde_json::from_str::<serde_json::Value>(&body)
            .map_or(true, |v| v.get("checks").is_none())
    );

    let (s_on, h_on, b_on) = get(&on, "/api/check/-bad-").await;
    let (s_off, h_off, b_off) = get(&off, "/api/check/-bad-").await;
    assert_eq!(s_on, s_off);
    assert_eq!(h_on.get("content-type"), h_off.get("content-type"));
    assert_eq!(b_on, b_off);
}

#[tokio::test]
async fn lens_openapi_lists_no_v1_path_and_is_unchanged_by_the_api_flag() {
    let (s_on, _, on) = get(&build(true), "/api-docs/openapi.json").await;
    let (s_off, _, off) = get(&build(false), "/api-docs/openapi.json").await;
    assert_eq!(s_on, StatusCode::OK);
    assert_eq!(s_off, StatusCode::OK);
    assert_eq!(on, off);
    let doc: serde_json::Value = serde_json::from_str(&on).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    assert!(!paths.is_empty());
    for p in paths.keys() {
        assert!(!p.starts_with("/api/v1"), "lens document lists {p}");
    }
}
