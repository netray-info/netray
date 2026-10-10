use der::{Any, Decode, Encode, asn1::OctetString};
use netray_common::fetch::{
    AtLimit, ClientSettings, FetchError, FetchOptions, Resolve, SystemResolver, fetch,
};
use netray_common::target_policy::is_allowed_target;
use reqwest::header::{CONTENT_TYPE, HeaderValue};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use spki::AlgorithmIdentifierOwned;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;
use x509_cert::serial_number::SerialNumber;
use x509_ocsp::{CertId, OcspRequest, Request, TbsRequest};

use chrono::Utc;

/// Result of a live OCSP revocation check via the AIA OCSP responder URL.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OcspRevocationResult {
    /// "good", "revoked", or "unknown"
    pub status: String,
    /// Revocation reason string when status == "revoked"; "blocked" when the responder URL
    /// was refused
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
    check_live_ocsp_with(
        Arc::new(SystemResolver),
        is_allowed_target,
        ocsp_url,
        leaf_der,
        issuer_der,
    )
    .await
}

/// [`check_live_ocsp`] with the resolver and the address predicate of the fetch chosen by the
/// caller. A refused target yields status "unknown" with reason "blocked".
pub async fn check_live_ocsp_with(
    resolver: Arc<dyn Resolve>,
    allow: fn(IpAddr) -> bool,
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

    let mut opts = FetchOptions::new(Method::POST);
    opts.body = Some(req_bytes.into());
    opts.headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/ocsp-request"),
    );
    opts.max_redirects = 10;
    opts.at_limit = AtLimit::Fail;
    opts.timeout = Duration::from_secs(3);
    opts.allow = allow;

    let response = match fetch(&ClientSettings::default(), resolver, ocsp_url, &opts).await {
        Ok(r) => r,
        Err(FetchError::Blocked { .. }) => {
            return OcspRevocationResult {
                reason: Some("blocked".to_string()),
                ..unknown()
            };
        }
        Err(_) => return unknown(),
    };

    if response.status != StatusCode::OK {
        return unknown();
    }

    match x509_ocsp::OcspResponse::from_der(&response.body) {
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

#[cfg(test)]
mod live_fetch_tests {
    use super::*;
    use netray_common::fetch::Resolve;
    use netray_common::target_policy::is_allowed_target;
    use std::future::Future;
    use std::net::{IpAddr, Ipv4Addr};
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct StubResolver(IpAddr);

    impl Resolve for StubResolver {
        fn resolve(&self, _host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
            let ip = self.0;
            Box::pin(async move { vec![ip] })
        }
    }

    fn loopback_only(ip: IpAddr) -> bool {
        ip == IpAddr::V4(Ipv4Addr::LOCALHOST)
    }

    #[derive(Debug, Clone)]
    struct Seen {
        method: String,
        content_type: Option<String>,
        body_len: usize,
    }

    struct Listener {
        port: u16,
        count: Arc<AtomicUsize>,
        seen: Arc<Mutex<Vec<Seen>>>,
    }

    /// Plain-HTTP loopback listener; answers every request with `reply`.
    async fn listen(reply: String) -> Listener {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (c, s) = (count.clone(), seen.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = l.accept().await else {
                    return;
                };
                c.fetch_add(1, Ordering::SeqCst);
                let (s, reply) = (s.clone(), reply.clone());
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 1024];
                    let head_end = loop {
                        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            break i + 4;
                        }
                        match sock.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    };
                    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                    let header = |name: &str| {
                        head.lines().find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.eq_ignore_ascii_case(name).then(|| v.trim().to_string())
                        })
                    };
                    let len = header("content-length")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    while buf.len() < head_end + len {
                        match sock.read(&mut chunk).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    s.lock().unwrap().push(Seen {
                        method: head.split(' ').next().unwrap_or("").to_string(),
                        content_type: header("content-type"),
                        body_len: buf.len() - head_end,
                    });
                    let _ = sock.write_all(reply.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        Listener { port, count, seen }
    }

    fn http(status: u16, location: Option<&str>) -> String {
        let loc = location
            .map(|l| format!("Location: {l}\r\n"))
            .unwrap_or_default();
        format!("HTTP/1.1 {status} X\r\n{loc}Content-Length: 0\r\nConnection: close\r\n\r\n")
    }

    /// A real (leaf, issuer) DER pair, so `build_ocsp_request` succeeds.
    fn cert_pair() -> (Vec<u8>, Vec<u8>) {
        use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
        let ca_key = KeyPair::generate().unwrap();
        let mut ca_params = CertificateParams::new(vec![]).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca = ca_params.self_signed(&ca_key).unwrap();
        let issuer = Issuer::from_params(&ca_params, &ca_key);
        let leaf_key = KeyPair::generate().unwrap();
        let leaf = CertificateParams::new(vec!["leaf.example.com".to_string()])
            .unwrap()
            .signed_by(&leaf_key, &issuer)
            .unwrap();
        (leaf.der().to_vec(), ca.der().to_vec())
    }

    fn assert_blocked(r: &OcspRevocationResult) {
        assert_eq!(r.status, "unknown");
        assert_eq!(r.reason.as_deref(), Some("blocked"));
    }

    #[tokio::test]
    async fn live_ocsp_refuses_loopback_target_without_connecting() {
        let (leaf, issuer) = cert_pair();
        let resolver: Arc<dyn Resolve> = Arc::new(StubResolver(IpAddr::V4(Ipv4Addr::LOCALHOST)));

        // C12: literal loopback URL.
        let srv = listen(http(200, None)).await;
        let url = format!("http://127.0.0.1:{}/", srv.port);
        let r =
            check_live_ocsp_with(resolver.clone(), is_allowed_target, &url, &leaf, &issuer).await;
        assert_blocked(&r);
        assert_eq!(srv.count.load(Ordering::SeqCst), 0, "literal loopback");

        // C12: name that resolves to loopback.
        let srv = listen(http(200, None)).await;
        let url = format!("http://ocsp.invalid:{}/", srv.port);
        let r = check_live_ocsp_with(resolver, is_allowed_target, &url, &leaf, &issuer).await;
        assert_blocked(&r);
        assert_eq!(srv.count.load(Ordering::SeqCst), 0, "name to loopback");
    }

    #[tokio::test]
    async fn live_ocsp_redirect_method_and_body_follow_status() {
        let (leaf, issuer) = cert_pair();
        let req_len = build_ocsp_request(&leaf, &issuer).unwrap().len();
        let resolver: Arc<dyn Resolve> = Arc::new(StubResolver(IpAddr::V4(Ipv4Addr::LOCALHOST)));

        // C13: 301 downgrades to GET without body or OCSP content-type;
        // 307 preserves POST, body and content-type.
        for (status, method, body_len, ocsp_ct) in
            [(301u16, "GET", 0usize, false), (307, "POST", req_len, true)]
        {
            let second = listen(http(200, None)).await;
            let first = listen(http(
                status,
                Some(&format!("http://ocsp2.test:{}/", second.port)),
            ))
            .await;
            let url = format!("http://ocsp.test:{}/", first.port);
            let _ =
                check_live_ocsp_with(resolver.clone(), loopback_only, &url, &leaf, &issuer).await;

            let seen = second.seen.lock().unwrap().clone();
            assert_eq!(seen.len(), 1, "{status}: second listener hit once");
            assert_eq!(seen[0].method, method, "{status}: method");
            assert_eq!(seen[0].body_len, body_len, "{status}: body length");
            assert_eq!(
                seen[0].content_type.as_deref() == Some("application/ocsp-request"),
                ocsp_ct,
                "{status}: content-type {:?}",
                seen[0].content_type
            );
        }
    }
}
