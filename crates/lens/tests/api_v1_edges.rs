//! `/api/v1` mount edges in `lens::app` (spec v2-api): trailing slash, body limit errors,
//! the scoring profile, the metrics namespace.
use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;
use netray_common::server::metrics_recorder;

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
        api: Default::default(),
    }
}

fn app(config: Config) -> Router {
    lens::app(AppState::new(config).unwrap())
}

async fn send(app: &Router, mut req: Request<Body>) -> (StatusCode, Option<String>, String) {
    let peer: SocketAddr = "203.0.113.9:4000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, ct, String::from_utf8_lossy(&bytes).into_owned())
}

fn is_json(ct: &Option<String>) -> bool {
    ct.as_deref().is_some_and(|c| c.starts_with("application/json"))
}

#[tokio::test]
async fn trailing_slash_under_the_mount_is_a_json_404_not_the_spa() {
    let (s, ct, body) = send(
        &app(base_config()),
        Request::get("/api/v1/").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(is_json(&ct), "content-type {ct:?}, body {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"]["code"], "NOT_FOUND");
}

#[tokio::test]
async fn oversized_body_is_a_json_413_on_v1_and_a_plain_413_elsewhere() {
    let app = app(base_config());
    let req = Request::get("/api/v1/catalogue")
        .header("content-length", "9000")
        .body(Body::from(vec![b'a'; 9000]))
        .unwrap();
    let (s, ct, body) = send(&app, req).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(is_json(&ct), "content-type {ct:?}, body {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"]["code"], "PAYLOAD_TOO_LARGE");

    let req = Request::post("/api/check")
        .header("content-type", "application/json")
        .header("content-length", "9000")
        .body(Body::from(vec![b'a'; 9000]))
        .unwrap();
    let (s, _, _) = send(&app, req).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn scoring_profile_path_sets_the_profile_v1_reports() {
    let text = include_str!("../../engine/profiles/v3.toml");
    assert!(text.contains("version = 3"));
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), text.replacen("version = 3", "version = 4", 1)).unwrap();
    let mut config = base_config();
    config.scoring.profile_path = Some(file.path().to_string_lossy().into_owned());
    let app = app(config);

    let (s, _, body) = send(&app, Request::get("/api/v1/meta").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["current_profile"], 4);

    let (s, _, body) = send(
        &app,
        Request::get("/api/v1/profiles/4").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
}

#[test]
fn zero_series_include_the_api_v1_rate_limit_scope() {
    let recorder = metrics_recorder(lens::metrics::HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    lens::metrics::init_zero_series();
    let out = handle.render();
    assert!(
        out.lines()
            .any(|l| l == r#"lens_rate_limit_hits_total{scope="api_v1"} 0"#),
        "missing api_v1 zero series in:\n{out}"
    );
}
