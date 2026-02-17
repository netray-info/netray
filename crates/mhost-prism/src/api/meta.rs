//! Metadata endpoints: health, servers, record types.

use axum::Json;
use mhost::RecordType;
use mhost::nameserver::NameServerConfig;
use mhost::nameserver::predefined::PredefinedProvider;
use serde::Serialize;

// ---------------------------------------------------------------------------
// GET /api/health
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

// ---------------------------------------------------------------------------
// GET /api/servers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ServerInfo {
    name: String,
    configs: Vec<ServerConfigInfo>,
}

#[derive(Serialize)]
pub struct ServerConfigInfo {
    /// Human-readable string representation (e.g. "udp:1.1.1.1:53,name=Cloudflare 1").
    description: String,
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

#[derive(Serialize)]
pub struct RecordTypeInfo {
    name: String,
}

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
