//! `/api/v1` mount in `lens::app` (spec v2-api, requirements 7, 8, 32).
//! Copied from crates/lens/tests/api_v1_mount.rs (spec v2-cutover, Phase 6); dropped as V1: the
//! `/api/check` comparison with the API on and off, and the lens OpenAPI document test
//! (`/api-docs/openapi.json` is gone). Unmatched paths answer the pages' 404 instead of the SPA.
use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use netray_engine::Registry;
use netray_web::api::{AdmissionLimits, ApiConfig};
use netray_web::config::ApexConfig;
use tower::ServiceExt;

/// lens's `base_config`: the cache off, 60 per minute and a burst of 10 per IP, 1000 and 100
/// globally, `[api] enabled` as given.
fn base_config(enabled: bool) -> ApexConfig {
    let defaults = ApexConfig::default();
    ApexConfig {
        cache_enabled: false,
        cache_window_seconds: 300,
        limits: AdmissionLimits {
            rate_limit_per_ip_per_minute: 60,
            rate_limit_per_ip_burst: 10,
            rate_limit_global_per_minute: 1000,
            rate_limit_global_burst: 100,
            ..defaults.limits
        },
        api: ApiConfig {
            enabled,
            ..Default::default()
        },
        ..defaults
    }
}

fn build(enabled: bool) -> Router {
    let registry = Registry::new();
    let plan = common::default_plan(&registry);
    netray_web::host::apex::app(common::state_with(base_config(enabled), registry, plan))
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
async fn unknown_v1_path_is_json_and_other_paths_stay_pages() {
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
async fn disabled_api_mounts_nothing() {
    let off = build(false);
    let (_, h, body) = get(&off, "/api/v1/catalogue").await;
    let json_ct = h
        .get("content-type")
        .is_some_and(|c| c.to_str().unwrap().starts_with("application/json"));
    assert!(!json_ct, "disabled api answered JSON: {body}");
    assert!(
        serde_json::from_str::<serde_json::Value>(&body)
            .map_or(true, |v| v.get("checks").is_none())
    );
}
