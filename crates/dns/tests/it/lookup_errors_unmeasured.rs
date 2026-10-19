// A lookup that errored, or a check the stream never reported, is `unmeasured`, never a
// measured status (Rule M).
//
// Spec: specs/features/v2-unmeasured/spec.md, requirement 6 (first bullet), scenarios 5 and 6:
// C3a/C9 (DNSKEY SERVFAIL on every resolver), C10 (definite NODATA stays N/A), C3b (stream
// timeout).
//
// Seam: `netray_dns::translate` over prism's check events. A DNSKEY batch whose lookups are
// mhost `NxDomain` results carrying a response code (mhost 0.12.1) is what the pipeline hands
// over; the lint stage still emits `NotFound` for it today. No new event type is assumed.
//
// Construction: lookups are built through their public serde form, as in lint_results_table.rs.

use mhost::resolver::Lookups;
use netray_dns::api::{BatchEvent, LintEvent};
use netray_dns::{CheckEvent, translate};
use netray_engine::SectionOutcome;
use netray_model::Status;
use serde_json::json;

const DOMAIN: &str = "example.com.";

/// A DNSKEY batch in which each name server answered with an `NxDomain` result of `rcode`.
fn dnskey_batch(rcode: &str, servers: &[&str]) -> CheckEvent {
    let items: Vec<_> = servers
        .iter()
        .map(|ns| {
            json!({
                "query": { "name": DOMAIN, "record_type": "DNSKEY" },
                "name_server": ns,
                "result": { "NxDomain": {
                    "response_time": { "secs": 0, "nanos": 12_000_000 },
                    "response_code": rcode
                }}
            })
        })
        .collect();
    let lookups: Lookups =
        serde_json::from_value(json!({ "lookups": items })).expect("lookups deserialise");
    CheckEvent::Batch(BatchEvent {
        request_id: "r".to_owned(),
        record_type: "DNSKEY".to_owned(),
        lookups,
        completed: 1,
        total: 1,
        transport: None,
        source: None,
    })
}

/// A lint event that found nothing: what the lint stage emits for a missing record set.
fn lint_not_found(category: &str) -> CheckEvent {
    CheckEvent::Lint(
        serde_json::from_str::<LintEvent>(
            &json!({
                "request_id": "r", "category": category, "results": [{ "NotFound": [] }]
            })
            .to_string(),
        )
        .expect("lint event deserialises"),
    )
}

fn lint_ok(category: &str) -> CheckEvent {
    CheckEvent::Lint(
        serde_json::from_value::<LintEvent>(json!({
            "request_id": "r", "category": category, "results": [{ "Ok": "fine" }]
        }))
        .expect("lint event deserialises"),
    )
}

struct Row {
    name: &'static str,
    events: Vec<CheckEvent>,
    /// (check id, expected status, whether a non-empty finding is required, text every finding
    /// set must mention, lowercase)
    want: Vec<(&'static str, Status, bool, Option<&'static str>)>,
}

#[test]
fn lookup_errors_are_unmeasured() {
    let servers = ["udp:1.1.1.1:53", "udp:8.8.8.8:53"];
    let rows = vec![
        Row {
            name: "C3a/C9 DNSKEY SERVFAIL on every resolver",
            events: vec![
                dnskey_batch("SERVFAIL", &servers),
                lint_not_found("dnskey_algorithm"),
                lint_not_found("dnssec_rollover"),
            ],
            want: vec![
                (
                    "dns.dnskey_algorithm",
                    Status::Unmeasured,
                    true,
                    Some("dnskey"),
                ),
                (
                    "dns.dnssec_rollover",
                    Status::Unmeasured,
                    true,
                    Some("dnskey"),
                ),
            ],
        },
        Row {
            name: "C10 DNSKEY NODATA is a definite empty answer",
            events: vec![
                dnskey_batch("NOERROR", &servers),
                lint_not_found("dnskey_algorithm"),
                lint_not_found("dnssec_rollover"),
            ],
            want: vec![
                ("dns.dnskey_algorithm", Status::NotApplicable, false, None),
                ("dns.dnssec_rollover", Status::NotApplicable, false, None),
            ],
        },
        Row {
            name: "C3b stream timeout leaves checks without a lint event",
            events: vec![
                lint_ok("ttl"),
                CheckEvent::Error {
                    code: "STREAM_TIMEOUT",
                    message: "stream deadline exceeded".to_owned(),
                },
            ],
            want: vec![
                ("dns.ttl", Status::Pass, false, None),
                ("dns.caa", Status::Unmeasured, true, None),
                ("dns.dnskey_algorithm", Status::Unmeasured, true, None),
            ],
        },
    ];

    for row in rows {
        let SectionOutcome::Measured { checks, .. } = translate(&row.events) else {
            panic!("{}: expected Measured", row.name);
        };
        for (id, status, needs_finding, mention) in row.want {
            let check = checks
                .iter()
                .find(|c| c.id.to_string() == id)
                .unwrap_or_else(|| panic!("{}: {id} missing", row.name));
            assert_eq!(
                check.status, status,
                "{}: {id} ({:?})",
                row.name, check.findings
            );
            if needs_finding {
                assert!(
                    check.findings.iter().any(|f| !f.trim().is_empty()),
                    "{}: {id} carries a finding",
                    row.name
                );
                assert!(
                    !check.findings.iter().any(|f| f == "not reported"),
                    "{}: {id} is not 'not reported'",
                    row.name
                );
            }
            if let Some(text) = mention {
                assert!(
                    check
                        .findings
                        .iter()
                        .any(|f| f.to_lowercase().contains(text)),
                    "{}: {id} finding names the {text} lookup: {:?}",
                    row.name,
                    check.findings
                );
            }
        }
    }
}
