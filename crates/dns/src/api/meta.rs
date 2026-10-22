//! Metadata endpoints: health, ready, servers, record types.

use mhost::RecordType;
use mhost::nameserver::NameServerConfig;
use mhost::nameserver::predefined::PredefinedProvider;
use serde::Serialize;

// ---------------------------------------------------------------------------
// GET /health
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GET /ready
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GET /api/servers
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct ServerInfo {
    /// Provider name (e.g. `"cloudflare"`).
    name: String,
    configs: Vec<ServerConfigInfo>,
}

impl ServerInfo {
    pub fn name(&self) -> &str {
        &self.name
    }
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

/// Every predefined DNS provider with its transport configurations.
pub fn server_catalogue() -> Vec<ServerInfo> {
    PredefinedProvider::all()
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
        .collect()
}

// ---------------------------------------------------------------------------
// GET /api/record-types
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct RecordTypeInfo {
    /// DNS record type name (e.g. `"A"`, `"MX"`, `"TXT"`).
    pub name: String,
}

// ---------------------------------------------------------------------------
// GET /api/config
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GET /api/record-types
// ---------------------------------------------------------------------------

/// The names of every record type a query may ask for.
pub fn record_type_names() -> Vec<String> {
    RecordType::all()
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
        .map(|rt| rt.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------
