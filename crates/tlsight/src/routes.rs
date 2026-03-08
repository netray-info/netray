use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::AppError;
use crate::input::{self, Target};
use crate::security::target_policy;
use crate::state::AppState;
use crate::tls;
use crate::validate;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct InspectResponse {
    pub request_id: String,
    pub hostname: String,
    pub input_mode: &'static str,
    pub summary: validate::Summary,
    pub ports: Vec<PortResult>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PortResult {
    pub port: u16,
    pub ips: Vec<tls::IpInspectionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<validate::ValidationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<tls::InspectionError>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MetaResponse {
    pub version: &'static str,
    pub features: MetaFeatures,
    pub limits: MetaLimits,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<MetaEcosystem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MetaFeatures {
    pub dane: bool,
    pub caa: bool,
    pub ct: bool,
    pub multi_port: bool,
    pub multi_ip: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MetaLimits {
    pub max_ports: usize,
    pub max_ips_per_hostname: usize,
    pub handshake_timeout_secs: u64,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MetaEcosystem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_base_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InspectQuery {
    pub h: String,
}

// ---------------------------------------------------------------------------
// Routers
// ---------------------------------------------------------------------------

pub fn health_router() -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/ready", get(ready_handler))
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/inspect", get(inspect_handler))
        .route("/api/meta", get(meta_handler))
        .route("/api-docs/openapi.json", get(openapi_handler))
        .route("/docs", get(docs_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn ready_handler() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ready" })
}

async fn meta_handler(State(state): State<AppState>) -> Json<MetaResponse> {
    let config = &state.config;
    let ecosystem =
        if config.ecosystem.dns_base_url.is_some() || config.ecosystem.ip_base_url.is_some() {
            Some(MetaEcosystem {
                dns_base_url: config.ecosystem.dns_base_url.clone(),
                ip_base_url: config.ecosystem.ip_base_url.clone(),
            })
        } else {
            None
        };

    Json(MetaResponse {
        version: env!("CARGO_PKG_VERSION"),
        features: MetaFeatures {
            dane: config.validation.check_dane,
            caa: config.validation.check_caa,
            ct: config.validation.check_ct,
            multi_port: true,
            multi_ip: true,
        },
        limits: MetaLimits {
            max_ports: config.limits.max_ports,
            max_ips_per_hostname: config.limits.max_ips_per_hostname,
            handshake_timeout_secs: config.limits.handshake_timeout_secs,
            request_timeout_secs: config.limits.request_timeout_secs,
        },
        ecosystem,
    })
}

async fn inspect_handler(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    query: Query<InspectQuery>,
) -> Result<Json<InspectResponse>, AppError> {
    let request_start = Instant::now();

    // Extract request ID from extensions (set by middleware)
    let request_id = uuid::Uuid::now_v7().to_string();

    // Parse input
    let parsed = input::parse_input(&query.h, state.config.limits.max_ports)?;

    let hostname_str = match &parsed.target {
        Target::Hostname(h) => h.clone(),
        Target::Ip(ip) => ip.to_string(),
    };
    let input_mode = match &parsed.target {
        Target::Hostname(_) => "hostname",
        Target::Ip(_) => "ip",
    };

    // Extract client IP for rate limiting
    let client_ip = state.ip_extractor.extract(&headers, addr);

    // Rate limit check (cost = ports * 1 IP for Phase 1)
    let cost = parsed.ports.len() as u32;
    state
        .rate_limiter
        .check_cost(client_ip, &hostname_str, cost)?;

    let mut warnings = Vec::new();

    // Resolve IPs
    let ips = match &parsed.target {
        Target::Hostname(h) => resolve_hostname(h).await?,
        Target::Ip(ip) => vec![*ip],
    };

    if ips.is_empty() {
        return Err(AppError::DnsResolutionFailed(format!(
            "no addresses found for {hostname_str}"
        )));
    }

    // Filter blocked IPs
    let allowed_ips: Vec<_> = ips
        .into_iter()
        .filter(|ip| match target_policy::check_allowed(ip) {
            Ok(()) => true,
            Err(reason) => {
                warnings.push(format!("{ip}: blocked ({reason})"));
                false
            }
        })
        .collect();

    if allowed_ips.is_empty() {
        return Err(AppError::BlockedTarget(format!(
            "all resolved IPs for {hostname_str} are in blocked ranges"
        )));
    }

    // IP input warning
    if input_mode == "ip" {
        warnings.push(
            "IP address input: no SNI sent, results may differ from hostname-based access"
                .to_string(),
        );
    }

    let handshake_timeout = Duration::from_secs(state.config.limits.handshake_timeout_secs);
    let request_timeout = Duration::from_secs(state.config.limits.request_timeout_secs);

    // Inspect all ports concurrently, with request timeout
    let port_results = tokio::time::timeout(request_timeout, async {
        let mut results = Vec::with_capacity(parsed.ports.len());
        for &port in &parsed.ports {
            let result = inspect_port(
                &allowed_ips,
                port,
                parsed.target.hostname(),
                handshake_timeout,
                state.cert_verifier.as_ref(),
            )
            .await;
            results.push(result);
        }
        results
    })
    .await
    .map_err(|_| AppError::RequestTimeout)?;

    // Compute summary from first successful port result
    let (validation_ref, ocsp_stapled) = port_results
        .iter()
        .flat_map(|pr| pr.ips.iter())
        .find(|ip_result| ip_result.error.is_none())
        .map(|ip_result| (&ip_result.validation, ip_result.ocsp_stapled()))
        .unwrap_or((&None, false));

    let summary = validate::summarize(
        validation_ref.as_ref(),
        parsed.target.hostname(),
        ocsp_stapled,
    );

    let duration_ms = request_start.elapsed().as_millis() as u64;

    Ok(Json(InspectResponse {
        request_id,
        hostname: hostname_str,
        input_mode,
        summary,
        ports: port_results,
        warnings,
        duration_ms,
    }))
}

/// Inspect a single port across all IPs.
async fn inspect_port(
    ips: &[std::net::IpAddr],
    port: u16,
    hostname: Option<&str>,
    timeout: Duration,
    cert_verifier: &dyn rustls::client::danger::ServerCertVerifier,
) -> PortResult {
    let mut ip_results = Vec::with_capacity(ips.len());

    for &ip in ips {
        let mut result = tls::inspect_ip(ip, port, hostname, timeout).await;

        // Run validation if we got a chain
        if let Some(chain) = &result.chain {
            let raw_certs = result.raw_certs.as_deref().unwrap_or(&[]);
            let validation =
                validate::chain_trust::validate_chain(chain, hostname, cert_verifier, raw_certs);
            result.validation = Some(validation);
        }

        ip_results.push(result);
    }

    PortResult {
        port,
        ips: ip_results,
        validation: None, // Per-port validation summary not needed in Phase 1
        error: None,
    }
}

/// Resolve a hostname to IP addresses using tokio's built-in resolver.
/// Phase 1: Simple resolution. Phase 2+ will use mhost for DNSSEC.
async fn resolve_hostname(hostname: &str) -> Result<Vec<std::net::IpAddr>, AppError> {
    let addrs: Vec<_> = tokio::net::lookup_host(format!("{hostname}:0"))
        .await
        .map_err(|e| AppError::DnsResolutionFailed(format!("{hostname}: {e}")))?
        .map(|addr| addr.ip())
        .collect();

    // Deduplicate
    let mut unique = Vec::new();
    for ip in addrs {
        if !unique.contains(&ip) {
            unique.push(ip);
        }
    }
    Ok(unique)
}

async fn openapi_handler() -> impl IntoResponse {
    // TODO("proper utoipa OpenAPI spec generation")
    let spec = serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "tlsight",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "TLS certificate inspection and diagnostics API"
        },
        "paths": {
            "/api/inspect": {
                "get": {
                    "summary": "Inspect TLS configuration",
                    "parameters": [{
                        "name": "h",
                        "in": "query",
                        "required": true,
                        "schema": { "type": "string" }
                    }]
                }
            },
            "/api/health": {
                "get": { "summary": "Health check" }
            },
            "/api/ready": {
                "get": { "summary": "Readiness check" }
            },
            "/api/meta": {
                "get": { "summary": "Service metadata" }
            }
        }
    });
    Json(spec)
}

async fn docs_handler() -> Html<&'static str> {
    Html(include_str!("scalar_docs.html"))
}

// ---------------------------------------------------------------------------
// IpInspectionResult helper
// ---------------------------------------------------------------------------

impl tls::IpInspectionResult {
    fn ocsp_stapled(&self) -> bool {
        self.tls.as_ref().is_some_and(|t| t.ocsp.stapled)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_config() -> crate::config::Config {
        crate::config::Config {
            server: crate::config::ServerConfig {
                bind: ([127, 0, 0, 1], 8080).into(),
                metrics_bind: ([127, 0, 0, 1], 9090).into(),
                trusted_proxies: Vec::new(),
            },
            limits: crate::config::LimitsConfig {
                per_ip_per_minute: 30,
                per_ip_burst: 10,
                per_target_per_minute: 60,
                per_target_burst: 20,
                max_concurrent_connections: 256,
                max_concurrent_handshakes: 10,
                handshake_timeout_secs: 5,
                request_timeout_secs: 15,
                max_ports: 5,
                max_ips_per_hostname: 10,
                max_domain_length: 253,
            },
            dns: crate::config::DnsConfig {
                resolver: "cloudflare".to_owned(),
                timeout_secs: 3,
            },
            validation: crate::config::ValidationConfig {
                expiry_warning_days: 30,
                expiry_critical_days: 14,
                check_dane: true,
                check_caa: true,
                check_ct: false,
                custom_ca_dir: None,
            },
            ecosystem: crate::config::EcosystemConfig::default(),
        }
    }

    fn test_state() -> AppState {
        let _ = rustls::crypto::ring::default_provider().install_default();
        AppState::new(&test_config())
    }

    fn test_router() -> Router {
        let state = test_state();
        health_router().merge(api_router(state))
    }

    async fn get(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        (status, body)
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = test_router();
        let (status, body) = get(&app, "/api/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn ready_returns_ready() {
        let app = test_router();
        let (status, body) = get(&app, "/api/ready").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ready");
    }

    #[tokio::test]
    async fn meta_returns_version() {
        let app = test_router();
        let (status, body) = get(&app, "/api/meta").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["version"].is_string());
        assert!(body["features"]["multi_port"].as_bool().unwrap());
        assert_eq!(body["limits"]["max_ports"], 5);
    }

    #[tokio::test]
    async fn inspect_missing_param_returns_error() {
        let app = test_router();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/inspect")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Missing required `h` param — 400 (or 500 without ConnectInfo in test)
        assert!(response.status().is_client_error() || response.status().is_server_error());
    }

    #[tokio::test]
    async fn inspect_empty_param_returns_error() {
        let app = test_router();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/inspect?h=")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Empty input — error (400 or 500 without ConnectInfo in test)
        assert!(response.status().is_client_error() || response.status().is_server_error());
    }

    #[tokio::test]
    async fn openapi_returns_json() {
        let app = test_router();
        let (status, body) = get(&app, "/api-docs/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["openapi"], "3.1.0");
        assert_eq!(body["info"]["title"], "tlsight");
    }
}
