//! Mount rules for the result router in `lens::app` (spec v2-result-page, Phase 2: C1 lens half, C16, C21).
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, RETENTION_SECS, Recorded};

const ENTRY_DOMAIN: &str = "shop.example.com";
const NOW: i64 = RETENTION_SECS + 5000;

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

fn a_run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

async fn get(app: &Router, uri: &str) -> (StatusCode, String, String, Option<String>) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let loc = resp
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        ct,
        String::from_utf8_lossy(&bytes).into_owned(),
        loc,
    )
}

const SPA_SHELL: &str = "<div id=\"root\"";
const IDLE_FORM: &str = "action=\"/\"";

/// Returns the app with history enabled, and the id of the recorded shop.example.com entry.
async fn app_with_entry() -> (Router, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = base_config();
    config.history.enabled = true;
    config.history.db_path = dir.path().join("history.db");
    config.snapshots.enabled = false;
    let history = HistoryStore::open(&config.history.db_path).await.unwrap();
    history.migrate().await.unwrap();
    let history = Arc::new(history);
    let id = match history
        .record(ENTRY_DOMAIN, &a_run(), &[], NOW, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    let mut state = AppState::new(config).unwrap();
    state.history_store = Some(history);
    (lens::app(state), id.to_string(), dir)
}

fn app_without_history() -> Router {
    let mut config = base_config();
    config.snapshots.enabled = false;
    lens::app(AppState::new(config).unwrap())
}

fn is_result_router_page(body: &str) -> bool {
    // The idle page's inspector line; every frame links /inspect/dns in its menu and footer.
    body.contains("id=\"result\"")
        || (body.contains(IDLE_FORM) && body.contains("One protocol only"))
}

#[tokio::test]
async fn lens_serves_idle_and_result_pages_with_history_enabled() {
    let (app, id, _dir) = app_with_entry().await;

    let (status, _, body, _) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains(SPA_SHELL), "GET / is still the SPA shell");
    assert!(body.contains(IDLE_FORM), "idle form missing: {body}");
    assert!(body.contains("name=\"d\""), "idle input missing: {body}");
    assert!(body.contains("/inspect/dns"), "idle links missing: {body}");

    let (status, _, body, _) = get(&app, "/shop.example.com").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains(SPA_SHELL), "result is still the SPA shell");
    assert!(
        body.contains("id=\"result\""),
        "result page missing: {body}"
    );
    assert!(body.contains(&id), "entry id {id} missing: {body}");

    let (status, _, _, loc) = get(&app, "/?d=Example.com").await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(loc.as_deref(), Some("/example.com"));
}

#[tokio::test]
async fn result_router_does_not_answer_robots_share_or_api_paths() {
    let (app, id, _dir) = app_with_entry().await;
    // Entry exists, so a stray match of the result router would show id="result".
    assert!(!id.is_empty());

    let (status, ct, body, _) = get(&app, "/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("text/plain"), "robots content-type: {ct}");
    assert!(body.contains("User-agent"), "robots body: {body}");
    assert!(!is_result_router_page(&body));

    for uri in ["/r/abcdefgh", "/api/check/example.com"] {
        let (_, _, body, _) = get(&app, uri).await;
        assert!(
            !is_result_router_page(&body),
            "{uri} answered by result router: {body}"
        );
    }

    // Guard against the vacuous pass: the result router must exist at all.
    let (_, _, body, _) = get(&app, "/").await;
    assert!(
        is_result_router_page(&body),
        "result router not mounted at /"
    );
}

#[tokio::test]
async fn result_router_does_not_answer_static_and_well_known_paths() {
    let (app, _id, _dir) = app_with_entry().await;
    let plain = app_without_history();

    for uri in [
        "/favicon.svg",
        "/favicon.ico",
        "/bimi-logo.svg",
        "/.well-known/mta-sts.txt",
    ] {
        let (status, ct, body, _) = get(&app, uri).await;
        let (p_status, p_ct, _, _) = get(&plain, uri).await;
        assert_eq!(
            status, p_status,
            "{uri}: status differs from history-less app"
        );
        assert_eq!(
            ct, p_ct,
            "{uri}: content-type differs from history-less app"
        );
        assert!(!body.contains("id=\"result\""), "{uri}: result page served");
    }

    let (_, _, body, _) = get(&app, "/").await;
    assert!(
        is_result_router_page(&body),
        "result router not mounted at /"
    );
}
