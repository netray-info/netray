// v2-unmeasured, criteria C29-C31: a half-failed address lookup (only the AAAA half errors)
// leaves the check's status measured, and where an address family stayed unresolved the check
// says so in a finding naming the host.
//
// Each row drives beacon's whole pipeline (`run_all_checks`) offline against an in-memory
// `TestDnsResolver`, translates the SSE events with `translate`, and asserts status and findings
// of the checks the row is about. Helpers as in `partial_address_lookups.rs`.

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
use tokio::sync::mpsc;

const DOMAIN: &str = "example.com";
const MX_HOST: &str = "mx.example.com";

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

/// What a row expects of one check: its status, plus findings that must / must not appear
/// (case-insensitive substring match over the joined findings).
struct Want {
    id: &'static str,
    status: Status,
    finding_contains: &'static [&'static str],
    finding_absent: &'static [&'static str],
}

struct Row {
    name: &'static str,
    dns: fn() -> TestDnsResolver,
    wants: Vec<Want>,
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            name: "C29 MX host: AAAA errors, PTR confirms, DNSBL clean",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, vec![ip("192.0.2.10")])
                    .fail_aaaa(MX_HOST)
                    .with_txt(DOMAIN, vec!["v=spf1 mx -all"])
                    .with_ptr(ip("192.0.2.10"), vec![MX_HOST])
            },
            wants: vec![
                Want {
                    id: "email.fcrdns",
                    status: Status::Pass,
                    finding_contains: &[MX_HOST],
                    finding_absent: &[],
                },
                Want {
                    id: "email.dnsbl",
                    status: Status::Pass,
                    finding_contains: &[MX_HOST],
                    finding_absent: &[],
                },
            ],
        },
        Row {
            name: "C30 SPF a: host AAAA errors, MX IPv6 not a coverage gap",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, vec![ip("192.0.2.10"), ip("2001:db8::25")])
                    // out.example.com carries both MX addresses; its AAAA half errors, so the
                    // SPF side never learns 2001:db8::25. No `mx` mechanism covers it either.
                    .with_ips("out.example.com", vec![ip("192.0.2.10")])
                    .fail_aaaa("out.example.com")
                    .with_txt(DOMAIN, vec!["v=spf1 a:out.example.com -all"])
            },
            wants: vec![Want {
                id: "email.spf",
                status: Status::Pass,
                finding_contains: &[],
                finding_absent: &["MX IP(s) not in SPF", "2001:db8::25"],
            }],
        },
        Row {
            name: "C31 FCrDNS family filter: PTR name's A is another IP, AAAA errors",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, vec![ip("192.0.2.10")])
                    .with_txt(DOMAIN, vec!["v=spf1 mx -all"])
                    .with_ptr(ip("192.0.2.10"), vec!["out.example.net"])
                    .with_ips("out.example.net", vec![ip("192.0.2.99")])
                    .fail_aaaa("out.example.net")
            },
            wants: vec![Want {
                id: "email.fcrdns",
                status: Status::Warn,
                finding_contains: &[],
                finding_absent: &[],
            }],
        },
    ]
}

fn config() -> Arc<ModuleConfig> {
    let module: ModuleConfig =
        toml::from_str("[dnsbl]\nzones = [\"bl1.example.com\"]\ntimeout_ms = 1000\n").unwrap();
    Arc::new(module)
}

async fn run_row(row: &Row) -> SectionOutcome {
    let dns = Arc::new((row.dns)());
    let mut fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
    fetch.allow = |_| true;

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
fn partial_address_findings_table() {
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
        for want in &row.wants {
            let Some(c) = check(checks, want.id) else {
                problems.push(format!("{}: no {}", row.name, want.id));
                continue;
            };
            if c.status != want.status {
                problems.push(format!(
                    "{}: {} expected {:?}, got {:?} {:?}",
                    row.name, want.id, want.status, c.status, c.findings
                ));
            }
            let joined = c.findings.join("\n").to_lowercase();
            for s in want.finding_contains {
                if !joined.contains(&s.to_lowercase()) {
                    problems.push(format!(
                        "{}: {} has no finding naming {s:?}: {:?}",
                        row.name, want.id, c.findings
                    ));
                }
            }
            for s in want.finding_absent {
                if joined.contains(&s.to_lowercase()) {
                    problems.push(format!(
                        "{}: {} must not have a finding with {s:?}: {:?}",
                        row.name, want.id, c.findings
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
