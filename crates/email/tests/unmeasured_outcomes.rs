// v2-unmeasured, requirement 14 (Rule M for email), criteria C3-C16 of report.md Phase 5.
//
// Each row drives beacon's whole pipeline (`run_all_checks`) offline against an in-memory
// `TestDnsResolver` with per-lookup error injection, translates the SSE events with
// `translate`, and asserts the V2 status of the email checks the row is about.
//
// Seam assumed (implemented to match):
// - `netray_email::dns::test_support::TestDnsResolver`, public under the `testing` feature.
//   `fail_mx(name)`, `fail_txt(name)`, `fail_a(name)`: that lookup errors (SERVFAIL or timeout
//   on every resolver); a name without records is a definite empty answer (NXDOMAIN/NODATA).
// - `netray_email::state::OutboundFetch::with_mta_sts_port(port)`: the port the MTA-STS policy
//   fetch connects to instead of 443, so a loopback listener can answer it.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

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
const MX_HOST: &str = "mail.example.com";
const MX_IPS: [&str; 2] = ["192.0.2.10", "198.51.100.10"];
const ZONES: [&str; 3] = ["bl1.example.com", "bl2.example.com", "bl3.example.com"];

const MX_DEPENDENT: [&str; 7] = [
    "email.mx",
    "email.mta_sts",
    "email.tls_rpt",
    "email.bimi",
    "email.dane",
    "email.dnsbl",
    "email.fcrdns",
];

fn mx_ips() -> Vec<IpAddr> {
    MX_IPS.iter().map(|s| s.parse().unwrap()).collect()
}

/// The DNSBL query name for `ip` in `zone`.
fn dnsbl_name(ip: &str, zone: &str) -> String {
    let o: Vec<&str> = ip.split('.').collect();
    format!("{}.{}.{}.{}.{zone}", o[3], o[2], o[1], o[0])
}

/// A mail domain: one MX host with two IPv4 addresses in different /24s, an SPF record.
fn mail_domain() -> TestDnsResolver {
    TestDnsResolver::new()
        .with_mx(DOMAIN, vec![(10, MX_HOST)])
        .with_ips(MX_HOST, mx_ips())
        .with_txt(DOMAIN, vec!["v=spf1 mx -all"])
}

/// `answer` for every MX address in `zone`.
fn zone_answers(r: TestDnsResolver, zone: &str, answer: &str) -> TestDnsResolver {
    MX_IPS.iter().fold(r, |r, ip| {
        r.with_a(&dnsbl_name(ip, zone), vec![answer.parse().unwrap()])
    })
}

/// Every MX address lookup in `zone` errors.
fn zone_fails(r: TestDnsResolver, zone: &str) -> TestDnsResolver {
    MX_IPS
        .iter()
        .fold(r, |r, ip| r.fail_a(&dnsbl_name(ip, zone)))
}

/// A mail domain with an `_mta-sts` record whose policy host resolves to the loopback listener.
fn mta_sts_domain() -> TestDnsResolver {
    mail_domain()
        .with_txt("_mta-sts.example.com", vec!["v=STSv1; id=20200101T000000"])
        .with_ips("mta-sts.example.com", vec!["127.0.0.1".parse().unwrap()])
}

#[derive(Clone, Copy)]
enum Policy {
    /// No listener: the policy fetch goes nowhere the row cares about.
    None,
    /// Accepts the connection and never answers: the fetch times out.
    Silent,
    /// Answers every request with HTTP 404.
    NotFound,
}

enum Expect {
    /// `(check id, status, substring every one of which must appear in the findings)`.
    Checks(Vec<(&'static str, Status, Vec<&'static str>)>),
    TimedOut,
}

struct Row {
    name: &'static str,
    dns: fn() -> TestDnsResolver,
    policy: Policy,
    /// Run with a paused clock (pipeline timeout row).
    paused: bool,
    expect: Expect,
}

fn each(ids: &[&'static str], status: Status, needle: &'static str) -> Expect {
    let needles = if needle.is_empty() {
        vec![]
    } else {
        vec![needle]
    };
    Expect::Checks(
        ids.iter()
            .map(|id| (*id, status, needles.clone()))
            .collect(),
    )
}

fn config() -> Arc<ModuleConfig> {
    let zones = ZONES.map(|z| format!("\"{z}\"")).join(", ");
    let module: ModuleConfig =
        toml::from_str(&format!("[dnsbl]\nzones = [{zones}]\ntimeout_ms = 1000\n")).unwrap();
    Arc::new(module)
}

/// Starts a loopback listener for `policy`; returns its port.
async fn start_policy_listener(policy: Policy) -> Option<u16> {
    let listener = match policy {
        Policy::None => return None,
        _ => TcpListener::bind("127.0.0.1:0").await.unwrap(),
    };
    let port = listener.local_addr().unwrap().port();
    match policy {
        Policy::None => unreachable!(),
        Policy::Silent => {
            tokio::spawn(async move {
                let mut held = Vec::new();
                while let Ok((tcp, _)) = listener.accept().await {
                    held.push(tcp);
                }
            });
        }
        Policy::NotFound => {
            let rcgen::CertifiedKey { cert, signing_key } =
                rcgen::generate_simple_self_signed(vec!["mta-sts.example.com".to_string()])
                    .unwrap();
            let key =
                rustls::pki_types::PrivateKeyDer::try_from(signing_key.serialize_der()).unwrap();
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
            let response =
                "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
            tokio::spawn(async move {
                while let Ok((tcp, _)) = listener.accept().await {
                    let acceptor = acceptor.clone();
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
        }
    }
    Some(port)
}

async fn run_row(row: &Row) -> SectionOutcome {
    let dns = Arc::new((row.dns)());
    let mut fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
    // 127.0.0.1 stands in for a public address; the listener's certificate is self-signed.
    fetch.allow = |_| true;
    fetch.settings.accept_invalid_certs = true;
    if let Some(port) = start_policy_listener(row.policy).await {
        fetch = fetch.with_mta_sts_port(port);
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

fn rows() -> Vec<Row> {
    vec![
        Row {
            name: "C3 MX lookup SERVFAIL",
            dns: || mail_domain().fail_mx(DOMAIN),
            policy: Policy::None,
            paused: false,
            expect: each(&MX_DEPENDENT, Status::Unmeasured, ""),
        },
        Row {
            name: "C4 definite empty MX",
            dns: || TestDnsResolver::new().with_txt(DOMAIN, vec!["v=spf1 -all"]),
            policy: Policy::None,
            paused: false,
            expect: each(&MX_DEPENDENT, Status::NotApplicable, "no MX"),
        },
        Row {
            name: "C16 definite empty MX, TLS-RPT TXT SERVFAIL",
            dns: || {
                TestDnsResolver::new()
                    .with_txt(DOMAIN, vec!["v=spf1 -all"])
                    .fail_txt("_smtp._tls.example.com")
            },
            policy: Policy::None,
            paused: false,
            expect: each(&["email.tls_rpt"], Status::NotApplicable, "no MX"),
        },
        Row {
            name: "C5 one MX host, two IPv4 addresses, no IPv6",
            dns: mail_domain,
            policy: Policy::None,
            paused: false,
            expect: Expect::Checks(vec![(
                "email.mx",
                Status::Pass,
                vec!["only one MX record", "AAAA"],
            )]),
        },
        Row {
            name: "C6 no BIMI record",
            dns: mail_domain,
            policy: Policy::None,
            paused: false,
            expect: each(&["email.bimi"], Status::NotApplicable, ""),
        },
        Row {
            name: "C7 one zone lists, two time out",
            dns: || {
                let r = zone_answers(mail_domain(), ZONES[0], "127.0.0.2");
                zone_fails(zone_fails(r, ZONES[1]), ZONES[2])
            },
            policy: Policy::None,
            paused: false,
            expect: each(&["email.dnsbl"], Status::Fail, ""),
        },
        Row {
            name: "C8 one zone clean, two refuse",
            dns: || {
                let r = zone_answers(mail_domain(), ZONES[1], "127.255.255.254");
                zone_answers(r, ZONES[2], "127.255.255.254")
            },
            policy: Policy::None,
            paused: false,
            expect: Expect::Checks(vec![(
                "email.dnsbl",
                Status::Pass,
                vec![ZONES[1], ZONES[2]],
            )]),
        },
        Row {
            name: "C9 every zone refuses or SERVFAILs",
            dns: || {
                let r = zone_answers(mail_domain(), ZONES[0], "127.255.255.254");
                zone_fails(zone_fails(r, ZONES[1]), ZONES[2])
            },
            policy: Policy::None,
            paused: false,
            expect: each(&["email.dnsbl"], Status::Unmeasured, ""),
        },
        Row {
            name: "C10 MTA-STS policy fetch timeout",
            dns: mta_sts_domain,
            policy: Policy::Silent,
            paused: false,
            expect: each(&["email.mta_sts"], Status::Unmeasured, "timeout"),
        },
        Row {
            name: "C11 MTA-STS policy fetch HTTP 404",
            dns: mta_sts_domain,
            policy: Policy::NotFound,
            paused: false,
            expect: each(&["email.mta_sts"], Status::Fail, "404"),
        },
        Row {
            name: "C12 MTA-STS policy host does not resolve",
            dns: || mail_domain().with_txt("_mta-sts.example.com", vec!["v=STSv1; id=1"]),
            policy: Policy::None,
            paused: false,
            expect: each(&["email.mta_sts"], Status::Fail, ""),
        },
        Row {
            name: "C13 apex TXT lookup timeout",
            dns: || mail_domain().fail_txt(DOMAIN),
            policy: Policy::None,
            paused: false,
            expect: each(&["email.spf"], Status::Unmeasured, ""),
        },
        Row {
            name: "C14 no SPF record",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, mx_ips())
            },
            policy: Policy::None,
            paused: false,
            expect: each(&["email.spf"], Status::Fail, ""),
        },
        Row {
            name: "C15 beacon pipeline timeout",
            dns: || mail_domain().with_delay(Duration::from_secs(40)),
            policy: Policy::None,
            paused: true,
            expect: Expect::TimedOut,
        },
    ]
}

fn check<'a>(checks: &'a [CheckResult], id: &str) -> Option<&'a CheckResult> {
    checks.iter().find(|c| c.id.to_string() == id)
}

#[test]
fn unmeasured_outcomes_table() {
    let mut problems = Vec::new();
    for row in rows() {
        let mut builder = tokio::runtime::Builder::new_multi_thread();
        if row.paused {
            builder = tokio::runtime::Builder::new_current_thread();
            builder.start_paused(true);
        }
        let runtime = builder.enable_all().build().unwrap();
        let outcome = runtime.block_on(run_row(&row));

        match (&row.expect, &outcome) {
            (Expect::TimedOut, SectionOutcome::TimedOut) => {}
            (Expect::TimedOut, other) => {
                problems.push(format!("{}: expected TimedOut, got {other:?}", row.name))
            }
            (Expect::Checks(expected), SectionOutcome::Measured { checks, .. }) => {
                for (id, status, needles) in expected {
                    let Some(c) = check(checks, id) else {
                        problems.push(format!("{}: no {id}", row.name));
                        continue;
                    };
                    if c.status != *status {
                        problems.push(format!(
                            "{}: {id} expected {status:?}, got {:?} {:?}",
                            row.name, c.status, c.findings
                        ));
                    }
                    // Rule M: every unmeasured result names its cause.
                    if *status == Status::Unmeasured && c.findings.is_empty() {
                        problems.push(format!("{}: {id} unmeasured without a finding", row.name));
                    }
                    let text = c.findings.join("\n");
                    for needle in needles {
                        if !text.contains(needle) {
                            problems.push(format!(
                                "{}: {id} findings {:?} do not mention {needle:?}",
                                row.name, c.findings
                            ));
                        }
                    }
                }
            }
            (Expect::Checks(_), other) => {
                problems.push(format!("{}: expected Measured, got {other:?}", row.name))
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
