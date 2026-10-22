//! DNSSEC chain-of-trust endpoint: walk the delegation chain querying DNSKEY/DS/RRSIG
//! at each zone level, streaming results as SSE.
//!
//! - `POST /api/dnssec` — accept a JSON body with domain and optional timeout_secs.
//!
//! Performs iterative resolution from root servers, querying DNSSEC records at each
//! delegation level. Emits one `chain` SSE event per zone level and a final `done` event.

use serde::Deserialize;

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct DnssecRequest {
    /// Domain name to validate (e.g. `"example.com"`).
    pub domain: String,
    /// Query timeout per level in seconds. Clamped to config max.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}
