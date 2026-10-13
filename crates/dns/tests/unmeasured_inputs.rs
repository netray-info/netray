// A check is `unmeasured` only when one of ITS inputs is unmeasured, never because of an
// unrelated lookup (Rule M, input-scoped).
//
// Spec: specs/features/v2-unmeasured/spec.md, C14 (https_svcb), C15 (cname_apex), C16 (NS
// non-timeout failure), C17 (private NS address stays a measured warning).
//
// Assumed interface (not in the tree yet), in `netray_dns::testing`, feature `testing`:
//
//     pub enum NsStub { Authoritative, Timeout, NxDomain, NetError, Private }
//
// `NetError`: every raw query to the host fails with `RawError::Io` (not a timeout).
// `Private`: the host resolves to 10.0.0.1, which the outbound policy refuses.
//
// Cargo needs on netray-dns: `[[test]] name = "unmeasured_inputs"`,
// `path = "tests/unmeasured_inputs.rs"`, `required-features = ["testing"]`.

use mhost::resolver::Lookups;
use netray_dns::api::{BatchEvent, LintEvent};
use netray_dns::testing::{NsStub, ns_check_outcomes};
use netray_dns::{CheckEvent, translate};
use netray_engine::SectionOutcome;
use netray_model::{CheckResult, Status};
use serde_json::{Value, json};

const DOMAIN: &str = "example.com.";
const SERVERS: [&str; 2] = ["udp:1.1.1.1:53", "udp:8.8.8.8:53"];

/// What every resolver answered for a batch.
enum Answer {
    Servfail,
    /// Definite empty answer: NOERROR, no records.
    Nodata,
    Soa,
}

fn result_of(answer: &Answer) -> Value {
    match answer {
        Answer::Servfail => json!({ "NxDomain": {
            "response_time": { "secs": 0, "nanos": 12_000_000 }, "response_code": "SERVFAIL" } }),
        Answer::Nodata => json!({ "NxDomain": {
            "response_time": { "secs": 0, "nanos": 12_000_000 }, "response_code": "NOERROR" } }),
        Answer::Soa => json!({ "Response": {
            "records": [{ "name": DOMAIN, "type": "SOA", "ttl": 300, "data": { "SOA": {
                "mname": "ns1.example.com.", "rname": "hostmaster.example.com.",
                "serial": 1, "refresh": 7200, "retry": 3600, "expire": 1209600, "minimum": 300
            }}}],
            "response_time": { "secs": 0, "nanos": 12_000_000 },
            "valid_until": "2099-01-01T00:00:00Z"
        }}),
    }
}

fn batch(record_type: &str, answer: Answer) -> CheckEvent {
    let items: Vec<Value> = SERVERS
        .iter()
        .map(|ns| {
            json!({
                "query": { "name": DOMAIN, "record_type": record_type },
                "name_server": ns,
                "result": result_of(&answer)
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

fn lint(category: &str, result: Value) -> CheckEvent {
    CheckEvent::Lint(
        serde_json::from_value::<LintEvent>(
            json!({ "request_id": "r", "category": category, "results": [result] }),
        )
        .expect("lint event deserialises"),
    )
}

#[test]
fn a_check_is_unmeasured_only_by_its_own_inputs() {
    // (case, events, check id, expected status)
    let rows: Vec<(&str, Vec<CheckEvent>, &str, Status)> = vec![
        (
            "C14 HTTPS NODATA + SVCB SERVFAIL",
            vec![
                batch("HTTPS", Answer::Nodata),
                batch("SVCB", Answer::Servfail),
                lint("https_svcb", json!({ "NotFound": [] })),
            ],
            "dns.https_svcb",
            Status::Unmeasured,
        ),
        (
            "C15 A SERVFAIL is not an input of cname_apex",
            vec![
                batch("A", Answer::Servfail),
                batch("CNAME", Answer::Nodata),
                batch("SOA", Answer::Soa),
                lint("cname_apex", json!({ "Ok": "Apex zone without CNAME" })),
            ],
            "dns.cname_apex",
            Status::Pass,
        ),
        (
            "C15 CNAME SERVFAIL",
            vec![
                batch("CNAME", Answer::Servfail),
                batch("SOA", Answer::Soa),
                lint("cname_apex", json!({ "Ok": "Apex zone without CNAME" })),
            ],
            "dns.cname_apex",
            Status::Unmeasured,
        ),
    ];

    for (case, events, id, want) in rows {
        let SectionOutcome::Measured { checks, .. } = translate(&events) else {
            panic!("{case}: expected Measured");
        };
        let check = checks
            .iter()
            .find(|c| c.id.to_string() == id)
            .unwrap_or_else(|| panic!("{case}: {id} missing"));
        assert_eq!(check.status, want, "{case}: {id}: {:?}", check.findings);
        if want == Status::Unmeasured {
            assert!(
                check.findings.iter().any(|f| !f.trim().is_empty()),
                "{case}: {id} carries a finding"
            );
        }
    }
}

fn outcome<'a>(results: &'a [CheckResult], id: &str) -> &'a CheckResult {
    results
        .iter()
        .find(|r| r.id.to_string() == id)
        .unwrap_or_else(|| panic!("{id} missing from {results:?}"))
}

#[tokio::test]
async fn ns_network_error_is_unmeasured_and_names_the_server() {
    let results = ns_check_outcomes(
        "example.com",
        &[
            ("ns1.example.com", NsStub::NetError),
            ("ns2.example.com", NsStub::NetError),
        ],
    )
    .await;
    for id in ["dns.ns_lame", "dns.ns_delegation"] {
        let r = outcome(&results, id);
        assert_eq!(r.status, Status::Unmeasured, "{id}: {r:?}");
        assert!(
            r.findings.iter().any(|f| f.contains("ns1.example.com")),
            "{id} finding names the server: {:?}",
            r.findings
        );
    }
}

#[tokio::test]
async fn private_ns_address_stays_a_measured_warning() {
    let results = ns_check_outcomes(
        "example.com",
        &[
            ("ns1.example.com", NsStub::Private),
            ("ns2.example.com", NsStub::Timeout),
        ],
    )
    .await;
    let r = outcome(&results, "dns.ns_lame");
    assert_eq!(r.status, Status::Warn, "{r:?}");
    assert!(
        r.findings
            .iter()
            .any(|f| f.to_lowercase().contains("not public")
                || f.to_lowercase().contains("not a public")),
        "finding carries the not-public text: {:?}",
        r.findings
    );
    assert!(
        r.findings.iter().any(|f| f.contains("ns2.example.com")),
        "finding names the silent ns2: {:?}",
        r.findings
    );
}
