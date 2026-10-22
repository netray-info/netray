//! Pins the label bounds of /og/:domain.png that the deny.toml reachability
//! comment for rustybuzz/ttf-parser relies on (advisories spec, C14-C16).
//!
//! Copied from crates/lens/tests/og_label_bounds.rs (spec v2-cutover, Phase 6); nothing dropped.
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use netray_engine::Registry;
use netray_engine::domain_run::{CheckOutput, OverallScore};
use netray_web::admission::{ApexState, BadgeCheckFn};
use netray_web::config::{ApexConfig, BadgesConfig, OgCardsConfig};

use crate::common;

fn make_state(cache_enabled: bool, og_enabled: bool) -> ApexState {
    let mut config = ApexConfig {
        cache_enabled,
        cache_window_seconds: 7200,
        badges: BadgesConfig {
            ttl_seconds: 7200,
            ..BadgesConfig::default()
        },
        og_cards: OgCardsConfig {
            enabled: og_enabled,
        },
        ..ApexConfig::default()
    };
    config.limits.rate_limit_per_ip_per_minute = 60;
    config.limits.rate_limit_per_ip_burst = 10;
    config.limits.rate_limit_global_per_minute = 1000;
    config.limits.rate_limit_global_burst = 100;
    ApexState::new(
        config,
        Registry::new(),
        Vec::new(),
        Duration::from_millis(2000),
        common::profile(),
    )
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
                        complete: grade != "error" && grade != "incomplete",
                    },
                    duration_ms: 1,
                }
            });
        fut
    })
}

fn og_app(state: ApexState) -> Router {
    netray_web::pages::og::router(state)
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
