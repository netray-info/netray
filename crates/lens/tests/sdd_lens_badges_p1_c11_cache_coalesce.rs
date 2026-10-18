/// §11.1 — Cold cache triggers single run_check and returns grade SVG
/// §11.2 — Warm cache skips run_check entirely
/// §11.3 — Concurrent cold-cache requests coalesce: run_check called exactly once
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::check::CheckOutput;
use lens::config::{BadgesConfig, CacheConfig};
use lens::routes::badge_router;
use lens::score::OverallScore;
use lens::state::{AppState, BadgeCheckFn};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state_with_cache(ttl_seconds: u64) -> AppState {
    use lens::config::{
        BackendsConfig, Config, EcosystemConfig, ScoringConfig, ServerConfig, SiteConfig,
    };
    let config = Config {
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
        admission: lens::config::AdmissionConfig::default(),
        api: Default::default(),
        og_cards: lens::config::OgCardsConfig::default(),
        snapshots: lens::config::SnapshotsConfig::default(),
        history: lens::config::HistoryConfig::default(),
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
                        complete: grade != "error" && grade != "incomplete",
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
    // Follows v2-admission Phase 3: the warm state is a fresh history head, not moka.
    let counter = Arc::new(AtomicUsize::new(0));
    let mut state = make_state_with_cache(300);
    // Mock would return "A" — but it must never be called.
    state.badge_check_fn = Some(mock_check_fn(counter.clone(), "A"));

    // Seed the history with a fresh complete run graded B (10 s old).
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let run = Run {
        profile_version: state.scoring_profile.version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    store
        .record("example.com", &run, &[], now - 10, |p, c| {
            netray_model::diff::diff(p, c, &[], None)
        })
        .await
        .unwrap();
    state.history_store = Some(Arc::new(store));

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
        "a fresh head must serve its grade B, not mock grade A"
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
                        complete: true,
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

    // The first request starts the run; the others join the run in flight (v2-admission
    // R12/R16) and render its result. run_check fires once.
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "run_check must be called exactly once across concurrent cold-cache requests"
    );
}
