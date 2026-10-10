use serde::Serialize;
use utoipa::ToSchema;

use super::connect::HandshakeResult;
use super::ocsp;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TlsParams {
    pub version: String,
    pub cipher_suite: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sni: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_exchange_group: Option<String>,
    pub ocsp: ocsp::OcspInfo,
    /// Live OCSP revocation check result (queried after handshake if AIA OCSP URL is present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocsp_live: Option<ocsp::OcspRevocationResult>,
    pub handshake_ms: u32,
    /// STARTTLS protocol used to upgrade the connection (e.g. "smtp"), if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starttls: Option<String>,
    /// Whether ECH (Encrypted Client Hello) is advertised via the HTTPS DNS record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ech_advertised: Option<bool>,
}

pub fn extract_params(result: &HandshakeResult, sni: Option<&str>) -> TlsParams {
    let version = result
        .version
        .map(format_version)
        .unwrap_or_else(|| "unknown".to_string());

    let cipher_suite = result
        .cipher_suite
        .map(|cs| format!("{:?}", cs.suite()))
        .unwrap_or_else(|| "unknown".to_string());

    let alpn = result
        .alpn
        .as_ref()
        .and_then(|p| String::from_utf8(p.clone()).ok());

    let ocsp_info = ocsp::parse_ocsp_staple(result.ocsp_response.as_deref());

    TlsParams {
        version,
        cipher_suite,
        alpn,
        sni: sni.map(|s| s.to_string()),
        key_exchange_group: result.key_exchange_group.clone(),
        ocsp: ocsp_info,
        ocsp_live: None,
        handshake_ms: result.handshake_ms,
        starttls: None,
        ech_advertised: None,
    }
}

fn format_version(v: rustls::ProtocolVersion) -> String {
    match v {
        rustls::ProtocolVersion::TLSv1_2 => "TLSv1.2".to_string(),
        rustls::ProtocolVersion::TLSv1_3 => "TLSv1.3".to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_version_tls13() {
        assert_eq!(format_version(rustls::ProtocolVersion::TLSv1_3), "TLSv1.3");
    }

    #[test]
    fn format_version_tls12() {
        assert_eq!(format_version(rustls::ProtocolVersion::TLSv1_2), "TLSv1.2");
    }

    #[test]
    fn format_version_unknown() {
        // ProtocolVersion is a newtype around u16
        let v = rustls::ProtocolVersion::Unknown(0x0300);
        let formatted = format_version(v);
        assert!(!formatted.is_empty());
    }
}
