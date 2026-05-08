/// §11.1 — Cold cache triggers single run_check and returns grade SVG
/// §11.2 — Warm cache skips run_check entirely
/// §11.3 — Concurrent cold-cache requests coalesce: run_check called exactly once
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::cache::{CachedResult, cache_key};
use lens::check::CheckOutput;
use lens::config::{BadgesConfig, CacheConfig};
use lens::routes::badge_router;
use lens::scoring::engine::OverallScore;
use lens::state::{AppState, BadgeCheckFn};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state_with_cache(ttl_seconds: u64) -> AppState {
    use lens::config::{
        BackendsConfig, Config, EcosystemConfig, ScoringConfig, ServerConfig, SiteConfig,
    };
    let config = Config {
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            dns: netray_common::backend::BackendConfig {
                url: Some("http://127.0.0.1:19999".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            dns_servers: Vec::new(),
            tls: netray_common::backend::BackendConfig {
                url: Some("http://127.0.0.1:19998".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            ip: netray_common::backend::BackendConfig {
                url: Some("http://127.0.0.1:19997".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            http: None,
            email: None,
        },
        ecosystem: EcosystemConfig::default(),
        cache: CacheConfig {
            enabled: true,
            ttl_seconds,
        },
        telemetry: Default::default(),
        rate_limit: lens::config::RateLimitConfig {
            per_ip_per_minute: 60,
            per_ip_burst: 10,
            global_per_minute: 1000,
            global_burst: 100,
        },
        scoring: ScoringConfig::default(),
        site: SiteConfig::default(),
        badges: BadgesConfig {
            ttl_seconds,
            ..BadgesConfig::default()
        },
        og_cards: lens::config::OgCardsConfig::default(),
        snapshots: lens::config::SnapshotsConfig::default(),
    };
    AppState::new(config).unwrap()
}

fn mock_check_fn(counter: Arc<AtomicUsize>, grade: &'static str) -> BadgeCheckFn {
    Arc::new(move |_domain: String| {
        let counter = counter.clone();
        let fut: Pin<Box<dyn std::future::Future<Output = CheckOutput> + Send>> =
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
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

fn badge_app(state: AppState) -> Router {
    let (routes, _) = badge_router().split_for_parts();
    Router::new().merge(routes.with_state(state))
}

// ---------------------------------------------------------------------------
// §11.1 — Cold cache triggers single check and returns grade SVG
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cold_cache_triggers_single_check() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut state = make_state_with_cache(300);
    state.badge_check_fn = Some(mock_check_fn(counter.clone(), "B"));

    let app = badge_app(state);
    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 65536).await.unwrap();
    let body = std::str::from_utf8(&bytes).unwrap();
    assert!(
        body.contains(">B<"),
        "cold cache response must contain grade B"
    );
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "run_check must be called exactly once on a cold cache miss"
    );
}

// ---------------------------------------------------------------------------
// §11.2 — Warm cache skips recompute
// ---------------------------------------------------------------------------

#[tokio::test]
async fn warm_cache_skips_run_check() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut state = make_state_with_cache(300);
    // Mock would return "A" — but it must never be called.
    state.badge_check_fn = Some(mock_check_fn(counter.clone(), "A"));

    // Seed the cache with a fresh "B" entry.
    if let Some(cache) = &state.cache {
        let cached = Arc::new(CachedResult {
            sections: HashMap::new(),
            score: OverallScore {
                sections: HashMap::new(),
                overall_percentage: 80.0,
                grade: "B".to_string(),
                hard_fail_triggered: false,
                hard_fail_checks: vec![],
                not_applicable: HashMap::new(),
            },
            duration_ms: 1,
            cached_at: SystemTime::now(),
        });
        cache.insert(cache_key("example.com"), cached).await;
    }

    let app = badge_app(state);
    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 65536).await.unwrap();
    let body = std::str::from_utf8(&bytes).unwrap();
    assert!(
        body.contains(">B<"),
        "warm cache response must serve cached grade B, not mock grade A"
    );
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "run_check must not be called when cache is warm"
    );
}

// ---------------------------------------------------------------------------
// §11.3 — Concurrent cold-cache requests coalesce: run_check called once
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_requests_coalesce_to_single_check() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut state = make_state_with_cache(300);
    let counter_c = counter.clone();
    // Mock sleeps briefly so concurrent requests overlap during the check.
    state.badge_check_fn = Some(Arc::new(move |_domain: String| {
        let counter = counter_c.clone();
        let fut: Pin<Box<dyn std::future::Future<Output = CheckOutput> + Send>> =
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                CheckOutput {
                    domain: "example.com".to_string(),
                    sections: HashMap::new(),
                    score: OverallScore {
                        sections: HashMap::new(),
                        overall_percentage: 80.0,
                        grade: "C".to_string(),
                        hard_fail_triggered: false,
                        hard_fail_checks: vec![],
                        not_applicable: HashMap::new(),
                    },
                    duration_ms: 50,
                }
            });
        fut
    }));

    // Fire 10 concurrent requests for the same domain.
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let app = badge_app(state.clone());
            tokio::spawn(async move {
                let req = Request::builder()
                    .uri("/badge/example.com.svg")
                    .body(Body::empty())
                    .unwrap();
                app.oneshot(req).await.unwrap()
            })
        })
        .collect();

    let responses = futures::future::join_all(handles).await;

    // All requests must succeed.
    for resp in &responses {
        assert_eq!(
            resp.as_ref().unwrap().status(),
            StatusCode::OK,
            "all concurrent requests must return 200"
        );
    }

    // The rate limiter allows 1 token per TTL period per domain. The first request
    // consumes the token and triggers run_check; subsequent requests are throttled
    // (returning a ? badge) or served from cache. Either way, run_check fires once.
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "run_check must be called exactly once across concurrent cold-cache requests"
    );
}
