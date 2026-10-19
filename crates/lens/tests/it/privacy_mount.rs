//! `/privacy` mount in `lens::app` (spec v2-entries, requirements 5, 16).
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::check::CheckOutput;
use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::score::OverallScore;
use lens::snapshot::SnapshotStore;
use lens::state::{AppState, BadgeCheckFn};
use netray_store::HistoryStore;

fn base_config() -> Config {
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
        api: Default::default(),
    }
}

struct App {
    app: Router,
    state: AppState,
    counter: Arc<AtomicUsize>,
    _dir: tempfile::TempDir,
}

async fn build(history_on: bool, snapshots_on: bool) -> App {
    let dir = tempfile::tempdir().unwrap();
    let mut config = base_config();
    config.history.enabled = history_on;
    config.history.db_path = dir.path().join("history.db");
    config.snapshots.enabled = snapshots_on;
    config.snapshots.db_path = dir.path().join("snapshots.db");

    let counter = Arc::new(AtomicUsize::new(0));
    let mut state = AppState::new(config).unwrap();
    if history_on {
        let history = HistoryStore::open(&state.config.history.db_path)
            .await
            .unwrap();
        history.migrate().await.unwrap();
        state.history_store = Some(Arc::new(history));
    }
    if snapshots_on {
        let snaps = SnapshotStore::new(&state.config.snapshots.db_path)
            .await
            .unwrap();
        snaps.migrate().await.unwrap();
        state.snapshot_store = Some(Arc::new(snaps));
    }
    state.badge_check_fn = Some(counting_check_fn(counter.clone()));
    App {
        app: lens::app(state.clone()),
        state,
        counter,
        _dir: dir,
    }
}

fn counting_check_fn(counter: Arc<AtomicUsize>) -> BadgeCheckFn {
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
                        grade: "A".to_string(),
                        hard_fail_triggered: false,
                        hard_fail_checks: vec![],
                        not_applicable: HashMap::new(),
                        complete: true,
                    },
                    duration_ms: 1,
                }
            });
        fut
    })
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

#[tokio::test]
async fn privacy_is_served_in_every_store_configuration() {
    let expected = netray_web::entries::render::privacy().unwrap();
    assert!(expected.contains("90 days"));
    assert!(expected.contains("in memory"));
    for (history_on, snapshots_on) in [(false, false), (true, false), (false, true), (true, true)] {
        let a = build(history_on, snapshots_on).await;
        let (status, headers, body) = get(&a.app, "/privacy").await;
        let label = format!("history={history_on} snapshots={snapshots_on}");
        assert_eq!(status, StatusCode::OK, "{label}");
        assert_eq!(
            headers["content-type"], "text/html; charset=utf-8",
            "{label}"
        );
        assert_eq!(headers["cache-control"], "public, max-age=3600", "{label}");
        assert_eq!(body, expected, "{label}");
        assert_eq!(
            a.counter.load(Ordering::SeqCst),
            0,
            "badge check ran: {label}"
        );
    }
}

#[tokio::test]
async fn spa_routes_still_answer_the_shell_with_stores_disabled() {
    let a = build(false, false).await;
    let privacy = netray_web::entries::render::privacy().unwrap();
    for path in ["/", "/some-client-route"] {
        let direct = lens::spa::handler(
            axum::extract::State(lens::spa::Shell(a.state.rendered_html.clone())),
            path.parse::<axum::http::Uri>().unwrap(),
        )
        .await;
        let want_status = direct.status();
        let want_ct = direct.headers().get("content-type").cloned();
        let want_body = to_bytes(direct.into_body(), usize::MAX).await.unwrap();
        let (status, headers, body) = get(&a.app, path).await;
        assert_eq!(status, want_status, "{path}");
        assert_eq!(headers.get("content-type").cloned(), want_ct, "{path}");
        assert_eq!(body.as_bytes(), &want_body[..], "{path}");
        assert_ne!(body, privacy, "{path} must not be the privacy page");
    }
}
