//! Trace endpoint: walk the DNS delegation chain for a domain, streaming results as SSE.
//!
//! - `POST /api/trace` — accept a JSON body with domain and optional record_type/timeout_secs.
//!
//! Performs iterative, non-recursive resolution from root servers, emitting one `hop` SSE
//! event per delegation level and a final `done` event with summary counts.

use serde::Deserialize;

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct TraceRequest {
    /// Domain name to trace (e.g. `"example.com"`).
    pub domain: String,
    /// DNS record type to trace (default: `"A"`).
    #[serde(default = "default_record_type")]
    pub record_type: String,
    /// Query timeout per hop in seconds. Clamped to config max.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

fn default_record_type() -> String {
    "A".to_owned()
}
