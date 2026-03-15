use der::{Any, Decode, Encode, asn1::OctetString};
use serde::Serialize;
use sha1::{Digest, Sha1};
use spki::AlgorithmIdentifierOwned;
use std::time::Duration;
use utoipa::ToSchema;
use x509_cert::serial_number::SerialNumber;
use x509_ocsp::{CertId, OcspRequest, Request, TbsRequest};

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
/// On network error or timeout, returns status "unknown".
pub async fn check_live_ocsp(
    ocsp_url: &str,
    leaf_der: &[u8],
    issuer_der: &[u8],
) -> OcspRevocationResult {
    let checked_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let unknown = || OcspRevocationResult {
        status: "unknown".to_string(),
        reason: None,
        revoked_at: None,
        checked_at: checked_at.clone(),
    };

    let req_bytes = match build_ocsp_request(leaf_der, issuer_der) {
        Ok(b) => b,
        Err(_) => return unknown(),
    };

    let response = match reqwest::Client::new()
        .post(ocsp_url)
        .header("Content-Type", "application/ocsp-request")
        .body(req_bytes)
        .timeout(Duration::from_secs(3))
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return unknown(),
    };

    if response.status() != reqwest::StatusCode::OK {
        return unknown();
    }

    let body = match response.bytes().await {
        Ok(b) => b,
        Err(_) => return unknown(),
    };

    match x509_ocsp::OcspResponse::from_der(&body) {
        Ok(resp) => parse_live_ocsp_response(resp, checked_at),
        Err(_) => unknown(),
    }
}

fn build_ocsp_request(leaf_der: &[u8], issuer_der: &[u8]) -> Result<Vec<u8>, ()> {
    let (_, leaf) = x509_parser::parse_x509_certificate(leaf_der).map_err(|_| ())?;
    let (_, issuer) = x509_parser::parse_x509_certificate(issuer_der).map_err(|_| ())?;

    let issuer_name_hash: [u8; 20] = Sha1::digest(issuer.tbs_certificate.subject.as_raw()).into();
    let issuer_key_hash: [u8; 20] =
        Sha1::digest(issuer.tbs_certificate.subject_pki.subject_public_key.data).into();

    // SHA-1 OID: 1.3.14.3.2.26
    let sha1_oid = der::asn1::ObjectIdentifier::new_unwrap("1.3.14.3.2.26");
    let hash_algorithm = AlgorithmIdentifierOwned {
        oid: sha1_oid,
        parameters: Some(Any::from(der::asn1::Null)),
    };

    let serial_bytes = leaf.tbs_certificate.raw_serial();
    let serial_number = SerialNumber::new(serial_bytes).map_err(|_| ())?;

    let cert_id = CertId {
        hash_algorithm,
        issuer_name_hash: OctetString::new(issuer_name_hash.to_vec()).map_err(|_| ())?,
        issuer_key_hash: OctetString::new(issuer_key_hash.to_vec()).map_err(|_| ())?,
        serial_number,
    };

    let ocsp_req = OcspRequest {
        tbs_request: TbsRequest {
            request_list: vec![Request {
                req_cert: cert_id,
                single_request_extensions: None,
            }],
            ..Default::default()
        },
        optional_signature: None,
    };

    ocsp_req.to_der().map_err(|_| ())
}

fn parse_live_ocsp_response(
    resp: x509_ocsp::OcspResponse,
    checked_at: String,
) -> OcspRevocationResult {
    let unknown = || OcspRevocationResult {
        status: "unknown".to_string(),
        reason: None,
        revoked_at: None,
        checked_at: checked_at.clone(),
    };

    if resp.response_status != x509_ocsp::OcspResponseStatus::Successful {
        return unknown();
    }

    let Some(response_bytes) = resp.response_bytes else {
        return unknown();
    };

    let basic = match x509_ocsp::BasicOcspResponse::from_der(response_bytes.response.as_bytes()) {
        Ok(b) => b,
        Err(_) => return unknown(),
    };

    let Some(single) = basic.tbs_response_data.responses.first() else {
        return unknown();
    };

    match &single.cert_status {
        x509_ocsp::CertStatus::Good(_) => OcspRevocationResult {
            status: "good".to_string(),
            reason: None,
            revoked_at: None,
            checked_at,
        },
        x509_ocsp::CertStatus::Revoked(info) => {
            let reason = info.revocation_reason.map(|r| format!("{:?}", r));
            let revoked_at = Some(format_generalized_time(&info.revocation_time));
            OcspRevocationResult {
                status: "revoked".to_string(),
                reason,
                revoked_at,
                checked_at,
            }
        }
        x509_ocsp::CertStatus::Unknown(_) => unknown(),
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
