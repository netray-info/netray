//! The inspector routes on `lens::app` (spec v2-inspectors, requirement 13): the real
//! `InspectorJobs` and v1's `IpExtractor` are mounted ahead of `spa::handler`.
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use netray_ip::lookup::{IpLookup, IpRefusal};
use netray_ip::testing::StubIpLookup;
use netray_web::api::ApiConfig;
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;

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

/// Records the address every caller lookup was made for, answers like the stub.
#[derive(Default)]
struct Recording {
    callers: Mutex<Vec<IpAddr>>,
    inner: StubIpLookup,
}

impl IpLookup for Recording {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<netray_ip::backend::Ifconfig, IpRefusal>> {
        self.inner.lookup(ip)
    }

    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<netray_ip::backend::Ifconfig, IpRefusal>> {
        self.callers.lock().unwrap().push(remote.ip());
        self.inner.lookup_caller(remote, user_agent)
    }

    fn range<'a>(
        &'a self,
        cidr: &'a str,
    ) -> BoxFuture<'a, Result<netray_ip::lookup::RangeResponse, IpRefusal>> {
        self.inner.range(cidr)
    }

    fn asn(
        &self,
        n: u32,
    ) -> BoxFuture<'_, Result<netray_ip::lookup::AsnLookupResponse, IpRefusal>> {
        self.inner.asn(n)
    }
}

async fn send(app: &Router, mut req: Request<Body>, peer: &str) -> (StatusCode, String) {
    let peer: SocketAddr = format!("{peer}:40000").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn post_empty_ip() -> Request<Body> {
    Request::post("/inspect/ip/run")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("x-forwarded-for", "192.0.2.7")
        .body(Body::from("ip="))
        .unwrap()
}

#[tokio::test]
async fn inspector_routes_are_mounted_ahead_of_the_spa() {
    let (_, spa_shell) = send(
        &lens::app(AppState::new(base_config()).unwrap()),
        get("/some/client/route"),
        "192.0.2.1",
    )
    .await;
    assert!(spa_shell.contains("<div id=\"root\""));

    for api_enabled in [true, false] {
        let mut config = base_config();
        config.api = ApiConfig {
            enabled: api_enabled,
            ..Default::default()
        };
        let app = lens::app(AppState::new(config).unwrap());

        let (status, body) = send(&app, get("/inspect/tls"), "192.0.2.1").await;
        assert_eq!(status, StatusCode::OK, "api enabled={api_enabled}");
        assert!(
            body.contains(r#"data-stream-url="/inspect/tls/run""#),
            "api enabled={api_enabled}: {body}"
        );
        assert!(
            !body.contains("<div id=\"root\""),
            "api enabled={api_enabled}"
        );

        // A client route outside the inspector still reaches the SPA handler.
        let (status, body) = send(&app, get("/example.com/smtp"), "192.0.2.1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, spa_shell);
    }
}

#[tokio::test]
async fn real_jobs_answer_unavailable_without_an_ip_part() {
    let mut state = AppState::new(base_config()).unwrap();
    state.api_parts.ip = None;
    let app = lens::app(state);

    let (status, body) = send(&app, post_empty_ip(), "192.0.2.1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("IP lookups are unavailable"), "body: {body}");
}

#[tokio::test]
async fn empty_ip_input_looks_up_the_caller_v1s_extractor_derives() {
    let mut config = base_config();
    config.server.trusted_proxies = vec!["203.0.113.9".to_string()];
    let lookup = Arc::new(Recording::default());
    let mut state = AppState::new(config).unwrap();
    state.api_parts.ip = Some(lookup.clone());
    let app = lens::app(state);

    let (status, body) = send(&app, post_empty_ip(), "203.0.113.9").await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        *lookup.callers.lock().unwrap(),
        vec!["192.0.2.7".parse::<IpAddr>().unwrap()],
        "body: {body}"
    );
}
