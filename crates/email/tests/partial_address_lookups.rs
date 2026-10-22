// v2-unmeasured, criteria C26-C28: a half-failed address lookup (`lookup_ips` where only the
// AAAA half errors) must not erase what the other half answered.
//
// Each row drives beacon's whole pipeline (`run_all_checks`) offline against an in-memory
// `TestDnsResolver`, translates the SSE events with `translate`, and asserts the V2 status of
// the check the row is about. Seam and helpers as in `unmeasured_outcomes.rs` (loopback MTA-STS
// policy server via `with_mta_sts_port`) and `unmeasured_partial.rs` (`fail_aaaa`).

use std::net::IpAddr;
use std::sync::Arc;

use netray_email::checks::run_all_checks;
use netray_email::dns::FetchResolver;
use netray_email::dns::test_support::TestDnsResolver;
use netray_email::quality::types::SseEvent;
use netray_email::state::OutboundFetch;
use netray_email::{ModuleConfig, translate};
use netray_engine::SectionOutcome;
use netray_model::{CheckResult, Status};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const DOMAIN: &str = "example.com";
const MX_HOST: &str = "mx.example.com";
const POLICY: &str = "version: STSv1\r\nmode: enforce\r\nmx: mx.example.com\r\nmax_age: 86400\r\n";

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

/// One MX host `MX_HOST` with the given addresses and an SPF record.
fn base(ips: &[&str]) -> TestDnsResolver {
    TestDnsResolver::new()
        .with_mx(DOMAIN, vec![(10, MX_HOST)])
        .with_ips(MX_HOST, ips.iter().map(|s| ip(s)).collect())
        .with_txt(DOMAIN, vec!["v=spf1 mx -all"])
}

struct Row {
    name: &'static str,
    dns: fn() -> TestDnsResolver,
    /// Serve a valid MTA-STS policy on a loopback listener.
    policy: bool,
    id: &'static str,
    want: Status,
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            name: "C26 MTA-STS policy host: A answers, AAAA errors",
            dns: || {
                base(&["192.0.2.10"])
                    .with_txt("_mta-sts.example.com", vec!["v=STSv1; id=1"])
                    .with_ips("mta-sts.example.com", vec![ip("127.0.0.1")])
                    .fail_aaaa("mta-sts.example.com")
            },
            policy: true,
            id: "email.mta_sts",
            want: Status::Pass,
        },
        Row {
            name: "C27 FCrDNS: PTR name's A confirms, its AAAA errors",
            dns: || {
                base(&["192.0.2.10"])
                    .with_ptr(ip("192.0.2.10"), vec!["out.example.net"])
                    .with_ips("out.example.net", vec![ip("192.0.2.10")])
                    .fail_aaaa("out.example.net")
            },
            policy: false,
            id: "email.fcrdns",
            want: Status::Pass,
        },
        Row {
            name: "C28 FCrDNS: IPv6 MX IP, PTR name's A answers, AAAA errors",
            dns: || {
                base(&["2001:db8::25"])
                    .with_ptr(ip("2001:db8::25"), vec!["out.example.net"])
                    .with_ips("out.example.net", vec![ip("192.0.2.25")])
                    .fail_aaaa("out.example.net")
            },
            policy: false,
            id: "email.fcrdns",
            want: Status::Unmeasured,
        },
    ]
}

fn config() -> Arc<ModuleConfig> {
    let module: ModuleConfig =
        toml::from_str("[dnsbl]\nzones = [\"bl1.example.com\"]\ntimeout_ms = 1000\n").unwrap();
    Arc::new(module)
}

/// A loopback TLS listener answering every request with a valid MTA-STS policy; returns its port.
async fn start_policy_listener() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["mta-sts.example.com".to_string()]).unwrap();
    let key = rustls::pki_types::PrivateKeyDer::try_from(signing_key.serialize_der()).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let server_config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(cert.der().to_vec())],
            key,
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{POLICY}",
        POLICY.len()
    );
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
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
    port
}

async fn run_row(row: &Row) -> SectionOutcome {
    let dns = Arc::new((row.dns)());
    let mut fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
    // 127.0.0.1 stands in for a public address; the listener's certificate is self-signed.
    fetch.allow = |_| true;
    fetch.settings.accept_invalid_certs = true;
    if row.policy {
        fetch = fetch.with_mta_sts_port(start_policy_listener().await);
    }

    let (tx, mut rx) = mpsc::channel::<SseEvent>(64);
    let task = tokio::spawn(run_all_checks(
        DOMAIN.to_string(),
        Vec::new(),
        config(),
        dns.clone(),
        dns,
        fetch,
        None,
        tx,
    ));
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let done = matches!(event, SseEvent::Summary { .. });
        events.push(event);
        if done {
            break;
        }
    }
    task.abort();
    translate(&events)
}

fn check<'a>(checks: &'a [CheckResult], id: &str) -> Option<&'a CheckResult> {
    checks.iter().find(|c| c.id.to_string() == id)
}

#[test]
fn partial_address_lookups_table() {
    let mut problems = Vec::new();
    for row in rows() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let outcome = runtime.block_on(run_row(&row));
        let SectionOutcome::Measured { checks, .. } = &outcome else {
            problems.push(format!("{}: expected Measured, got {outcome:?}", row.name));
            continue;
        };
        let Some(c) = check(checks, row.id) else {
            problems.push(format!("{}: no {}", row.name, row.id));
            continue;
        };
        if c.status != row.want {
            problems.push(format!(
                "{}: {} expected {:?}, got {:?} {:?}",
                row.name, row.id, row.want, c.status, c.findings
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
