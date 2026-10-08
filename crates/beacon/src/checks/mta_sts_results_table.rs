//! Pinning table for today's MTA-STS results.
//!
//! Each row drives `check_mta_sts` through a stub DNS resolver and a client
//! built from the production builder (`crate::state::http_client_builder`).
//! The client is given a resolve override that points
//! `mta-sts.example.com` at a local TLS listener and trusts the listener's
//! self-signed certificate; everything else (timeout, no-redirect policy,
//! user agent) is the production configuration.
//!
//! The expected sub-check names, verdicts and grades are derived from reading
//! `mta_sts.rs`, not from running it.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::checks::mta_sts::check_mta_sts;
use crate::dns::test_support::TestDnsResolver;
use crate::quality::{Grade, Verdict, compute_grade};

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
        // Row 1: the stub returns no addresses (models a beacon resolver error),
        // so the pre-flight loop over resolved addresses has nothing to reject.
        // The client's own resolve map still reaches the listener, which serves
        // a valid enforce policy: only `mode` Pass is recorded. Verdict Pass,
        // no Warn/Fail, grade A.
        Row {
            name: "empty stub answer, policy reachable",
            stub_ips: vec![],
            endpoint: Endpoint::ValidPolicy,
            expected: vec![("mode", Verdict::Pass)],
            grade: Grade::A,
        },
        // Row 2: 10.0.0.1 is RFC 1918, so `is_allowed_target` is false and
        // check_mta_sts returns early with a single `ssrf_blocked` Fail before
        // any fetch. Category verdict Fail; one Fail and no Warn gives grade D.
        Row {
            name: "private address in stub",
            stub_ips: vec!["10.0.0.1".parse().unwrap()],
            endpoint: Endpoint::ValidPolicy,
            expected: vec![("ssrf_blocked", Verdict::Fail)],
            grade: Grade::D,
        },
        // Row 3: 93.184.216.34 is publicly routable and not in any blocked
        // range, so the pre-flight passes and the fetch runs against the
        // listener, giving a valid enforce policy: `mode` Pass only, grade A.
        Row {
            name: "public address in stub, policy reachable",
            stub_ips: vec!["93.184.216.34".parse().unwrap()],
            endpoint: Endpoint::ValidPolicy,
            expected: vec![("mode", Verdict::Pass)],
            grade: Grade::A,
        },
        // Row 4: the endpoint answers 301. The client never follows redirects
        // and fetch_and_parse_policy checks `is_redirection()` first, so it
        // records `https_redirect` Fail and stops (no mode check). Grade D.
        Row {
            name: "policy endpoint answers 301",
            stub_ips: vec!["93.184.216.34".parse().unwrap()],
            endpoint: Endpoint::Redirect,
            expected: vec![("https_redirect", Verdict::Fail)],
            grade: Grade::D,
        },
    ];

    for row in rows {
        let (port, cert_der) = start_tls_listener(row.endpoint).await;

        // `resolve` keeps a non-zero port when the URL has no explicit port
        // (the policy URL has none), so this reaches the ephemeral listener.
        let client = crate::state::http_client_builder(5_000)
            .resolve(
                "mta-sts.example.com",
                SocketAddr::from(([127, 0, 0, 1], port)),
            )
            .add_root_certificate(reqwest::Certificate::from_der(&cert_der).unwrap())
            .build()
            .unwrap();

        let resolver = TestDnsResolver::new()
            .with_txt("_mta-sts.example.com", vec!["v=STSv1; id=20200101T000000;"])
            .with_ips("mta-sts.example.com", row.stub_ips.clone());

        let (result, _info) = check_mta_sts("example.com", &resolver, &client).await;

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
