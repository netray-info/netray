use der::Decode;
use serde::Serialize;
use utoipa::ToSchema;

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
        // TODO("extract GeneralizedTime from single.this_update / single.next_update")
        this_update: None,
        next_update: None,
    }
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
