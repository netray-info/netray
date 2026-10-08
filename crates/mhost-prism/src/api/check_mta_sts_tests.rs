//! prism's MTA-STS policy fetch through the outbound fetch helper (requirement 9 of
//! `specs/features/outbound-fetch/spec.md`).
//!
//! The tests call `check_mta_sts_at`, the seam that takes the policy URL, so a listener can
//! stand on a local port. Every TLS listener counts accepted TCP connections before the
//! handshake, so a refused target is proven by a count of 0.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mhost::lints::CheckResult;
use mhost::resolver::Lookups;
use netray_common::fetch::{ClientSettings, Resolve};
use netray_common::target_policy::is_allowed_target;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::{Outbound, check_mta_sts_at};

const DOMAIN: &str = "example.com";
const POLICY_HOST: &str = "mta-sts.example.com";
const POLICY_BODY: &str =
    "version: STSv1\nmode: enforce\nmx: mail.example.com\nmax_age: 604800\n";

/// `_mta-sts.example.com` with a valid TXT record, built through serde like `tests::make_txt_lookups`.
fn sts_lookups() -> Lookups {
    let name = format!("_mta-sts.{DOMAIN}");
    let txt: Vec<Vec<u8>> = vec![b"v=STSv1; id=20240101000000".to_vec()];
    serde_json::from_value(serde_json::json!({
        "lookups": [{
            "query": { "name": name, "record_type": "TXT" },
            "name_server": "udp:127.0.0.1:53",
            "result": {
                "Response": {
                    "records": [{
                        "name": name,
                        "type": "TXT",
                        "ttl": 300,
                        "data": { "TXT": { "txt_data": txt } }
                    }],
                    "response_time": { "secs": 0, "nanos": 10000000 },
                    "valid_until": "2099-01-01T00:00:00Z"
                }
            }
        }]
    }))
    .expect("test Lookups")
}

struct Stub(HashMap<String, Vec<IpAddr>>);

impl Resolve for Stub {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let ips = self.0.get(host).cloned().unwrap_or_default();
        Box::pin(async move { ips })
    }
}

fn stub(host: &str, ip: IpAddr) -> Arc<dyn Resolve> {
    Arc::new(Stub(HashMap::from([(host.to_string(), vec![ip])])))
}

fn only_loopback_v4(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST)
}

struct Listener {
    port: u16,
    cert_der: Vec<u8>,
    connections: Arc<AtomicUsize>,
}

impl Listener {
    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    fn root(&self) -> reqwest::Certificate {
        reqwest::Certificate::from_der(&self.cert_der).unwrap()
    }
}

/// A TLS listener on `ip` with a self-signed certificate for `names`; answers every request
/// with `response` and counts accepted connections.
async fn listen(ip: IpAddr, names: &[&str], response: String) -> Listener {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(names.iter().map(|n| n.to_string()).collect::<Vec<_>>())
            .unwrap();
    let cert_der = cert.der().to_vec();
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(signing_key.serialize_der()).unwrap();
    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![rustls::pki_types::CertificateDer::from(cert_der.clone())],
        key_der,
    )
    .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));

    let listener = TcpListener::bind((ip, 0))
        .await
        .unwrap_or_else(|e| panic!("bind {ip}: {e}"));
    let port = listener.local_addr().unwrap().port();
    let connections = Arc::new(AtomicUsize::new(0));
    let counter = connections.clone();

    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            counter.fetch_add(1, Ordering::SeqCst);
            let acceptor = acceptor.clone();
            let response = response.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
                    match tls.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let _ = tls.write_all(response.as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });

    Listener {
        port,
        cert_der,
        connections,
    }
}

fn policy_response() -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        POLICY_BODY.len(),
        POLICY_BODY
    )
}

fn redirect_response(location: &str) -> String {
    format!(
        "HTTP/1.1 302 Found\r\nlocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
    )
}

fn unreachable() -> Vec<CheckResult> {
    vec![
        CheckResult::Ok("MTA-STS DNS record valid".into()),
        CheckResult::Warning("MTA-STS policy file unreachable".into()),
    ]
}

/// (a) The policy host resolves to 10.0.0.1, which the production allow refuses.
#[tokio::test]
async fn mta_sts_policy_host_on_private_address_is_unreachable() {
    let outbound = Outbound {
        settings: ClientSettings::default(),
        resolver: stub(POLICY_HOST, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
        allow: is_allowed_target,
    };
    let url = format!("https://{POLICY_HOST}/.well-known/mta-sts.txt");

    let results = check_mta_sts_at(&sts_lookups(), DOMAIN, &Lookups::empty(), &outbound, &url).await;

    assert_eq!(results, unreachable());
}

/// (b) An allowed policy host redirects to the loopback literal `[::1]`: refused, 0
/// connections there.
#[tokio::test]
async fn mta_sts_policy_redirect_to_loopback_is_unreachable() {
    let target = listen(IpAddr::V6(Ipv6Addr::LOCALHOST), &["::1"], policy_response()).await;
    let location = format!("https://[::1]:{}/.well-known/mta-sts.txt", target.port);
    let policy = listen(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        &[POLICY_HOST],
        redirect_response(&location),
    )
    .await;
    let outbound = Outbound {
        settings: ClientSettings {
            root_certificates: vec![policy.root(), target.root()],
            ..ClientSettings::default()
        },
        resolver: stub(POLICY_HOST, IpAddr::V4(Ipv4Addr::LOCALHOST)),
        allow: only_loopback_v4,
    };
    let url = format!("https://{POLICY_HOST}:{}/.well-known/mta-sts.txt", policy.port);

    let results = check_mta_sts_at(&sts_lookups(), DOMAIN, &Lookups::empty(), &outbound, &url).await;

    assert_eq!(results, unreachable());
    assert_eq!(policy.connections(), 1, "the policy host was fetched");
    assert_eq!(target.connections(), 0, "[::1] was reached");
}

/// (c) A valid policy on an allowed listener keeps today's result.
#[tokio::test]
async fn mta_sts_valid_policy_on_allowed_host_is_valid() {
    let policy = listen(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        &[POLICY_HOST],
        policy_response(),
    )
    .await;
    let outbound = Outbound {
        settings: ClientSettings {
            root_certificates: vec![policy.root()],
            ..ClientSettings::default()
        },
        resolver: stub(POLICY_HOST, IpAddr::V4(Ipv4Addr::LOCALHOST)),
        allow: only_loopback_v4,
    };
    let url = format!("https://{POLICY_HOST}:{}/.well-known/mta-sts.txt", policy.port);

    let results = check_mta_sts_at(&sts_lookups(), DOMAIN, &Lookups::empty(), &outbound, &url).await;

    assert_eq!(
        results,
        vec![
            CheckResult::Ok("MTA-STS DNS record valid".into()),
            CheckResult::Ok("MTA-STS policy valid (mode: enforce, max_age: 604800s)".into()),
        ]
    );
    assert_eq!(policy.connections(), 1);
}
