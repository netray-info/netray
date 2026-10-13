// Contract: every DNS catalogue check can report its worst measured status and can report
// `Unmeasured` with a finding. One table, two rows per `dns.*` id in checks.toml.
//
// Spec: specs/features/v2-unmeasured/spec.md, requirements 7 and 16 (C4, C13).
//
// Seam: `netray_dns::translate` over prism's check events, as in lookup_errors_unmeasured.rs.
// An unmeasured row is the record types the check's lint reads, each answered by every resolver
// with SERVFAIL, next to the lint event prism still emits for them. The two NS-walk checks
// (`dns.ns_lame`, `dns.ns_delegation`) are not lint-over-batch: their unmeasured row runs the
// production checks against nameservers that all time out (`testing::ns_check_outcomes`, as in
// ns_timeouts.rs), so this file needs `required-features = ["testing"]` in Cargo.toml.
//
// Check id -> record types whose batches the lint reads (check.rs `lint_lookups`, mhost lints):
//   dns.dnssec            DNSKEY, DS
//   dns.dnskey_algorithm  DNSKEY
//   dns.dnssec_rollover   DNSKEY
//   dns.caa               CAA
//   dns.ns                NS
//   dns.cname_apex        CNAME, A
//   dns.https_svcb        HTTPS
//   dns.ttl               every record type (the lint scans all unique records)
//   dns.ns_lame           raw NS-host queries (stubbed), not a batch
//   dns.ns_delegation     raw NS-host queries (stubbed), not a batch
//
// Exceptions (ids without a row): none.

use std::collections::BTreeSet;

use mhost::resolver::Lookups;
use netray_dns::api::{BatchEvent, LintEvent};
use netray_dns::testing::{NsStub, ns_check_outcomes};
use netray_dns::{CheckEvent, translate};
use netray_engine::SectionOutcome;
use netray_model::Status;
use serde_json::json;

const DOMAIN: &str = "example.com.";
const ALL_TYPES: &[&str] = &[
    "A", "AAAA", "NS", "MX", "TXT", "SOA", "CNAME", "CAA", "DNSKEY", "DS", "HTTPS", "SVCB",
];

/// A batch of `record_type` in which every resolver answered SERVFAIL.
fn failed_batch(record_type: &str) -> CheckEvent {
    let items: Vec<_> = ["udp:1.1.1.1:53", "udp:8.8.8.8:53"]
        .iter()
        .map(|ns| {
            json!({
                "query": { "name": DOMAIN, "record_type": record_type },
                "name_server": ns,
                "result": { "NxDomain": {
                    "response_time": { "secs": 0, "nanos": 12_000_000 },
                    "response_code": "SERVFAIL"
                }}
            })
        })
        .collect();
    let lookups: Lookups =
        serde_json::from_value(json!({ "lookups": items })).expect("lookups deserialise");
    CheckEvent::Batch(BatchEvent {
        request_id: "r".to_owned(),
        record_type: record_type.to_owned(),
        lookups,
        completed: 1,
        total: 1,
        transport: None,
        source: None,
    })
}

fn lint(category: &str, result: serde_json::Value) -> CheckEvent {
    CheckEvent::Lint(
        serde_json::from_value::<LintEvent>(json!({
            "request_id": "r", "category": category, "results": [result]
        }))
        .expect("lint event deserialises"),
    )
}

enum Unmeasured {
    /// Batches that errored on every resolver, then the lint event prism emits for them.
    Batches(&'static [&'static str], serde_json::Value),
    /// Production NS checks against nameservers that all time out.
    NsAllTimeout,
}

struct Row {
    id: &'static str,
    worst: Status,
    /// The lint result that makes `worst`.
    worst_lint: serde_json::Value,
    unmeasured: Unmeasured,
}

fn table() -> Vec<Row> {
    let fail = || json!({ "Failed": "broken" });
    let warn = || json!({ "Warning": "questionable" });
    let none = || json!({ "NotFound": [] });
    let ok = || json!({ "Ok": "fine" });
    vec![
        Row {
            id: "dns.dnssec",
            worst: Status::Fail,
            worst_lint: fail(),
            unmeasured: Unmeasured::Batches(&["DNSKEY", "DS"], warn()),
        },
        Row {
            id: "dns.dnskey_algorithm",
            worst: Status::Fail,
            worst_lint: fail(),
            unmeasured: Unmeasured::Batches(&["DNSKEY"], none()),
        },
        Row {
            id: "dns.dnssec_rollover",
            worst: Status::Fail,
            worst_lint: fail(),
            unmeasured: Unmeasured::Batches(&["DNSKEY"], none()),
        },
        Row {
            id: "dns.caa",
            worst: Status::Warn,
            worst_lint: warn(),
            unmeasured: Unmeasured::Batches(&["CAA"], warn()),
        },
        Row {
            id: "dns.ns",
            worst: Status::Fail,
            worst_lint: fail(),
            unmeasured: Unmeasured::Batches(&["NS"], none()),
        },
        Row {
            id: "dns.ns_lame",
            worst: Status::Fail,
            worst_lint: fail(),
            unmeasured: Unmeasured::NsAllTimeout,
        },
        Row {
            id: "dns.ns_delegation",
            worst: Status::Fail,
            worst_lint: fail(),
            unmeasured: Unmeasured::NsAllTimeout,
        },
        Row {
            id: "dns.cname_apex",
            worst: Status::Fail,
            worst_lint: fail(),
            unmeasured: Unmeasured::Batches(&["CNAME", "A"], ok()),
        },
        Row {
            id: "dns.https_svcb",
            worst: Status::Fail,
            worst_lint: fail(),
            unmeasured: Unmeasured::Batches(&["HTTPS"], none()),
        },
        Row {
            id: "dns.ttl",
            worst: Status::Warn,
            worst_lint: warn(),
            unmeasured: Unmeasured::Batches(ALL_TYPES, ok()),
        },
    ]
}

fn status_of(events: &[CheckEvent], id: &str) -> (Status, Vec<String>) {
    let SectionOutcome::Measured { checks, .. } = translate(events) else {
        panic!("{id}: expected Measured");
    };
    let c = checks
        .iter()
        .find(|c| c.id.to_string() == id)
        .unwrap_or_else(|| panic!("{id} missing"));
    (c.status, c.findings.clone())
}

fn category(id: &str) -> &str {
    id.strip_prefix("dns.").expect("dns. prefix")
}

#[tokio::test]
async fn every_dns_check_has_worst_and_unmeasured_status() {
    for row in table() {
        let (got, findings) = status_of(&[lint(category(row.id), row.worst_lint.clone())], row.id);
        assert_eq!(
            got, row.worst,
            "{}: worst measured status ({findings:?})",
            row.id
        );

        let (got, findings) = match &row.unmeasured {
            Unmeasured::Batches(types, lint_result) => {
                let mut events: Vec<CheckEvent> = types.iter().map(|t| failed_batch(t)).collect();
                events.push(lint(category(row.id), lint_result.clone()));
                status_of(&events, row.id)
            }
            Unmeasured::NsAllTimeout => {
                let results = ns_check_outcomes(
                    "example.com",
                    &[
                        ("ns1.example.com", NsStub::Timeout),
                        ("ns2.example.com", NsStub::Timeout),
                    ],
                )
                .await;
                let r = results
                    .iter()
                    .find(|r| r.id.to_string() == row.id)
                    .unwrap_or_else(|| panic!("{} missing", row.id));
                (r.status, r.findings.clone())
            }
        };
        assert_eq!(
            got,
            Status::Unmeasured,
            "{}: unmeasured row ({findings:?})",
            row.id
        );
        assert!(
            findings.iter().any(|f| !f.trim().is_empty()),
            "{}: unmeasured needs a finding",
            row.id
        );
    }
}

#[test]
fn table_ids_equal_catalogue_dns_ids() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine/catalogue/checks.toml"
    );
    let toml: toml::Value =
        toml::from_str(&std::fs::read_to_string(path).expect("read checks.toml"))
            .expect("parse checks.toml");
    let catalogue: BTreeSet<String> = toml["check"]
        .as_array()
        .expect("[[check]] array")
        .iter()
        .filter_map(|c| c["id"].as_str())
        .filter(|id| id.starts_with("dns."))
        .map(str::to_owned)
        .collect();
    let covered: BTreeSet<String> = table().iter().map(|r| r.id.to_owned()).collect();
    // Exceptions: none for DNS.
    assert_eq!(
        covered, catalogue,
        "table ids must equal the dns.* catalogue ids"
    );
}
