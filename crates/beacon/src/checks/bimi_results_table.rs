//! Pinning table for `check_bimi` results.
//!
//! Each row builds a stub resolver (what the beacon resolver sees) and a
//! `reqwest` client (what the transport reaches) that deliberately disagree
//! in places, then asserts the literal sub-check names and verdicts plus the
//! BIMI-category grade that today's code produces. The table records current
//! behaviour; it does not say the behaviour is desirable.
//!
//! Every fake host is mapped to a local TLS listener through the client's
//! `resolve` override. reqwest uses the port in the URL, never the port of the
//! override address, so every fake-host URL carries the listener port.
//!
//! `check_bimi` does not read DMARC; the `_dmarc` record is present only so
//! the stub looks like a domain with a complete mail posture.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::checks::bimi::check_bimi;
use crate::dns::test_support::TestDnsResolver;
use crate::quality::{Grade, Verdict, compute_grade};

const PUBLIC_IP: &str = "93.184.216.34";
const NON_PUBLIC_IP: &str = "10.0.0.1";

/// One canned answer: (host, path, status, Location). Host `"*"` matches any host.
/// `{p}` in a Location is replaced by the listener port.
type Route = (&'static str, &'static str, u16, Option<&'static str>);

struct Row {
    name: &'static str,
    /// BIMI `l=` value; `{p}` is the IPv4 listener port, `{p6}` the IPv6 one.
    logo_url: &'static str,
    /// Hosts the beacon stub resolver answers for: (host, ip).
    stub_ips: &'static [(&'static str, &'static str)],
    /// Hosts the client's resolve map points at the local listener.
    client_hosts: &'static [&'static str],
    routes: &'static [Route],
    /// Nothing listens on the port (connection refused).
    listener_closed: bool,
    /// Also serve the same routes on [::1].
    ipv6: bool,
    expect: &'static [(&'static str, Verdict)],
    grade: Grade,
}

fn fill(s: &str, p: u16, p6: u16) -> String {
    s.replace("{p6}", &p6.to_string())
        .replace("{p}", &p.to_string())
}

fn tls_acceptor() -> TlsAcceptor {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_der = rustls::pki_types::CertificateDer::from(cert.der().to_vec());
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(signing_key.serialize_der()).unwrap();
    // Explicit provider: reqwest brings its own, so no process default is assumed.
    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert_der], key_der)
    .unwrap();
    TlsAcceptor::from(Arc::new(cfg))
}

fn serve(listener: TcpListener, acceptor: TlsAcceptor, routes: &'static [Route], port: u16) {
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
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
                let head = String::from_utf8_lossy(&buf).to_string();
                let path = head
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();
                let host = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("host:")
                            .map(|v| v.trim().to_string())
                    })
                    .unwrap_or_default();
                let host = match host.rsplit_once(':') {
                    Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => h.to_string(),
                    _ => host,
                };
                let (status, location) = routes
                    .iter()
                    .find(|(h, p, _, _)| (*h == "*" || *h == host) && *p == path)
                    .map(|(_, _, s, l)| (*s, *l))
                    .unwrap_or((404, None));
                let mut resp =
                    format!("HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n");
                if let Some(loc) = location {
                    resp.push_str(&format!("Location: {}\r\n", fill(loc, port, port)));
                }
                resp.push_str("\r\n");
                let _ = tls.write_all(resp.as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });
}

const ROWS: &[Row] = &[
    Row {
        // Nothing listens: reqwest returns a connect error, mapped to a Warn.
        name: "1 logo host refuses connections",
        logo_url: "https://logo.example.com:{p}/l.svg",
        stub_ips: &[("logo.example.com", PUBLIC_IP)],
        client_hosts: &["logo.example.com"],
        routes: &[],
        listener_closed: true,
        ipv6: false,
        expect: &[("logo_unreachable", Verdict::Warn)],
        grade: Grade::B,
    },
    Row {
        // Stub returns [] (not blocked) and the client has no entry, so the
        // system resolver fails the lookup: transport error, same Warn.
        name: "2 logo host does not resolve",
        logo_url: "https://nxdomain.example.com:{p}/l.svg",
        stub_ips: &[],
        client_hosts: &[],
        routes: &[],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_unreachable", Verdict::Warn)],
        grade: Grade::B,
    },
    Row {
        // Stub address 10.0.0.1 is non-public: refused before any fetch.
        name: "3 logo host resolves to a non-public address",
        logo_url: "https://logo.example.com:{p}/l.svg",
        stub_ips: &[("logo.example.com", NON_PUBLIC_IP)],
        client_hosts: &["logo.example.com"],
        routes: &[("*", "/l.svg", 200, None)],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_ssrf_blocked", Verdict::Fail)],
        grade: Grade::D,
    },
    Row {
        // Fetch succeeds (200); the post-redirect lookup of the final host
        // hits 10.0.0.1 in the stub, so the redirect is refused after the fact.
        name: "4a final target non-public in stub, answers 200",
        logo_url: "https://logo.example.com:{p}/l.svg",
        stub_ips: &[
            ("logo.example.com", PUBLIC_IP),
            ("final.example.com", NON_PUBLIC_IP),
        ],
        client_hosts: &["logo.example.com", "final.example.com"],
        routes: &[
            (
                "logo.example.com",
                "/l.svg",
                302,
                Some("https://final.example.com:{p}/l.svg"),
            ),
            ("final.example.com", "/l.svg", 200, None),
        ],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_redirect_ssrf_blocked", Verdict::Fail)],
        grade: Grade::D,
    },
    Row {
        // The post-redirect lookup only runs on a success status; a 404 ends in
        // the generic unreachable Warn and the non-public stub address is unused.
        name: "4b final target non-public in stub, answers 404",
        logo_url: "https://logo.example.com:{p}/l.svg",
        stub_ips: &[
            ("logo.example.com", PUBLIC_IP),
            ("final.example.com", NON_PUBLIC_IP),
        ],
        client_hosts: &["logo.example.com", "final.example.com"],
        routes: &[
            (
                "logo.example.com",
                "/l.svg",
                302,
                Some("https://final.example.com:{p}/l.svg"),
            ),
            ("final.example.com", "/l.svg", 404, None),
        ],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_unreachable", Verdict::Warn)],
        grade: Grade::B,
    },
    Row {
        // Only the first and last hosts are looked up; the middle hop with a
        // non-public stub address is never examined, so the chain passes.
        name: "5 intermediate hop non-public in stub, public end answers 200",
        logo_url: "https://logo.example.com:{p}/l.svg",
        stub_ips: &[
            ("logo.example.com", PUBLIC_IP),
            ("hop.example.com", NON_PUBLIC_IP),
            ("final.example.com", PUBLIC_IP),
        ],
        client_hosts: &["logo.example.com", "hop.example.com", "final.example.com"],
        routes: &[
            (
                "logo.example.com",
                "/l.svg",
                302,
                Some("https://hop.example.com:{p}/l.svg"),
            ),
            (
                "hop.example.com",
                "/l.svg",
                302,
                Some("https://final.example.com:{p}/l.svg"),
            ),
            ("final.example.com", "/l.svg", 200, None),
        ],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_reachable", Verdict::Pass)],
        grade: Grade::A,
    },
    Row {
        // 4 redirects: reqwest pushes the current URL into `previous` before the
        // policy runs, so the 4th redirect sees len 4 (< 5) and is followed.
        name: "6a four redirects are followed",
        logo_url: "https://logo.example.com:{p}/l0.svg",
        stub_ips: &[("logo.example.com", PUBLIC_IP)],
        client_hosts: &["logo.example.com"],
        routes: &[
            ("*", "/l0.svg", 302, Some("/l1.svg")),
            ("*", "/l1.svg", 302, Some("/l2.svg")),
            ("*", "/l2.svg", 302, Some("/l3.svg")),
            ("*", "/l3.svg", 302, Some("/l4.svg")),
            ("*", "/l4.svg", 200, None),
        ],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_reachable", Verdict::Pass)],
        grade: Grade::A,
    },
    Row {
        // 5 redirects: the 5th sees previous.len() == 5 (>= 5) and the policy
        // errors with "too many redirects", surfacing as a transport error.
        name: "6b five redirects are refused",
        logo_url: "https://logo.example.com:{p}/l0.svg",
        stub_ips: &[("logo.example.com", PUBLIC_IP)],
        client_hosts: &["logo.example.com"],
        routes: &[
            ("*", "/l0.svg", 302, Some("/l1.svg")),
            ("*", "/l1.svg", 302, Some("/l2.svg")),
            ("*", "/l2.svg", 302, Some("/l3.svg")),
            ("*", "/l3.svg", 302, Some("/l4.svg")),
            ("*", "/l4.svg", 302, Some("/l5.svg")),
            ("*", "/l5.svg", 200, None),
        ],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_unreachable", Verdict::Warn)],
        grade: Grade::B,
    },
    Row {
        // extract_host gives "127.0.0.1"; the stub has no entry, so [] is not
        // blocked. The fetch is plain 200 and the final host again looks up [].
        name: "7a IPv4 literal logo URL",
        logo_url: "https://127.0.0.1:{p}/l.svg",
        stub_ips: &[],
        client_hosts: &[],
        routes: &[("*", "/l.svg", 200, None)],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_reachable", Verdict::Pass)],
        grade: Grade::A,
    },
    Row {
        // extract_host keeps "x@127.0.0.1" as the host (stub: []), reqwest
        // moves the userinfo into a header and host_str() is "127.0.0.1" (stub: []).
        name: "7b IPv4 literal with userinfo",
        logo_url: "https://x@127.0.0.1:{p}/l.svg",
        stub_ips: &[],
        client_hosts: &[],
        routes: &[("*", "/l.svg", 200, None)],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_reachable", Verdict::Pass)],
        grade: Grade::A,
    },
    Row {
        // extract_host cuts at the first ':' and yields "[" (stub: []); the
        // fetch reaches [::1] and host_str() "[::1]" also looks up as [].
        name: "7c IPv6 literal logo URL",
        logo_url: "https://[::1]:{p6}/l.svg",
        stub_ips: &[],
        client_hosts: &[],
        routes: &[("*", "/l.svg", 200, None)],
        listener_closed: false,
        ipv6: true,
        expect: &[("logo_reachable", Verdict::Pass)],
        grade: Grade::A,
    },
    Row {
        // The beacon resolver answers [] (same shape as a resolver error), which
        // is not "blocked"; the client reaches the listener and gets 200.
        name: "8 beacon resolver empty, client resolver works",
        logo_url: "https://logo.example.com:{p}/l.svg",
        stub_ips: &[],
        client_hosts: &["logo.example.com"],
        routes: &[("*", "/l.svg", 200, None)],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_reachable", Verdict::Pass)],
        grade: Grade::A,
    },
    Row {
        // Fallback for an IP-literal redirect: the client cannot intercept
        // 10.1.2.3 without contacting it, so the redirect goes to the local
        // literal 127.0.0.1 instead. The final host looks up as [] in the stub,
        // which is not blocked, so the redirect to a literal passes.
        name: "9 redirect to an IP literal (local literal used)",
        logo_url: "https://logo.example.com:{p}/l.svg",
        stub_ips: &[("logo.example.com", PUBLIC_IP)],
        client_hosts: &["logo.example.com"],
        routes: &[
            (
                "logo.example.com",
                "/l.svg",
                302,
                Some("https://127.0.0.1:{p}/l.svg"),
            ),
            ("127.0.0.1", "/l.svg", 200, None),
        ],
        listener_closed: false,
        ipv6: false,
        expect: &[("logo_reachable", Verdict::Pass)],
        grade: Grade::A,
    },
];

#[tokio::test]
async fn bimi_results_table() {
    let mut failures = Vec::new();

    for row in ROWS {
        let acceptor = tls_acceptor();

        let v4 = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let p = v4.local_addr().unwrap().port();
        let mut p6 = 0;
        if row.ipv6 {
            let v6 = TcpListener::bind((Ipv6Addr::LOCALHOST, 0))
                .await
                .expect("the IPv6 loopback listener is required for this row");
            p6 = v6.local_addr().unwrap().port();
            serve(v6, acceptor.clone(), row.routes, p6);
        }
        if row.listener_closed {
            drop(v4);
        } else {
            serve(v4, acceptor, row.routes, p);
        }

        let mut builder =
            crate::state::http_client_follow_builder(5_000).danger_accept_invalid_certs(true);
        for host in row.client_hosts {
            builder = builder.resolve(host, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p));
        }
        let client = builder.build().unwrap();

        let record = format!("v=BIMI1; l={}", fill(row.logo_url, p, p6));
        let mut resolver = TestDnsResolver::new()
            .with_txt("default._bimi.example.com", vec![record.as_str()])
            .with_txt("_dmarc.example.com", vec!["v=DMARC1; p=reject"]);
        for (host, ip) in row.stub_ips {
            resolver = resolver.with_ips(host, vec![ip.parse().unwrap()]);
        }

        let (result, present) = check_bimi("example.com", &resolver, &client).await;
        let got: Vec<(String, Verdict)> = result
            .sub_checks
            .iter()
            .map(|s| (s.name.clone(), s.verdict))
            .collect();
        let want: Vec<(String, Verdict)> = row
            .expect
            .iter()
            .map(|(n, v)| (n.to_string(), *v))
            .collect();
        let grade = compute_grade(&[result.verdict]);

        if !present || got != want || grade != row.grade {
            failures.push(format!(
                "row {}: present={present} sub_checks={got:?} grade={grade:?}; want {want:?} grade={:?}",
                row.name, row.grade
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
