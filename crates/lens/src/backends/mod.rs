use std::collections::HashMap;
use std::net::IpAddr;

use crate::check::SectionError;
use crate::scoring::engine::CheckResult;

/// Section-specific extra data produced by each backend.
#[derive(Clone, Debug)]
pub enum BackendExtra {
    Dns {
        resolved_ips: Vec<IpAddr>,
        raw_headline: String,
        detail_url: String,
    },
    Tls {
        raw_headline: String,
        detail_url: String,
    },
    Http {
        raw_headline: String,
        detail_url: String,
        status_code: Option<u16>,
        http_version: Option<String>,
        response_duration_ms: Option<u64>,
        server_ip: Option<String>,
        server_org: Option<String>,
        server_network_type: Option<String>,
    },
    Ip {
        addresses: Vec<IpInfo>,
        raw_headline: String,
        detail_url: String,
    },
    Email {
        raw_headline: String,
        detail_url: String,
        grade: Option<String>,
        /// Bucket name → reason for buckets that are not-applicable (e.g. no MX records).
        bucket_na: HashMap<String, String>,
    },
}

/// One enriched public address of the IP section.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct IpInfo {
    pub ip: IpAddr,
    pub org: Option<String>,
    pub geo: Option<String>,
    pub network_type: String,
}

#[derive(Clone, Debug)]
pub struct BackendResult {
    pub checks: Vec<CheckResult>,
    pub extra: BackendExtra,
}

/// Cross-section context passed from wave 1 to wave 2.
#[derive(Clone)]
pub struct BackendContext {
    pub resolved_ips: Vec<IpAddr>,
    /// Validated DKIM selectors forwarded to the email backend.
    pub dkim_selectors: Option<Vec<String>>,
    /// Request headers for an HTTP backend; every section runs in-process, so none is read.
    pub forward_headers: reqwest::header::HeaderMap,
}

/// Minimal percent-encoding for query string values (RFC 3986 unreserved set).
pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

pub trait Backend: Send + Sync {
    /// Must match the key in `ScoringProfile.sections`.
    fn section(&self) -> &'static str;

    /// Run the backend. Each backend owns its own HTTP client and timeout.
    fn run(
        &self,
        domain: &str,
        context: &BackendContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>,
    >;
}
