//! Metadata endpoints: health, servers, record types.

use axum::Json;
use mhost::RecordType;
use mhost::nameserver::NameServerConfig;
use mhost::nameserver::predefined::PredefinedProvider;
use serde::Serialize;

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
