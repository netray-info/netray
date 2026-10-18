//! `/api/v1/meta` under lens's default config (spec v2-api, requirement 18): the admission and
//! rate-limit values lens is configured with.
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use lens::config::{
    AdmissionConfig, BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig,
    OgCardsConfig, RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

fn config() -> Config {
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
            per_ip_per_minute: 17,
            per_ip_burst: 5,
            global_per_minute: 900,
            global_burst: 90,
        },
        scoring: ScoringConfig::default(),
        site: SiteConfig::default(),
        badges: BadgesConfig::default(),
        og_cards: OgCardsConfig::default(),
        snapshots: SnapshotsConfig::default(),
        history: lens::config::HistoryConfig::default(),
        admission: AdmissionConfig::default(),
        inspect: Default::default(),
        api: Default::default(),
    }
}

#[tokio::test]
async fn meta_reports_lens_default_admission_and_the_configured_rate_limit() {
    let app = lens::app(AppState::new(config()).unwrap());
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
