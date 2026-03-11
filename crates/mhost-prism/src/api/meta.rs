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
// GET /api/health
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    /// Always `"ok"` when the service is healthy.
    status: &'static str,
}

/// Health check. Returns `{"status":"ok"}` when the service is running.
#[utoipa::path(
    get, path = "/api/health",
    tag = "Probes",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse),
    )
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

// ---------------------------------------------------------------------------
// GET /api/ready
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReadyResponse {
    /// `"ready"` when all circuit breakers are closed; `"degraded"` when any is open.
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

/// Readiness probe. Returns 200 when no circuit breakers are open, 503 when degraded.
#[utoipa::path(
    get, path = "/api/ready",
    tag = "Probes",
    responses(
        (status = 200, description = "Service is ready to handle traffic", body = ReadyResponse),
        (status = 503, description = "Service is degraded (circuit breaker open)", body = ReadyResponse),
    )
)]
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadyResponse>) {
    if state.circuit_breakers.any_open() {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadyResponse {
                status: "degraded",
                reason: Some("circuit breaker open"),
            }),
        )
    } else {
        (
            StatusCode::OK,
            Json(ReadyResponse {
                status: "ready",
                reason: None,
            }),
        )
    }
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
    Json(ClientConfig {
        site_name: state.config.site_name.clone(),
        version: env!("CARGO_PKG_VERSION"),
        ifconfig_url: state.config.ecosystem.ifconfig_url.clone(),
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
