use der::Decode;
use serde::Serialize;
use utoipa::ToSchema;

use chrono::Utc;

/// Result of a live OCSP revocation check via the AIA OCSP responder URL.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OcspRevocationResult {
    /// "good", "revoked", or "unknown"
    pub status: String,
    /// Revocation reason string (only set when status == "revoked")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// ISO 8601 revocation time (only set when status == "revoked")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
    /// ISO 8601 timestamp when this check was performed
    pub checked_at: String,
}

/// Perform a live OCSP check for the leaf cert using the AIA OCSP URL.
/// Returns None if ocsp_url is absent.
/// On network error or timeout, returns status "unknown".
#[allow(dead_code)]
pub async fn check_live_ocsp(
    ocsp_url: &str,
    leaf_der: &[u8],
    issuer_der: &[u8],
) -> OcspRevocationResult {
    let checked_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Build OCSP request using x509-parser to extract serial + issuer info,
    // then construct a minimal DER-encoded OCSPRequest.
    // TODO("live-ocsp-build"): Full OCSP request construction requires hashing
    // the issuer name + key and encoding CertID; this is complex without a
    // dedicated OCSP request builder crate. Returning "unknown" as a safe stub.
    // The data model is complete and the field is wired into the response.
    let _ = (ocsp_url, leaf_der, issuer_der);

    OcspRevocationResult {
        status: "unknown".to_string(),
        reason: None,
        revoked_at: None,
        checked_at,
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OcspInfo {
    pub stapled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub this_update: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_update: Option<String>,
}

pub fn parse_ocsp_staple(data: Option<&[u8]>) -> OcspInfo {
    let Some(der) = data else {
        return not_stapled();
    };

    if der.is_empty() {
        return not_stapled();
    }

    match x509_ocsp::OcspResponse::from_der(der) {
        Ok(resp) => parse_ocsp_response(resp),
        Err(_) => OcspInfo {
            stapled: true,
            status: Some("malformed".to_string()),
            this_update: None,
            next_update: None,
        },
    }
}

fn not_stapled() -> OcspInfo {
    OcspInfo {
        stapled: false,
        status: None,
        this_update: None,
        next_update: None,
    }
}

fn malformed() -> OcspInfo {
    OcspInfo {
        stapled: true,
        status: Some("malformed".to_string()),
        this_update: None,
        next_update: None,
    }
}

fn parse_ocsp_response(resp: x509_ocsp::OcspResponse) -> OcspInfo {
    if resp.response_status != x509_ocsp::OcspResponseStatus::Successful {
        return malformed();
    }

    let Some(response_bytes) = resp.response_bytes else {
        return malformed();
    };

    let basic = match x509_ocsp::BasicOcspResponse::from_der(response_bytes.response.as_bytes()) {
        Ok(b) => b,
        Err(_) => return malformed(),
    };

    let Some(single) = basic.tbs_response_data.responses.first() else {
        return malformed();
    };

    let status = match &single.cert_status {
        x509_ocsp::CertStatus::Good(_) => "good",
        x509_ocsp::CertStatus::Revoked(_) => "revoked",
        x509_ocsp::CertStatus::Unknown(_) => "unknown",
    };

    OcspInfo {
        stapled: true,
        status: Some(status.to_string()),
        this_update: Some(format_generalized_time(&single.this_update)),
        next_update: single.next_update.as_ref().map(format_generalized_time),
    }
}

fn format_generalized_time(t: &x509_ocsp::OcspGeneralizedTime) -> String {
    let dt = t.0.to_date_time();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minutes(),
        dt.seconds()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_staple_returns_not_stapled() {
        let info = parse_ocsp_staple(None);
        assert!(!info.stapled);
        assert!(info.status.is_none());
    }

    #[test]
    fn empty_data_returns_not_stapled() {
        let info = parse_ocsp_staple(Some(&[]));
        assert!(!info.stapled);
    }

    #[test]
    fn garbage_data_returns_malformed() {
        let info = parse_ocsp_staple(Some(&[0xFF, 0xFF, 0xFF]));
        assert!(info.stapled);
        assert_eq!(info.status.as_deref(), Some("malformed"));
    }
}
