// v2-unmeasured, criteria C32-C37: a failed lookup is never an answer. When any nested SPF
// lookup (include TXT, a, mx) failed, the authorised set is incomplete, so the MX-coverage
// cross-check must not warn and a failed include must not count as a void lookup. A failed
// family of an `a:` host still leaves the other family measured. DNSBL with no queryable zone
// for the resolved address family is Unmeasured.
//
// Each row drives `run_all_checks` offline against an in-memory `TestDnsResolver`, as in
// `partial_address_findings.rs`.

use std::net::IpAddr;
use std::sync::Arc;

use netray_email::checks::run_all_checks;
use netray_email::config::Config;
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

fn rows() -> Vec<Row> {
    vec![
        Row {
            name: "C32 SPF mx: lookup fails, no coverage Warn",
            zones: "bl1.example.com",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, vec![ip("192.0.2.10")])
                    .with_txt(DOMAIN, vec!["v=spf1 mx:relay.example.net -all"])
                    .fail_mx("relay.example.net")
            },
            wants: vec![Want {
                id: "email.spf",
                status: None,
                status_not: &[Status::Warn],
                finding_contains: &["relay.example.net"],
                finding_absent: &[COVERAGE],
            }],
        },
        Row {
            name: "C33 SPF include: TXT fails, no coverage Warn, not Fail",
            zones: "bl1.example.com",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, vec![ip("192.0.2.10")])
                    .with_txt(DOMAIN, vec!["v=spf1 include:_spf.example.net -all"])
                    .fail_txt("_spf.example.net")
            },
            wants: vec![Want {
                id: "email.spf",
                status: None,
                status_not: &[Status::Warn, Status::Fail],
                finding_contains: &["_spf.example.net"],
                finding_absent: &[COVERAGE],
            }],
        },
        Row {
            name: "C34 three failed includes are not void lookups",
            zones: "bl1.example.com",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, vec![ip("192.0.2.10")])
                    .with_txt(
                        DOMAIN,
                        vec![
                            "v=spf1 include:a.example.net include:b.example.net \
                             include:c.example.net -all",
                        ],
                    )
                    .fail_txt("a.example.net")
                    .fail_txt("b.example.net")
                    .fail_txt("c.example.net")
            },
            wants: vec![Want {
                id: "email.spf",
                status: None,
                status_not: &[Status::Fail],
                finding_contains: &[],
                finding_absent: &["void"],
            }],
        },
        Row {
            name: "C35 family pin: a: host AAAA fails, IPv4 gap still warns",
            zones: "bl1.example.com",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, vec![ip("192.0.2.20")])
                    .with_ips("out.example.com", vec![ip("192.0.2.30")])
                    .fail_aaaa("out.example.com")
                    .with_txt(DOMAIN, vec!["v=spf1 a:out.example.com -all"])
            },
            wants: vec![Want {
                id: "email.spf",
                status: Some(Status::Warn),
                status_not: &[],
                finding_contains: &["MX IP(s) not in SPF: 192.0.2.20"],
                finding_absent: &[],
            }],
        },
        Row {
            name: "C36 address pin: a: host fails fully, no coverage Warn",
            zones: "bl1.example.com",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, vec![ip("192.0.2.20")])
                    .with_txt(DOMAIN, vec!["v=spf1 a:out.example.com -all"])
                    .fail_ips("out.example.com")
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
            name: "C37 dnsbl: IPv4-only zone, MX A fails, AAAA answers",
            zones: "zen.spamhaus.org",
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, vec![ip("2001:db8::25")])
                    .fail_a(MX_HOST)
                    .with_txt(DOMAIN, vec!["v=spf1 mx -all"])
            },
            wants: vec![Want {
                id: "email.dnsbl",
                status: Some(Status::Unmeasured),
                status_not: &[],
                finding_contains: &[],
                finding_absent: &[],
            }],
        },
    ]
}

fn config(zones: &str) -> Arc<Config> {
    let module: ModuleConfig = toml::from_str(&format!(
        "[dnsbl]\nzones = [\"{zones}\"]\ntimeout_ms = 1000\n"
    ))
    .unwrap();
    Arc::new(Config::from_module(module))
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
fn spf_nested_errors_table() {
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
