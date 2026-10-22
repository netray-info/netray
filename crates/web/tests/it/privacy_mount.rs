//! `/privacy` mount in `lens::app` (spec v2-entries, requirements 5, 16).
//! Copied from crates/lens/tests/privacy_mount.rs (spec v2-cutover, Phase 6); dropped as V1: the
//! snapshot-store configurations (no snapshot store) and the comparison with the SPA shell (an
//! unmatched path answers the pages' 404 now); one `/privacy` route remains, on netray-web's
//! router.
use crate::common;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use netray_engine::Registry;
use netray_engine::domain_run::{CheckOutput, OverallScore};
use netray_web::admission::BadgeCheckFn;
use netray_web::api::AdmissionLimits;
use netray_web::config::ApexConfig;
use tower::ServiceExt;

/// lens's `base_config`: the cache off, 60 per minute and a burst of 10 per IP, 1000 and 100
/// globally.
fn base_config() -> ApexConfig {
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
        ..defaults
    }
}

struct App {
    app: Router,
    counter: Arc<AtomicUsize>,
    _dir: tempfile::TempDir,
}

async fn build(history_on: bool) -> App {
    let registry = Registry::new();
    let plan = common::default_plan(&registry);
    let mut state = common::state_with(base_config(), registry, plan);
    let (history, dir) = common::history_store().await;
    if history_on {
        state.history_store = Some(history);
    }
    let counter = Arc::new(AtomicUsize::new(0));
    state.badge_check_fn = Some(counting_check_fn(counter.clone()));
    App {
        app: netray_web::host::apex::app(state),
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
    for history_on in [false, true] {
        let a = build(history_on).await;
        let (status, headers, body) = get(&a.app, "/privacy").await;
        let label = format!("history={history_on}");
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
async fn other_paths_are_not_the_privacy_page_with_stores_disabled() {
    let a = build(false).await;
    let privacy = netray_web::entries::render::privacy().unwrap();
    for path in ["/", "/some-client-route"] {
        let (_, _, body) = get(&a.app, path).await;
        assert_ne!(body, privacy, "{path} must not be the privacy page");
    }
}
