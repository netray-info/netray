//! `/api/v1/docs` (spec v2-api, requirement 35): the Scalar page, its CSP, its rate-limit
//! exemption, its absence from the document, and the `[api] enabled = false` switch.
use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
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

fn app(enabled: bool) -> Router {
    let mut config = base_config();
    config.api.enabled = enabled;
    lens::app(AppState::new(config).unwrap())
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))))
}

struct Answer {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

async fn get(app: &Router, uri: &str) -> Answer {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    Answer {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn header(a: &Answer, name: &str) -> String {
    a.headers
        .get(name)
        .unwrap_or_else(|| panic!("missing header {name}"))
        .to_str()
        .unwrap()
        .to_string()
}

fn script_src_directive(csp: &str) -> &str {
    csp.split(';')
        .map(str::trim)
        .find(|d| d.starts_with("script-src "))
        .expect("CSP has a script-src directive")
}

/// Origins (`scheme://host`) of every `<script ... src="...">` with an absolute URL.
fn script_origins(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<script") {
        rest = &rest[i + 7..];
        let tag_end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..tag_end];
        if let Some(s) = tag.find("src=") {
            let v = &tag[s + 4..];
            let quote = v.chars().next().unwrap();
            let v = &v[1..];
            let url = &v[..v.find(quote).unwrap_or(v.len())];
            if let Some(after) = url.find("://") {
                let host_end = url[after + 3..]
                    .find('/')
                    .map_or(url.len(), |j| after + 3 + j);
                out.push(url[..host_end].to_string());
            }
        }
    }
    out
}

#[tokio::test]
async fn api_v1_docs_page_csp_exemptions_and_switch() {
    let app = app(true);

    // 200 text/html referencing the document.
    let docs = get(&app, "/api/v1/docs").await;
    assert_eq!(docs.status, StatusCode::OK);
    assert!(header(&docs, "content-type").starts_with("text/html"));
    assert!(docs.body.contains("/api/v1/openapi.json"));

    // Relaxed CSP equals lens's /docs; the catalogue keeps the strict one.
    let lens_docs = get(&app, "/docs").await;
    let docs_csp = header(&docs, "content-security-policy");
    assert_eq!(docs_csp, header(&lens_docs, "content-security-policy"));
    let strict = header(&get(&app, "/api/meta").await, "content-security-policy");
    assert_ne!(strict, docs_csp);
    let catalogue = get(&app, "/api/v1/catalogue").await;
    assert_eq!(header(&catalogue, "content-security-policy"), strict);

    // Every external script host of the page is allowed by the docs CSP.
    let allowed = script_src_directive(&docs_csp);
    for origin in script_origins(&docs.body) {
        assert!(
            allowed.split_whitespace().any(|t| t == origin),
            "script origin {origin} not in `{allowed}`"
        );
    }

    // Exempt from the per-IP limiter (burst 10): 11 requests, none limited.
    for n in 1..=11 {
        let a = get(&app, "/api/v1/docs").await;
        assert_ne!(
            a.status,
            StatusCode::TOO_MANY_REQUESTS,
            "request {n} limited"
        );
    }

    // Not a path of the document.
    let doc = netray_web::api::openapi();
    let paths: Vec<&String> = doc
        .paths
        .paths
        .keys()
        .filter(|p| p.contains("/docs"))
        .collect();
    assert!(paths.is_empty(), "document lists {paths:?}");

    // [api] enabled = false: the Scalar page is not served.
    let off = get(&self::app(false), "/api/v1/docs").await;
    assert!(
        off.status != StatusCode::OK || !off.body.contains("/api/v1/openapi.json"),
        "docs page served with api disabled"
    );
}
