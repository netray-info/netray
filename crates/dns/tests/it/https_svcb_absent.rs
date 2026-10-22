// An absent HTTPS/SVCB record set is a `warn` with the finding "Not found", not a `fail`; no
// other check's NotFound changes.
//
// Spec: specs/features/v2-correctness-followups/spec.md, Phase 3, C1/R5 and C5-C9:
// C5 (NODATA + lint NotFound -> warn "Not found"), C6 (mixed alias/service modes stay warn),
// C7 (Ok -> pass), C8 (failed HTTPS batch -> unmeasured), C9 (caa NotFound stays fail).
//
// Seam: `netray_dns::translate` over prism's check events; batches are built through their
// public serde form, as in lookup_errors_unmeasured.rs.

use mhost::resolver::Lookups;
use netray_dns::api::{BatchEvent, LintEvent};
use netray_dns::{CheckEvent, translate};
use netray_engine::SectionOutcome;
use netray_model::Status;
use serde_json::{Value, json};

const DOMAIN: &str = "example.com.";
const SERVERS: [&str; 2] = ["udp:1.1.1.1:53", "udp:8.8.8.8:53"];

/// A batch in which every name server answered with an `NxDomain` result of `rcode`
/// (NOERROR is a definite NODATA, SERVFAIL a failed lookup).
fn batch(record_type: &str, rcode: &str) -> CheckEvent {
    let items: Vec<Value> = SERVERS
        .iter()
        .map(|ns| {
            json!({
                "query": { "name": DOMAIN, "record_type": record_type },
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
fn an_absent_https_svcb_set_warns_and_nothing_else_changes() {
    const MIXED: &str = "HTTPS records mix alias (priority 0) and service modes: example.com.";
    // (case, events, check id, expected status, expected findings)
    let rows: Vec<(&str, Vec<CheckEvent>, &str, Status, Vec<&str>)> = vec![
        (
            "C5 NODATA batches + NotFound",
            vec![
                batch("HTTPS", "NOERROR"),
                batch("SVCB", "NOERROR"),
                lint("https_svcb", json!({ "NotFound": [] })),
            ],
            "dns.https_svcb",
            Status::Warn,
            vec!["Not found"],
        ),
        (
            "C6 mixed modes",
            vec![
                batch("HTTPS", "NOERROR"),
                batch("SVCB", "NOERROR"),
                lint("https_svcb", json!({ "Warning": MIXED })),
            ],
            "dns.https_svcb",
            Status::Warn,
            vec![MIXED],
        ),
        (
            "C7 well-formed",
            vec![
                batch("HTTPS", "NOERROR"),
                batch("SVCB", "NOERROR"),
                lint(
                    "https_svcb",
                    json!({ "Ok": "HTTPS records are well-formed" }),
                ),
            ],
            "dns.https_svcb",
            Status::Pass,
            vec![],
        ),
        (
            "C8 failed HTTPS batch",
            vec![
                batch("HTTPS", "SERVFAIL"),
                batch("SVCB", "NOERROR"),
                lint("https_svcb", json!({ "NotFound": [] })),
            ],
            "dns.https_svcb",
            Status::Unmeasured,
            vec![],
        ),
        (
            "C9 caa NotFound stays fail",
            vec![
                batch("CAA", "NOERROR"),
                lint("caa", json!({ "NotFound": [] })),
            ],
            "dns.caa",
            Status::Fail,
            vec![],
        ),
    ];

    for (case, events, id, want, findings) in rows {
        let SectionOutcome::Measured { checks, .. } = translate(&events) else {
            panic!("{case}: expected Measured");
        };
        let check = checks
            .iter()
            .find(|c| c.id.to_string() == id)
            .unwrap_or_else(|| panic!("{case}: {id} missing"));
        assert_eq!(check.status, want, "{case}: {id}: {:?}", check.findings);
        if !findings.is_empty() {
            assert_eq!(check.findings, findings, "{case}: {id} findings");
        }
    }
}
