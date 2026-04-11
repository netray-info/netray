//! Metadata endpoints: health, ready, servers, record types.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mhost::RecordType;
use mhost::nameserver::NameServerConfig;
use mhost::nameserver::predefined::PredefinedProvider;
use serde::Serialize;

use crate::api::AppState;

// ---------------------------------------------------------------------------
// GET /health
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    /// Always `"ok"` when the service is healthy.
    status: &'static str,
}

/// Health check. Returns `{"status":"ok"}` when the service is running.
#[utoipa::path(
    get, path = "/health",
    tag = "Probes",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse),
    )
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

// ---------------------------------------------------------------------------
// GET /ready
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReadyResponse {
    /// Always `"ok"`. Degraded state is indicated via the `warnings` array.
    status: &'static str,
    /// Non-fatal warnings about degraded dependencies (e.g. enrichment unreachable).
    warnings: Vec<String>,
}

/// Readiness probe. Always returns 200; degraded state is reported via `warnings`.
///
/// Checks:
/// - Open circuit breakers
/// - Enrichment service reachability (HEAD request, 2 s timeout)
#[utoipa::path(
    get, path = "/ready",
    tag = "Probes",
    responses(
        (status = 200, description = "Service is ready (warnings array may be non-empty if degraded)", body = ReadyResponse),
    )
)]
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadyResponse>) {
    let mut warnings: Vec<String> = Vec::new();

    if state.circuit_breakers.any_open() {
        warnings.push("circuit breaker open for one or more DNS providers".to_owned());
    }

    if let Some(ref client) = state.ip_enrichment {
        let reachable = probe_tcp(client.base_url()).await;
        if !reachable {
            warnings.push(format!("enrichment service unreachable: {}", client.base_url()));
        }
    }

    (
        StatusCode::OK,
        Json(ReadyResponse {
            status: "ok",
            warnings,
        }),
    )
}

// ---------------------------------------------------------------------------
// GET /api/servers
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct ServerInfo {
    /// Provider name (e.g. `"cloudflare"`).
    name: String,
    configs: Vec<ServerConfigInfo>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ServerConfigInfo {
    /// Human-readable string representation (e.g. "udp:1.1.1.1:53,name=Cloudflare 1").
    description: String,
    /// Transport protocol: `udp`, `tcp`, `tls`, or `https`.
    protocol: String,
}

fn protocol_str(config: &NameServerConfig) -> &'static str {
    match config {
        NameServerConfig::Udp { .. } => "udp",
        NameServerConfig::Tcp { .. } => "tcp",
        NameServerConfig::Tls { .. } => "tls",
        NameServerConfig::Https { .. } => "https",
    }
}

/// List all predefined DNS providers with their transport configurations.
#[utoipa::path(
    get, path = "/api/servers",
    tag = "Metadata",
    responses(
        (status = 200, description = "List of predefined DNS providers", body = Vec<ServerInfo>),
    )
)]
pub async fn servers() -> Json<Vec<ServerInfo>> {
    let providers: Vec<ServerInfo> = PredefinedProvider::all()
        .iter()
        .map(|p| {
            let configs = p
                .configs()
                .into_iter()
                .map(|c| ServerConfigInfo {
                    protocol: protocol_str(&c).to_owned(),
                    description: c.to_string(),
                })
                .collect();
            ServerInfo {
                name: p.to_string(),
                configs,
            }
        })
        .collect();
    Json(providers)
}

// ---------------------------------------------------------------------------
// GET /api/record-types
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct RecordTypeInfo {
    /// DNS record type name (e.g. `"A"`, `"MX"`, `"TXT"`).
    name: String,
}

// ---------------------------------------------------------------------------
// GET /api/config
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct ClientConfig {
    /// Display name shown in the UI.
    site_name: String,
    /// Service version.
    version: &'static str,
    /// Public ifconfig URL for IP lookups, or null if not configured.
    ifconfig_url: Option<String>,
    /// Public TLS inspector URL for cross-links, or null if not configured.
    tls_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ecosystem: Option<netray_common::ecosystem::EcosystemConfig>,
}

/// Returns client-facing configuration (e.g. site name, ifconfig URL for IP links).
#[utoipa::path(
    get, path = "/api/config",
    tag = "Metadata",
    responses(
        (status = 200, description = "Client configuration", body = ClientConfig),
    )
)]
pub async fn client_config(State(state): State<AppState>) -> Json<ClientConfig> {
    let eco = &state.config.ecosystem;
    let ecosystem = if eco.has_any() {
        Some(eco.clone())
    } else {
        None
    };
    Json(ClientConfig {
        site_name: state.config.site_name.clone(),
        version: env!("CARGO_PKG_VERSION"),
        ifconfig_url: eco.ip_base_url.clone(),
        tls_url: eco.tls_base_url.clone(),
        ecosystem,
    })
}

// ---------------------------------------------------------------------------
// GET /api/record-types
// ---------------------------------------------------------------------------

/// List all supported DNS record types (excludes ANY, AXFR, IXFR, OPT, ZERO, NULL).
#[utoipa::path(
    get, path = "/api/record-types",
    tag = "Metadata",
    responses(
        (status = 200, description = "List of supported DNS record types", body = Vec<RecordTypeInfo>),
    )
)]
pub async fn record_types() -> Json<Vec<RecordTypeInfo>> {
    let types: Vec<RecordTypeInfo> = RecordType::all()
        .into_iter()
        .filter(|rt| {
            !matches!(
                rt,
                RecordType::ANY
                    | RecordType::AXFR
                    | RecordType::IXFR
                    | RecordType::OPT
                    | RecordType::ZERO
                    | RecordType::NULL
            )
        })
        .map(|rt| RecordTypeInfo {
            name: rt.to_string(),
        })
        .collect();
    Json(types)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Attempt a TCP connect to the host:port derived from `url` within 2 seconds.
///
/// Returns `true` if the connection succeeds, `false` on timeout or error.
/// Used by the readiness probe to check enrichment service reachability without
/// adding a full HTTP client dependency.
async fn probe_tcp(url: &str) -> bool {
    // Strip scheme and path — only the host and port matter.
    let hostport = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("");

    // Infer default port from scheme.
    let addr = if hostport.contains(':') {
        hostport.to_owned()
    } else if url.starts_with("https://") {
        format!("{hostport}:443")
    } else {
        format!("{hostport}:80")
    };

    if addr.is_empty() {
        return false;
    }

    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::net::TcpStream::connect(addr.as_str()),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}
