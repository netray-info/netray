/// Pins the label bounds of /og/:domain.png that the deny.toml reachability
/// comment for rustybuzz/ttf-parser relies on (advisories spec, C14-C16).
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::check::CheckOutput;
use lens::config::{BadgesConfig, CacheConfig, OgCardsConfig};
use lens::routes::og_router;
use lens::scoring::engine::OverallScore;
use lens::state::{AppState, BadgeCheckFn};

fn make_state(cache_enabled: bool, og_enabled: bool) -> AppState {
    use lens::config::{
        BackendsConfig, Config, EcosystemConfig, RateLimitConfig, ScoringConfig, ServerConfig,
        SiteConfig,
    };
    let config = Config {
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            dns: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19999".to_string()),
                timeout_ms: 100,
            },
            dns_servers: Vec::new(),
            tls: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19998".to_string()),
                timeout_ms: 100,
            },
            ip: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19997".to_string()),
                timeout_ms: 100,
            },
            http: None,
            email: None,
        },
        ecosystem: EcosystemConfig::default(),
        cache: CacheConfig {
            enabled: cache_enabled,
            ttl_seconds: 7200,
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
        badges: BadgesConfig {
            ttl_seconds: 7200,
            ..BadgesConfig::default()
        },
        og_cards: OgCardsConfig {
            enabled: og_enabled,
        },
        snapshots: lens::config::SnapshotsConfig::default(),
    };
    AppState::new(config).unwrap()
}

fn mock_check_fn(grade: &'static str) -> BadgeCheckFn {
    Arc::new(move |_domain: String| {
        let fut: Pin<Box<dyn std::future::Future<Output = CheckOutput> + Send>> =
            Box::pin(async move {
                CheckOutput {
                    domain: "example.com".to_string(),
                    sections: HashMap::new(),
                    score: OverallScore {
                        sections: HashMap::new(),
                        overall_percentage: 80.0,
                        grade: grade.to_string(),
                        hard_fail_triggered: false,
                        hard_fail_checks: vec![],
                        not_applicable: HashMap::new(),
                    },
                    duration_ms: 1,
                }
            });
        fut
    })
}

fn og_app(state: AppState) -> Router {
    og_router().with_state(state)
}

async fn get_label(label: &str) -> (StatusCode, Vec<u8>) {
    let mut state = make_state(true, true);
    state.badge_check_fn = Some(mock_check_fn("B"));
    let encoded: String = label
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let req = Request::builder()
        .uri(format!("/og/example.com.png?label={encoded}"))
        .body(Body::empty())
        .unwrap();
    let resp = og_app(state).oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, bytes.to_vec())
}

#[tokio::test]
async fn c14_label_of_33_bytes_is_rejected() {
    let (status, _) = get_label(&"a".repeat(33)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn c15_non_ascii_label_is_rejected() {
    let (status, _) = get_label("caf\u{e9}").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn c16_label_of_32_printable_bytes_renders_png() {
    let label = "a b-c.d~".repeat(4);
    assert_eq!(label.len(), 32);
    let (status, body) = get_label(&label).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with(b"\x89PNG"), "body must be a PNG");
}
