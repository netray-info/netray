// v2-unmeasured, requirement 14 (Rule M for email), criteria C19-C25: partial and incomplete
// lookups must not be read as a definite answer.
//
// Each row drives beacon's whole pipeline (`run_all_checks`) offline against an in-memory
// `TestDnsResolver` with per-lookup error injection, translates the SSE events with
// `translate`, and asserts the V2 status of the email checks the row is about. C25 asserts the
// V1 grade in the summary SSE event.
//
// Seam assumed (new builders on `netray_email::dns::test_support::TestDnsResolver`):
// - `fail_aaaa(name)`: only the AAAA half of `lookup_ips(name)` errors (the A half answers).
//   `lookup_ips(name)` as a whole then returns a `LookupError`.
// - `fail_ips(name)`: both the A and the AAAA half of `lookup_ips(name)` error.
// - `fail_dnskey(name)`: the DNSKEY lookup behind the DNSSEC check of `name` errors.

use std::net::IpAddr;
use std::sync::Arc;

use netray_email::checks::run_all_checks;
use netray_email::dns::FetchResolver;
use netray_email::dns::TlsaRecord;
use netray_email::dns::test_support::TestDnsResolver;
use netray_email::quality::types::{Grade, SseEvent};
use netray_email::state::OutboundFetch;
use netray_email::{ModuleConfig, translate};
use netray_engine::SectionOutcome;
use netray_model::{CheckResult, Status};
use tokio::sync::mpsc;

const DOMAIN: &str = "example.com";
const MX_HOST: &str = "mail.example.com";

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

/// One MX host `host` with SPF and the given addresses.
fn base(host: &str, ips: &[&str]) -> TestDnsResolver {
    TestDnsResolver::new()
        .with_mx(DOMAIN, vec![(10, host)])
        .with_ips(host, ips.iter().map(|s| ip(s)).collect())
        .with_txt(DOMAIN, vec!["v=spf1 mx -all"])
}

enum Want {
    Is(Status),
    IsNot(&'static [Status]),
}

struct Expect {
    id: &'static str,
    want: Want,
    /// Substrings that must appear in the findings.
    has: &'static [&'static str],
    /// Substrings that must not appear in the findings.
    lacks: &'static [&'static str],
}

fn is(id: &'static str, status: Status) -> Expect {
    Expect {
        id,
        want: Want::Is(status),
        has: &[],
        lacks: &[],
    }
}

struct Row {
    name: &'static str,
    dns: fn() -> TestDnsResolver,
    expect: Vec<Expect>,
    /// V1 grade expected in the summary event.
    grade: Option<Grade>,
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            name: "C19 one MX IP confirms, the other's PTR SERVFAILs",
            dns: || {
                base(MX_HOST, &["192.0.2.1", "192.0.2.2"])
                    .with_ptr(ip("192.0.2.1"), vec!["mx1.example.com"])
                    .with_ips("mx1.example.com", vec![ip("192.0.2.1")])
                    .fail_ptr(ip("192.0.2.2"))
            },
            expect: vec![Expect {
                id: "email.fcrdns",
                want: Want::Is(Status::Pass),
                has: &["192.0.2.2"],
                lacks: &[],
            }],
            grade: None,
        },
        Row {
            name: "C20 every MX host's addresses error",
            dns: || base(MX_HOST, &["192.0.2.10"]).fail_ips(MX_HOST),
            expect: vec![
                is("email.fcrdns", Status::Unmeasured),
                is("email.dnsbl", Status::Unmeasured),
            ],
            grade: None,
        },
        Row {
            name: "C21a IPv6 MX IP, PTR forward A ok, forward AAAA errors",
            dns: || {
                base(MX_HOST, &["2001:db8::25"])
                    .with_ptr(ip("2001:db8::25"), vec!["mx.example.com"])
                    .with_ips("mx.example.com", vec![ip("192.0.2.25")])
                    .fail_aaaa("mx.example.com")
            },
            expect: vec![is("email.fcrdns", Status::Unmeasured)],
            grade: None,
        },
        Row {
            name: "C21b single MX host, A ok, AAAA errors",
            dns: || base(MX_HOST, &["192.0.2.10"]).fail_aaaa(MX_HOST),
            expect: vec![Expect {
                id: "email.mx",
                want: Want::IsNot(&[Status::Warn, Status::Fail]),
                has: &[],
                lacks: &["AAAA", "only one MX"],
            }],
            grade: None,
        },
        Row {
            name: "C22 single MX host, A and AAAA both error",
            dns: || base(MX_HOST, &["192.0.2.10"]).fail_ips(MX_HOST),
            expect: vec![is("email.mx", Status::Pass)],
            grade: None,
        },
        Row {
            name: "C23 valid TLSA, DNSKEY lookup errors",
            dns: || {
                base(MX_HOST, &["192.0.2.10"])
                    .with_tlsa(
                        &format!("_25._tcp.{MX_HOST}"),
                        vec![TlsaRecord {
                            usage: 3,
                            selector: 1,
                            matching_type: 1,
                        }],
                    )
                    .fail_dnskey(DOMAIN)
            },
            expect: vec![Expect {
                id: "email.dane",
                want: Want::IsNot(&[Status::Fail]),
                has: &[],
                lacks: &[],
            }],
            grade: None,
        },
        Row {
            name: "C24 MX lookup errors, DKIM key only under a provider selector",
            dns: || {
                TestDnsResolver::new()
                    .with_txt(DOMAIN, vec!["v=spf1 -all"])
                    .with_txt(
                        "selector1._domainkey.example.com",
                        vec!["v=DKIM1; k=rsa; p=MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQC"],
                    )
                    .fail_mx(DOMAIN)
            },
            expect: vec![is("email.dkim", Status::Unmeasured)],
            grade: None,
        },
        Row {
            name: "C25 SPF TXT SERVFAIL, everything else answers",
            dns: || {
                base(MX_HOST, &["192.0.2.10"])
                    .with_txt("_dmarc.example.com", vec!["v=DMARC1; p=reject"])
                    .fail_txt(DOMAIN)
            },
            expect: vec![is("email.spf", Status::Unmeasured)],
            grade: Some(Grade::Skipped),
        },
    ]
}

fn config() -> Arc<ModuleConfig> {
    let module: ModuleConfig =
        toml::from_str("[dnsbl]\nzones = [\"bl1.example.com\"]\ntimeout_ms = 1000\n").unwrap();
    Arc::new(module)
}

async fn run_row(row: &Row) -> (SectionOutcome, Option<Grade>) {
    let dns = Arc::new((row.dns)());
    let fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
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
    let mut grade = None;
    while let Some(event) = rx.recv().await {
        let done = if let SseEvent::Summary { grade: g, .. } = &event {
            grade = Some(*g);
            true
        } else {
            false
        };
        events.push(event);
        if done {
            break;
        }
    }
    task.abort();
    (translate(&events), grade)
}

fn check<'a>(checks: &'a [CheckResult], id: &str) -> Option<&'a CheckResult> {
    checks.iter().find(|c| c.id.to_string() == id)
}

#[test]
fn unmeasured_partial_table() {
    let mut problems = Vec::new();
    for row in rows() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (outcome, grade) = runtime.block_on(run_row(&row));

        if let Some(want) = row.grade
            && grade != Some(want)
        {
            problems.push(format!(
                "{}: V1 grade expected {want:?}, got {grade:?}",
                row.name
            ));
        }
        let SectionOutcome::Measured { checks, .. } = &outcome else {
            problems.push(format!("{}: expected Measured, got {outcome:?}", row.name));
            continue;
        };
        for e in &row.expect {
            let Some(c) = check(checks, e.id) else {
                problems.push(format!("{}: no {}", row.name, e.id));
                continue;
            };
            let ok = match &e.want {
                Want::Is(s) => c.status == *s,
                Want::IsNot(set) => !set.contains(&c.status),
            };
            if !ok {
                problems.push(format!(
                    "{}: {} got {:?} {:?}",
                    row.name, e.id, c.status, c.findings
                ));
            }
            // Rule M: every unmeasured result names its cause.
            if c.status == Status::Unmeasured && c.findings.is_empty() {
                problems.push(format!(
                    "{}: {} unmeasured without a finding",
                    row.name, e.id
                ));
            }
            let text = c.findings.join("\n");
            for n in e.has {
                if !text.contains(n) {
                    problems.push(format!(
                        "{}: {} findings {:?} lack {n:?}",
                        row.name, e.id, c.findings
                    ));
                }
            }
            for n in e.lacks {
                if text.contains(n) {
                    problems.push(format!(
                        "{}: {} findings {:?} contain {n:?}",
                        row.name, e.id, c.findings
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
