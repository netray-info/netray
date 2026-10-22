// v2-unmeasured, criteria C41-C43: SPF `exists:` qualifiers and macros. A resolving positive
// `exists:` authorises every IP (C41); a negative-qualified macro mechanism cannot authorise, so
// the coverage gap stays measured (C42); macro targets are never queried, so they cannot count
// as void lookups (C43).
//
// Each row drives `run_all_checks` offline against an in-memory `TestDnsResolver`, as in
// `spf_nested_errors.rs`.

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

/// What a row expects of one check. `status` is exact when set; `status_not` lists forbidden
/// statuses. Findings are matched case-insensitively over the joined findings.
struct Want {
    id: &'static str,
    status: Option<Status>,
    status_not: &'static [Status],
    finding_contains: &'static [&'static str],
    finding_absent: &'static [&'static str],
}

struct Row {
    name: &'static str,
    zones: &'static str,
    dns: fn() -> TestDnsResolver,
    wants: Vec<Want>,
}

const COVERAGE: &str = "MX IP(s) not in SPF";

fn mx_v4(addr: &str) -> TestDnsResolver {
    TestDnsResolver::new()
        .with_mx(DOMAIN, vec![(10, MX_HOST)])
        .with_ips(MX_HOST, vec![ip(addr)])
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            name: "C41 positive exists: resolves, authorises every IP, no coverage gap",
            zones: "bl1.example.com",
            dns: || {
                mx_v4("192.0.2.25")
                    .with_txt(DOMAIN, vec!["v=spf1 exists:relay.example.net -all"])
                    .with_ips("relay.example.net", vec![ip("127.0.0.2")])
            },
            wants: vec![Want {
                id: "email.spf",
                status: None,
                status_not: &[Status::Warn],
                finding_contains: &[],
                finding_absent: &[COVERAGE],
            }],
        },
        Row {
            name: "C42 negative exists: macro cannot authorise, gap stays measured",
            zones: "bl1.example.com",
            dns: || {
                mx_v4("198.51.100.25").with_txt(
                    DOMAIN,
                    vec!["v=spf1 -exists:%{i}._deny.example.net ip4:192.0.2.0/24 -all"],
                )
            },
            wants: vec![Want {
                id: "email.spf",
                status: Some(Status::Warn),
                status_not: &[],
                finding_contains: &["MX IP(s) not in SPF: 198.51.100.25"],
                finding_absent: &[],
            }],
        },
        Row {
            name: "C43 three exists: macro targets, not queried, no void finding",
            zones: "bl1.example.com",
            dns: || {
                mx_v4("192.0.2.25").with_txt(
                    DOMAIN,
                    vec![
                        "v=spf1 exists:%{i}._spf.example.net exists:%{i}._a.example.net exists:%{i}._b.example.net -all",
                    ],
                )
            },
            wants: vec![Want {
                id: "email.spf",
                status: None,
                status_not: &[],
                finding_contains: &[],
                finding_absent: &["void"],
            }],
        },
    ]
}

fn config(zones: &str) -> Arc<ModuleConfig> {
    let module: ModuleConfig = toml::from_str(&format!(
        "[dnsbl]\nzones = [\"{zones}\"]\ntimeout_ms = 1000\n"
    ))
    .unwrap();
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
        config(row.zones),
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
fn spf_exists_qualifier_table() {
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
            if let Some(s) = want.status
                && c.status != s
            {
                problems.push(format!(
                    "{}: {} expected {s:?}, got {:?} {:?}",
                    row.name, want.id, c.status, c.findings
                ));
            }
            if want.status_not.contains(&c.status) {
                problems.push(format!(
                    "{}: {} must not be {:?}: {:?}",
                    row.name, want.id, c.status, c.findings
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
