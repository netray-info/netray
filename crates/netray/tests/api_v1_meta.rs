//! `/api/v1/meta` under the default config (spec v2-api, requirement 18): the admission and
//! rate-limit values the apex is configured with.
//!
//! Copied from crates/lens/tests/api_v1_meta.rs (spec v2-cutover, Phase 6) onto netray-web's
//! apex; assertions unchanged.
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use netray_engine::Registry;
use netray_web::admission::ApexState;
use netray_web::config::ApexConfig;
use serde_json::{Value, json};
use tower::ServiceExt;

/// The defaults with the admission rate limit at 17 and 5 per IP, 900 and 90 globally.
fn config() -> ApexConfig {
    let mut config = ApexConfig {
        cache_enabled: false,
        ..ApexConfig::default()
    };
    config.limits.rate_limit_per_ip_per_minute = 17;
    config.limits.rate_limit_per_ip_burst = 5;
    config.limits.rate_limit_global_per_minute = 900;
    config.limits.rate_limit_global_burst = 90;
    config
}

#[tokio::test]
async fn meta_reports_lens_default_admission_and_the_configured_rate_limit() {
    let state = ApexState::new(
        config(),
        Registry::new(),
        Vec::new(),
        std::time::Duration::from_millis(2000),
        netray_web::admission::load_scoring_profile(None).unwrap(),
    );
    let app = netray_web::host::apex::app(state);
    let resp = app
        .oneshot(Request::get("/api/v1/meta").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["limits"]["admission"]["client_budget_per_hour"], 30);
    assert_eq!(body["limits"]["admission"]["domain_run_slots"], 8);
    assert_eq!(
        body["limits"]["rate_limit"],
        json!({"per_ip_per_minute": 17, "per_ip_burst": 5, "global_per_minute": 900, "global_burst": 90})
    );
}
