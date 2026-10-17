/// The badge reads `score.complete`, not only the grade string (v2-history requirement 14).
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::check::CheckOutput;
use lens::config::{BadgesConfig, CacheConfig, OgCardsConfig};
use lens::routes::badge_router;
use lens::score::OverallScore;
use lens::state::{AppState, BadgeCheckFn};

const SHORT_CACHE: &str = "public, max-age=300, s-maxage=300";
const LONG_CACHE: &str = "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400";

fn make_state(cache: bool) -> AppState {
    use lens::config::{
        BackendsConfig, Config, EcosystemConfig, RateLimitConfig, ScoringConfig, ServerConfig,
        SiteConfig,
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
                url: Some("http://127.0.0.1:19997".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            http: None,
            email: None,
        },
        ecosystem: EcosystemConfig::default(),
        cache: CacheConfig {
            enabled: cache,
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
        badges: BadgesConfig::default(),
        admission: lens::config::AdmissionConfig::default(),
        og_cards: OgCardsConfig::default(),
        snapshots: lens::config::SnapshotsConfig::default(),
        history: lens::config::HistoryConfig::default(),
    };
    AppState::new(config).unwrap()
}

fn mock_check_fn(grade: &'static str, complete: bool) -> BadgeCheckFn {
    Arc::new(move |_domain: String| {
        let fut: Pin<Box<dyn std::future::Future<Output = CheckOutput> + Send>> =
            Box::pin(async move {
                CheckOutput {
                    domain: "example.com".to_string(),
                    sections: HashMap::new(),
                    score: OverallScore {
                        sections: HashMap::new(),
                        overall_percentage: 0.0,
                        grade: grade.to_string(),
                        hard_fail_triggered: grade == "F",
                        hard_fail_checks: vec![],
                        not_applicable: HashMap::new(),
                        complete,
                    },
                    duration_ms: 1,
                }
            });
        fut
    })
}

/// Fetch the badge for example.com with a mocked result; returns (svg, cache-control).
async fn badge(grade: &'static str, complete: bool) -> (String, String) {
    badge_with(grade, complete, false).await
}

/// As [`badge`], with the result cache on or off.
async fn badge_with(grade: &'static str, complete: bool, cache: bool) -> (String, String) {
    let mut state = make_state(cache);
    state.badge_check_fn = Some(mock_check_fn(grade, complete));
    let (routes, _) = badge_router().split_for_parts();
    let app = Router::new().merge(routes.with_state(state));
    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cc = resp
        .headers()
        .get("cache-control")
        .expect("cache-control header must be present")
        .to_str()
        .unwrap()
        .to_string();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    (String::from_utf8(bytes.to_vec()).unwrap(), cc)
}

#[tokio::test]
async fn incomplete_result_with_grade_f_renders_unscored_badge() {
    let (svg, cc) = badge("F", false).await;
    let (unscored_svg, _) = badge("incomplete", false).await;

    assert_eq!(
        svg, unscored_svg,
        "an incomplete F must render like the unscored badge"
    );
    assert!(svg.contains(">?</text>"), "badge must show ?, got: {svg}");
    assert!(
        !svg.contains(">F</text>"),
        "badge must not show F, got: {svg}"
    );
    assert_eq!(cc, SHORT_CACHE);
}

#[tokio::test]
async fn incomplete_result_with_grade_f_renders_unscored_badge_with_the_cache_on() {
    let (svg, cc) = badge_with("F", false, true).await;
    assert!(svg.contains(">?</text>"), "badge must show ?, got: {svg}");
    assert_eq!(cc, SHORT_CACHE);
}

#[tokio::test]
async fn complete_result_with_grade_f_renders_f_badge() {
    let (svg, cc) = badge("F", true).await;

    assert!(svg.contains(">F</text>"), "badge must show F, got: {svg}");
    assert!(
        !svg.contains(">?</text>"),
        "badge must not show ?, got: {svg}"
    );
    assert_eq!(cc, LONG_CACHE);
}
