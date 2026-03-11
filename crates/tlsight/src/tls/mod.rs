pub mod chain;
pub mod connect;
pub mod ocsp;
pub mod params;
pub mod verifier;

use std::net::IpAddr;
use std::time::Duration;

use serde::Serialize;
use utoipa::ToSchema;

pub use chain::CertInfo;
pub use params::TlsParams;

/// Per-IP error detail embedded in inspection results.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InspectionError {
    /// Machine-readable error code (e.g. `HANDSHAKE_FAILED`).
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

/// Result of inspecting a single IP on a single port.
#[derive(Debug, Serialize, ToSchema)]
pub struct IpInspectionResult {
    pub ip: String,
    pub ip_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<Vec<CertInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<crate::validate::ValidationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ct: Option<crate::validate::ct::CtInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<crate::enrichment::IpEnrichment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<InspectionError>,
    /// Raw DER-encoded certificates from the handshake, used for chain trust validation.
    /// Not serialized to JSON.
    #[serde(skip)]
    #[schema(ignore)]
    pub raw_certs: Option<Vec<rustls::pki_types::CertificateDer<'static>>>,
}

/// Perform TLS inspection on a single IP and port.
pub async fn inspect_ip(
    ip: IpAddr,
    port: u16,
    hostname: Option<&str>,
    timeout: Duration,
) -> IpInspectionResult {
    let ip_version = if ip.is_ipv4() { "v4" } else { "v6" };

    match connect::tls_handshake(ip, port, hostname, timeout).await {
        Ok(result) => {
            let chain = result
                .peer_certs
                .as_ref()
                .map(|certs| chain::parse_chain(certs));
            let tls_params = params::extract_params(&result, hostname);
            let raw_certs = result.peer_certs;

            IpInspectionResult {
                ip: ip.to_string(),
                ip_version: ip_version.to_string(),
                tls: Some(tls_params),
                chain,
                validation: None,
                ct: None,
                enrichment: None,
                raw_certs,
                error: None,
            }
        }
        Err(e) => IpInspectionResult {
            ip: ip.to_string(),
            ip_version: ip_version.to_string(),
            tls: None,
            chain: None,
            validation: None,
            ct: None,
            enrichment: None,
            raw_certs: None,
            error: Some(InspectionError {
                code: "HANDSHAKE_FAILED".to_string(),
                message: e.to_string(),
            }),
        },
    }
}
