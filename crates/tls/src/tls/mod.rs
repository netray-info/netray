pub mod chain;
pub mod connect;
pub mod ocsp;
pub mod params;
pub mod probe;
pub mod verifier;

use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub use chain::CertInfo;
pub use params::TlsParams;

/// Per-IP error detail embedded in inspection results.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InspectionError {
    /// Machine-readable error code: `HANDSHAKE_FAILED`, `HANDSHAKE_TIMEOUT` or
    /// `NOT_TESTED_FROM_HERE`.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

/// Result of inspecting a single IP on a single port.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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
    pub enrichment: Option<netray_common::enrichment::IpInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<InspectionError>,
    /// Raw DER-encoded certificates from the handshake, used for chain trust validation.
    /// Not serialized to JSON.
    #[serde(skip)]
    #[schema(ignore)]
    pub raw_certs: Option<Vec<rustls::pki_types::CertificateDer<'static>>>,
    /// Protocol-version and suite-group probes of the address. Not serialized to JSON.
    #[serde(skip)]
    #[schema(ignore)]
    pub probes: Option<probe::ProtocolProbes>,
}

/// Map a `connect::tls_handshake` error to the code `inspect_ip` reports.
///
/// An error that says this host cannot reach the target at all (no route to the network, no
/// usable source address) is not a verdict on the target: `NOT_TESTED_FROM_HERE`. A handshake
/// that did not finish within the timeout is `HANDSHAKE_TIMEOUT`. A host unreachable
/// (`EHOSTUNREACH`) is target-side: a remote ICMP reply or a firewall's admin-prohibited
/// reject, so it stays `HANDSHAKE_FAILED`, as do a refusal, a reset and a TLS alert.
pub fn error_code(err: &(dyn std::error::Error + Send + Sync + 'static)) -> &'static str {
    if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
        match io_err.kind() {
            std::io::ErrorKind::NetworkUnreachable | std::io::ErrorKind::AddrNotAvailable => {
                return "NOT_TESTED_FROM_HERE";
            }
            std::io::ErrorKind::TimedOut => return "HANDSHAKE_TIMEOUT",
            _ => {}
        }
    }
    "HANDSHAKE_FAILED"
}

/// Maps an `error_code` value onto the shared model status: a target-side failure is `Fail`,
/// an untestable address `NotTested`, a timeout or an unknown code `Unmeasured`.
pub fn status_of_error_code(code: &str) -> netray_model::Status {
    match code {
        "NOT_TESTED_FROM_HERE" => netray_model::Status::NotTested,
        "HANDSHAKE_FAILED" => netray_model::Status::Fail,
        _ => netray_model::Status::Unmeasured,
    }
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
                probes: None,
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
            probes: None,
            error: Some(InspectionError {
                code: error_code(&*e).to_string(),
                message: e.to_string(),
            }),
        },
    }
}
