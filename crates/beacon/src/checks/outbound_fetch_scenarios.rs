//! Scenarios for beacon's MTA-STS and BIMI fetches through `OutboundFetch`.
//!
//! Every listener counts accepted TCP connections before the TLS handshake, so
//! a refused target is proven by a count of 0, not by the sub-check alone.
//!
//! An MTA-STS endpoint answering 301 (`https_redirect` Fail) is row 4 of
//! `mta_sts_results_table`.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use netray_common::fetch::ClientSettings;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::checks::bimi::check_bimi;
use crate::checks::mta_sts::check_mta_sts_at;
use crate::dns::FetchResolver;
use crate::dns::test_support::TestDnsResolver;
use crate::quality::Verdict;
use crate::state::OutboundFetch;

const POLICY_BODY: &str =
    "version: STSv1\nmode: enforce\nmx: mail.example.com\nmax_age: 604800\nid: 20200101T000000\n";

struct Listener {
    port: u16,
    cert_der: Vec<u8>,
    connections: Arc<AtomicUsize>,
}

impl Listener {
    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

/// A loopback TLS listener on `ip` with a self-signed certificate for `names`.
/// It answers every request with `response` and counts accepted connections.
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

fn empty_response(status: &str, location: Option<&str>) -> String {
    let mut r = format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n");
    if let Some(l) = location {
        r.push_str(&format!("location: {l}\r\n"));
    }
    r.push_str("\r\n");
    r
}

fn only_loopback_v4(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn sub_checks(result: &crate::quality::CheckResult) -> Vec<(String, Verdict, String)> {
    result
        .sub_checks
        .iter()
        .map(|s| (s.name.clone(), s.verdict, s.detail.clone()))
        .collect()
}

/// C6: a policy host the resolver answers with no address is not fetched.
#[tokio::test]
async fn mta_sts_policy_host_without_addresses_is_not_fetched() {
    let listener = listen(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        &["mta-sts.empty.invalid"],
        policy_response(),
    )
    .await;

    let resolver = Arc::new(
        TestDnsResolver::new()
            .with_txt(
                "_mta-sts.empty.invalid",
                vec!["v=STSv1; id=20200101T000000;"],
            )
            .with_ips("mta-sts.empty.invalid", vec![]),
    );
    let base = OutboundFetch::new(5_000, Arc::new(FetchResolver(resolver.clone())));
    let fetch = OutboundFetch {
        settings: ClientSettings {
            root_certificates: vec![reqwest::Certificate::from_der(&listener.cert_der).unwrap()],
            ..base.settings.clone()
        },
        allow: only_loopback_v4,
        ..base
    };

    let url = format!(
        "https://mta-sts.empty.invalid:{}/.well-known/mta-sts.txt",
        listener.port
    );
    let (result, _info) = check_mta_sts_at("empty.invalid", &*resolver, &fetch, &url).await;

    assert_eq!(
        sub_checks(&result),
        vec![(
            "https_fetch_failed".to_string(),
            Verdict::Fail,
            "policy host not reachable".to_string()
        )]
    );
    assert_eq!(listener.connections(), 0, "connections at the listener");
}

/// C7: the policy fetch connects to the address the stub resolver returns.
/// `.invalid` cannot resolve through the system, so reaching the listener
/// proves the connection is pinned to the stub's answer.
#[tokio::test]
async fn mta_sts_policy_fetch_connects_to_the_resolved_address() {
    let listener = listen(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        &["mta-sts.pinned.invalid"],
        policy_response(),
    )
    .await;

    let resolver = Arc::new(
        TestDnsResolver::new()
            .with_txt(
                "_mta-sts.pinned.invalid",
                vec!["v=STSv1; id=20200101T000000;"],
            )
            .with_ips(
                "mta-sts.pinned.invalid",
                vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            ),
    );
    let base = OutboundFetch::new(5_000, Arc::new(FetchResolver(resolver.clone())));
    let fetch = OutboundFetch {
        settings: ClientSettings {
            root_certificates: vec![reqwest::Certificate::from_der(&listener.cert_der).unwrap()],
            ..base.settings.clone()
        },
        allow: only_loopback_v4,
        ..base
    };

    let url = format!(
        "https://mta-sts.pinned.invalid:{}/.well-known/mta-sts.txt",
        listener.port
    );
    let (result, _info) = check_mta_sts_at("pinned.invalid", &*resolver, &fetch, &url).await;

    let got: Vec<(String, Verdict)> = sub_checks(&result)
        .into_iter()
        .map(|(n, v, _)| (n, v))
        .collect();
    assert_eq!(got, vec![("mode".to_string(), Verdict::Pass)]);
    assert_eq!(listener.connections(), 1, "connections at the listener");
}

/// C8: logo URLs whose target the production `allow` refuses give the generic
/// unreachable Warn, never a status or an error text, and no connection.
#[tokio::test]
async fn bimi_refused_logo_targets_warn_without_connecting() {
    let v4 = listen(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        &["localhost"],
        empty_response("200 OK", None),
    )
    .await;
    let v6 = listen(
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        &["localhost"],
        empty_response("200 OK", None),
    )
    .await;

    let cases = [
        format!("https://127.0.0.1:{}/l.svg", v4.port),
        format!("https://x@127.0.0.1:{}/l.svg", v4.port),
        format!("https://[::1]:{}/l.svg", v6.port),
        "https://internal.invalid/l.svg".to_string(),
    ];

    let mut failures = Vec::new();
    for logo_url in &cases {
        let record = format!("v=BIMI1; l={logo_url}");
        let resolver = Arc::new(
            TestDnsResolver::new()
                .with_txt("default._bimi.example.com", vec![record.as_str()])
                .with_ips("internal.invalid", vec![]),
        );
        let base = OutboundFetch::new(5_000, Arc::new(FetchResolver(resolver.clone())));
        // Production `allow`; only the certificate check is relaxed.
        let fetch = OutboundFetch {
            settings: ClientSettings {
                accept_invalid_certs: true,
                ..base.settings.clone()
            },
            ..base
        };

        let (result, _present) = check_bimi("example.com", &*resolver, &fetch).await;
        let got = sub_checks(&result);
        let want = vec![(
            "logo_unreachable".to_string(),
            Verdict::Warn,
            "logo host not reachable".to_string(),
        )];
        if got != want {
            failures.push(format!("{logo_url}: sub_checks={got:?}; want {want:?}"));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(v4.connections(), 0, "connections at 127.0.0.1");
    assert_eq!(v6.connections(), 0, "connections at [::1]");
}

/// C9: an admitted logo host redirecting to a refused literal fails at the
/// redirect, before the redirect target is contacted.
#[tokio::test]
async fn bimi_redirect_to_refused_literal_fails_before_connecting() {
    let target = listen(
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        &["localhost"],
        empty_response("200 OK", None),
    )
    .await;
    let location = format!("https://[::1]:{}/l.svg", target.port);
    let logo = listen(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        &["logo.example.com"],
        empty_response("302 Found", Some(&location)),
    )
    .await;

    let record = format!("v=BIMI1; l=https://logo.example.com:{}/l.svg", logo.port);
    let resolver = Arc::new(
        TestDnsResolver::new()
            .with_txt("default._bimi.example.com", vec![record.as_str()])
            .with_ips("logo.example.com", vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]),
    );
    let base = OutboundFetch::new(5_000, Arc::new(FetchResolver(resolver.clone())));
    let fetch = OutboundFetch {
        settings: ClientSettings {
            accept_invalid_certs: true,
            ..base.settings.clone()
        },
        allow: only_loopback_v4,
        ..base
    };

    let (result, _present) = check_bimi("example.com", &*resolver, &fetch).await;

    let got: Vec<(String, Verdict)> = sub_checks(&result)
        .into_iter()
        .map(|(n, v, _)| (n, v))
        .collect();
    assert_eq!(
        got,
        vec![("logo_redirect_ssrf_blocked".to_string(), Verdict::Fail)]
    );
    assert_eq!(logo.connections(), 1, "connections at the logo host");
    assert_eq!(target.connections(), 0, "connections at [::1]");
}
