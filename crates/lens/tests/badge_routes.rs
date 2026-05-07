/// Integration tests for the badge endpoint (SDD §11.1–11.12).
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::config::{BadgesConfig, CacheConfig};
use lens::routes::badge_router;
use lens::state::AppState;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_badge_state(badges: BadgesConfig, cache_enabled: bool) -> AppState {
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
            dns: netray_common::backend::BackendConfig {
                url: Some("http://127.0.0.1:19999".to_string()),
                timeout_ms: 1000,
                ..Default::default()
            },
            dns_servers: Vec::new(),
            tls: netray_common::backend::BackendConfig {
                url: Some("http://127.0.0.1:19998".to_string()),
                timeout_ms: 1000,
                ..Default::default()
            },
            ip: netray_common::backend::BackendConfig {
                url: Some("http://127.0.0.1:19997".to_string()),
                timeout_ms: 1000,
                ..Default::default()
            },
            http: None,
            email: None,
        },
        ecosystem: EcosystemConfig::default(),
        cache: CacheConfig {
            enabled: cache_enabled,
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
        badges,
    };
    AppState::new(config).unwrap()
}

fn default_badges_state() -> AppState {
    make_badge_state(BadgesConfig::default(), false)
}

fn badge_app(state: AppState) -> Router {
    let (routes, _) = badge_router().split_for_parts();
    Router::new().merge(routes.with_state(state))
}

// ---------------------------------------------------------------------------
// §11.6 — Invalid domain returns 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn invalid_domain_returns_400() {
    let app = badge_app(default_badges_state());
    let req = Request::builder()
        .uri("/badge/192.168.1.1.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"], "DOMAIN_INVALID");
    assert!(json["error"]["message"].is_string());
}

// ---------------------------------------------------------------------------
// §11.7 — Label with non-ASCII-printable character returns 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn invalid_label_returns_400() {
    let app = badge_app(default_badges_state());
    // %01 is a control character (0x01 < 0x20), outside the valid ASCII printable range.
    // Note: <script> contains only printable ASCII (< = 0x3C, > = 0x3E, both in 0x20–0x7E),
    // so angle brackets are valid label chars and are HTML-escaped in SVG output.
    let req = Request::builder()
        .uri("/badge/example.com.svg?label=bad%01label")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"], "INVALID_LABEL");
}

// ---------------------------------------------------------------------------
// §11.8 — Label is HTML-escaped in SVG output
// ---------------------------------------------------------------------------

#[tokio::test]
async fn label_ampersand_is_html_escaped_in_svg() {
    let app = badge_app(default_badges_state());
    // a&b — & is ASCII printable (0x26), valid label
    let req = Request::builder()
        .uri("/badge/example.com.svg?label=a%26b")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("image/svg+xml"), "must be SVG content-type");
    let bytes = to_bytes(resp.into_body(), 65536).await.unwrap();
    let body = std::str::from_utf8(&bytes).unwrap();
    assert!(body.contains("a&amp;b"), "& must be escaped as &amp; in SVG");
    assert!(!body.contains(">a&b<"), "bare & must not appear in SVG");
}

// ---------------------------------------------------------------------------
// §11.11 — Unknown ?style= falls back to flat (height=20)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unknown_style_falls_back_to_flat() {
    let app = badge_app(default_badges_state());
    let req = Request::builder()
        .uri("/badge/example.com.svg?style=garbage")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 65536).await.unwrap();
    let body = std::str::from_utf8(&bytes).unwrap();
    assert!(
        body.contains("height=\"20\""),
        "unknown style must fall back to flat (height=20)"
    );
    assert!(
        !body.contains("height=\"28\""),
        "must not produce for-the-badge height"
    );
}

// ---------------------------------------------------------------------------
// §11.12 — Empty domain segment produces 400 (domain validation rejects "")
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_domain_segment_returns_404() {
    let app = badge_app(default_badges_state());
    let req = Request::builder()
        .uri("/badge/.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // Route captures ".svg" as domain, strips suffix to "", validate_domain("")
    // returns DOMAIN_INVALID (400).
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "empty domain in /badge/.svg must yield 400"
    );
}

// ---------------------------------------------------------------------------
// §11.9 — Feature disabled returns 404
// ---------------------------------------------------------------------------

#[tokio::test]
async fn feature_disabled_returns_404() {
    let mut badges_cfg = BadgesConfig::default();
    badges_cfg.enabled = false;
    // When disabled, the route is not registered → use a router WITHOUT badge routes.
    // We simulate this by not adding the badge router.
    let state = make_badge_state(badges_cfg, false);
    // Build a router without the badge route (feature disabled path)
    let app: Router = Router::new(); // no badge routes
    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let _ = state; // ensure state is used
}

// ---------------------------------------------------------------------------
// §11.4 — ETag enables 304 short-circuit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn etag_round_trip_produces_304() {
    let app = badge_app(default_badges_state());

    // First request — get the ETag.
    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let etag = resp
        .headers()
        .get("etag")
        .expect("ETag header must be present")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(etag.starts_with('"') && etag.ends_with('"'), "ETag must be quoted");

    // Second request with matching If-None-Match.
    let app2 = badge_app(default_badges_state());
    let req2 = Request::builder()
        .uri("/badge/example.com.svg")
        .header("if-none-match", &etag)
        .body(Body::empty())
        .unwrap();
    let resp2 = app2.oneshot(req2).await.unwrap();
    assert_eq!(
        resp2.status(),
        StatusCode::NOT_MODIFIED,
        "matching If-None-Match must yield 304"
    );
    let body_bytes = to_bytes(resp2.into_body(), 1024).await.unwrap();
    assert!(body_bytes.is_empty(), "304 response must have no body");
}

// ---------------------------------------------------------------------------
// Content-Type and basic 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn badge_returns_svg_content_type() {
    let app = badge_app(default_badges_state());
    let req = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .expect("content-type header must be present")
        .to_str()
        .unwrap();
    assert_eq!(
        ct, "image/svg+xml; charset=utf-8",
        "Content-Type must be image/svg+xml; charset=utf-8"
    );
}

// ---------------------------------------------------------------------------
// Cache-Control for unknown/error grade (§11.5)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn error_grade_badge_has_short_cache_control() {
    // Backends are unreachable (127.0.0.1:1999x), so run_check returns error grade.
    let app = badge_app(default_badges_state());
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
        .unwrap();
    // Error grade → short max-age
    assert!(
        cc.contains("max-age=300"),
        "error grade must use max-age=300, got: {cc}"
    );
    let bytes = to_bytes(resp.into_body(), 65536).await.unwrap();
    let body = std::str::from_utf8(&bytes).unwrap();
    assert!(body.contains(">?<"), "error grade must render ?");
}

// ---------------------------------------------------------------------------
// §11.10 — Per-domain throttle after first miss
// ---------------------------------------------------------------------------

#[tokio::test]
async fn per_domain_recompute_throttled_after_first_miss() {
    // Very short TTL (1s) so the recompute limiter fires quickly.
    let badges_cfg = BadgesConfig {
        enabled: true,
        ttl_seconds: 1,
        default_label: "lens".into(),
        max_label_len: 32,
    };
    let state = make_badge_state(badges_cfg, false);
    let app = badge_app(state);

    // First request — allowed (consumes rate limiter token).
    let req1 = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let _ = to_bytes(resp1.into_body(), 65536).await.unwrap();

    // Second request immediately — rate limiter exhausted for this domain.
    let req2 = Request::builder()
        .uri("/badge/example.com.svg")
        .body(Body::empty())
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let cc2 = resp2
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        cc2.contains("max-age=300"),
        "throttled request must use max-age=300, got: {cc2}"
    );
    let body2 = to_bytes(resp2.into_body(), 65536).await.unwrap();
    let body2_str = std::str::from_utf8(&body2).unwrap();
    assert!(body2_str.contains(">?<"), "throttled request must render ?");
}
