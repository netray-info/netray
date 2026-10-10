//! Pinning table for today's MTA-STS results.
//!
//! Each row drives `check_mta_sts_at` (the policy URL carries the listener
//! port) through one stub DNS resolver that answers both the TXT lookup and
//! the fetch's host resolution (through `FetchResolver`). The fetch trusts the
//! listener's self-signed certificate and admits exactly 127.0.0.1, which
//! stands in for a public address; everything else (timeout, user agent) is
//! the production `OutboundFetch`.
//!
//! The expected sub-check names, verdicts and grades are derived from reading
//! `mta_sts.rs`, not from running it.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use netray_common::fetch::ClientSettings;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::checks::mta_sts::check_mta_sts_at;
use crate::dns::FetchResolver;
use crate::dns::test_support::TestDnsResolver;
use crate::quality::{Grade, Verdict, compute_grade};
use crate::state::OutboundFetch;

const POLICY_BODY: &str =
    "version: STSv1\nmode: enforce\nmx: mail.example.com\nmax_age: 604800\nid: 20200101T000000\n";

#[derive(Clone, Copy)]
enum Endpoint {
    /// 200, `text/plain`, a valid enforce-mode policy.
    ValidPolicy,
    /// 301 pointing at another HTTPS location.
    Redirect,
}

impl Endpoint {
    fn response(self) -> String {
        match self {
            Endpoint::ValidPolicy => format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                POLICY_BODY.len(),
                POLICY_BODY
            ),
            Endpoint::Redirect => "HTTP/1.1 301 Moved Permanently\r\nlocation: https://mta-sts.example.com/elsewhere\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string(),
        }
    }
}

/// Start a loopback TLS listener (HTTP/1.1, one response per connection) whose
/// certificate is valid for `mta-sts.example.com`. Returns the port and the
/// DER certificate the client must trust.
async fn start_tls_listener(endpoint: Endpoint) -> (u16, Vec<u8>) {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["mta-sts.example.com".to_string()]).unwrap();
    let cert_der = cert.der().to_vec();
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(signing_key.serialize_der()).unwrap();

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let server_config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(cert_der.clone())],
            key_der,
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let response = endpoint.response();

    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let response = response.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
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

    (port, cert_der)
}

struct Row {
    name: &'static str,
    /// What the stub resolver returns for `mta-sts.example.com`.
    stub_ips: Vec<IpAddr>,
    endpoint: Endpoint,
    expected: Vec<(&'static str, Verdict)>,
    grade: Grade,
}

#[tokio::test]
async fn mta_sts_results_table() {
    let rows = vec![
        // Row 1: refused target (requirement 8). The stub returns no addresses
        // (models a beacon resolver error); the fetch resolves through the same
        // stub and refuses a host without addresses: `https_fetch_failed` Fail,
        // grade D.
        Row {
            name: "empty stub answer, policy reachable",
            stub_ips: vec![],
            endpoint: Endpoint::ValidPolicy,
            expected: vec![("https_fetch_failed", Verdict::Fail)],
            grade: Grade::D,
        },
        // Row 2: 10.0.0.1 is RFC 1918, so the fetch's `allow` is false and
        // check_mta_sts returns early with a single `ssrf_blocked` Fail before
        // any fetch. Category verdict Fail; one Fail and no Warn gives grade D.
        Row {
            name: "private address in stub",
            stub_ips: vec!["10.0.0.1".parse().unwrap()],
            endpoint: Endpoint::ValidPolicy,
            expected: vec![("ssrf_blocked", Verdict::Fail)],
            grade: Grade::D,
        },
        // Row 3: 127.0.0.1 stands in for a public address (the only one the
        // fetch admits), so the pre-flight passes and the fetch runs against
        // the listener, giving a valid enforce policy: `mode` Pass only, grade A.
        Row {
            name: "public address in stub, policy reachable",
            stub_ips: vec!["127.0.0.1".parse().unwrap()],
            endpoint: Endpoint::ValidPolicy,
            expected: vec![("mode", Verdict::Pass)],
            grade: Grade::A,
        },
        // Row 4: the endpoint answers 301. The fetch follows no redirect and
        // the check tests `is_redirection()` first, so it records
        // `https_redirect` Fail and stops (no mode check). Grade D.
        Row {
            name: "policy endpoint answers 301",
            stub_ips: vec!["127.0.0.1".parse().unwrap()],
            endpoint: Endpoint::Redirect,
            expected: vec![("https_redirect", Verdict::Fail)],
            grade: Grade::D,
        },
    ];

    for row in rows {
        let (port, cert_der) = start_tls_listener(row.endpoint).await;

        let resolver = Arc::new(
            TestDnsResolver::new()
                .with_txt("_mta-sts.example.com", vec!["v=STSv1; id=20200101T000000;"])
                .with_ips("mta-sts.example.com", row.stub_ips.clone()),
        );

        let base = OutboundFetch::new(5_000, Arc::new(FetchResolver(resolver.clone())));
        let fetch = OutboundFetch {
            settings: ClientSettings {
                root_certificates: vec![reqwest::Certificate::from_der(&cert_der).unwrap()],
                ..base.settings.clone()
            },
            allow: |ip| ip == IpAddr::V4(Ipv4Addr::LOCALHOST),
            ..base
        };

        // The policy URL carries the listener port; the production URL has none.
        let policy_url = format!("https://mta-sts.example.com:{port}/.well-known/mta-sts.txt");
        let (result, _info) =
            check_mta_sts_at("example.com", &*resolver, &fetch, &policy_url).await;

        let got: Vec<(&str, Verdict)> = result
            .sub_checks
            .iter()
            .map(|s| (s.name.as_str(), s.verdict))
            .collect();
        assert_eq!(got, row.expected, "row '{}': sub-checks", row.name);
        assert_eq!(
            compute_grade(&[result.verdict]),
            row.grade,
            "row '{}': grade",
            row.name
        );
    }
}
