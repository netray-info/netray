use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::quality::types::{CheckStatus, QualityReport};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InspectResponse {
    pub url: String,
    pub final_url: String,
    pub timestamp: String, // ISO 8601
    pub duration_ms: u64,
    pub http_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_svc: Option<String>,
    pub status: u16,
    pub redirects: Vec<RedirectHop>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_upgrade: Option<HttpUpgrade>,
    #[schema(value_type = Object, additional_properties)]
    pub headers: IndexMap<String, String>,
    pub security: SecurityReport,
    pub cors: CorsReport,
    pub cookies: Vec<CookieEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
    pub caching: CachingReport,
    pub cdn: CdnReport,
    pub fingerprint: FingerprintReport,
    pub deprecated_headers: Vec<String>,
    pub reporting: ReportingReport,
    pub quality: QualityReport,
    pub enrichment: EnrichmentInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProbeError>,
    /// Internal: set when redirect limit was reached. Not serialized.
    #[serde(skip)]
    pub redirect_limit_reached: Option<usize>,
    /// Internal: every HTTPS response header as received, lowercased name and value, repeats
    /// kept. Not serialized; empty after a JSON replay.
    #[serde(skip)]
    pub raw_headers: Vec<(String, String)>,
    /// Internal: the lowercased names of HTTPS response headers whose first line is not readable
    /// as text (`HeaderValue::to_str` fails). Not serialized; empty after a JSON replay.
    #[serde(skip)]
    pub unreadable_headers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RedirectHop {
    pub url: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub http_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HttpUpgrade {
    pub redirects_to_https: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    pub same_host: bool,
    pub message: String,
    pub redirects: Vec<RedirectHop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProbeError>,
    /// Internal: the probed port-80 URL. Not serialized.
    #[serde(skip)]
    pub url: Option<String>,
    /// Internal: the `Location` of the response that ended the followed port-80 chain, not of
    /// port 80's own first answer. Not serialized.
    #[serde(skip)]
    pub location: Option<String>,
}

/// Why a probe got no response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ProbeError {
    pub kind: ProbeErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProbeErrorKind {
    Timeout,
    Refused,
    Reset,
    Tls,
    Blocked,
    Unreachable,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SecurityReport {
    pub hsts: HstsCheck,
    pub csp: CspReport,
    pub x_frame_options: HeaderCheck,
    pub permissions_policy: HeaderCheck,
    pub x_content_type_options: HeaderCheck,
    pub referrer_policy: HeaderCheck,
    pub coop: HeaderCheck,
    pub coep: HeaderCheck,
    pub corp: HeaderCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HstsCheck {
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u64>,
    pub include_sub_domains: bool,
    pub preload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CspReport {
    pub status: CheckStatus,
    pub enforced: bool,
    pub report_only: bool,
    #[schema(value_type = Object, additional_properties)]
    pub directives: IndexMap<String, Vec<String>>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HeaderCheck {
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CorsReport {
    pub allows_any_origin: bool,
    pub reflects_origin: bool,
    pub allows_credentials: bool,
    pub status: CheckStatus,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProbeError>,
    /// Internal: the raw `Access-Control-Allow-Origin` value. Not serialized.
    #[serde(skip)]
    pub allow_origin: Option<String>,
    /// Internal: the raw `Access-Control-Allow-Credentials` value. Not serialized.
    #[serde(skip)]
    pub allow_credentials_header: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CookieEntry {
    pub name: String,
    pub secure: bool,
    pub httponly: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub samesite: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub expires: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CachingReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<String>,
    pub directives: CacheControlDirectives,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    pub vary: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CacheControlDirectives {
    pub public: bool,
    pub private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u64>,
    pub no_store: bool,
    pub no_cache: bool,
    pub must_revalidate: bool,
    pub immutable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CdnReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_status: Option<String>,
    pub indicators: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FingerprintReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    pub info_leakage: InfoLeakage,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InfoLeakage {
    pub status: CheckStatus,
    pub exposed_headers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReportingReport {
    pub report_to: bool,
    pub nel: bool,
    pub csp_reporting: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct EnrichmentInfo {
    pub ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    /// Public IP-tool page for `ip`; absent when no `[meta] ip_base_url` is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_url: Option<String>,
    /// IP classification: "cloud", "datacenter", "residential", "vpn", "cdn", "isp", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_type: Option<String>,
    /// Threat flag: "C2", "DROP", or "TOR" if the IP is flagged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threat: Option<String>,
    /// ASN network role (e.g. "Midsize Transit", "Access Provider").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}
