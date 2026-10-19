// v2-dns-api Phase 1: the per-job summary (`jobs::summary`) maps mhost lookups onto per-server
// answers, compares them per record type and condenses a job. Pure functions over hand-built
// `DnsBatch` values; no resolver, no network.

use netray_dns::DnsBatch;
use netray_dns::jobs::events::BatchLookups;
use netray_dns::jobs::summary::{
    Agreement, AnswerOutcome, DnsRecordDetail, agreement, median_ms, soa_serial_age_days, summarise,
};
use serde_json::{Value, json};

const G1: &str = "udp:8.8.8.8:53,name=Google 1";
const G2: &str = "udp:8.8.4.4:53,name=Google 2";

fn a_record(data: &str, ttl: u32) -> Value {
    json!({"name": "example.com", "type": "A", "ttl": ttl, "data": {"A": data}})
}

fn response(records: Vec<Value>) -> Value {
    json!({"Response": {
        "records": records,
        "response_time": {"secs": 0, "nanos": 10_000_000},
        "valid_until": "2099-01-01T00:00:00Z"
    }})
}

fn nx(code: &str) -> Value {
    json!({"NxDomain": {
        "response_time": {"secs": 0, "nanos": 10_000_000},
        "response_code": code
    }})
}

fn batch(record_type: &str, rows: Vec<(&str, Value)>) -> DnsBatch {
    let lookups: Vec<Value> = rows
        .into_iter()
        .map(|(server, result)| {
            json!({
                "query": {"name": "example.com", "record_type": record_type},
                "name_server": server,
                "result": result
            })
        })
        .collect();
    let lookups = serde_json::from_value(json!({ "lookups": lookups })).expect("lookups parse");
    DnsBatch {
        request_id: "rid".into(),
        record_type: record_type.into(),
        lookups: BatchLookups::Resolved(lookups),
        completed: 1,
        total: 1,
        transport: None,
        source: None,
    }
}

// C1 (R1)
#[test]
fn an_answered_lookup_maps_to_a_server_answer() {
    let summary = summarise(&[batch(
        "A",
        vec![(G1, response(vec![a_record("192.0.2.10", 300)]))],
    )]);
    let answer = &summary.types[0].answers[0];

    assert_eq!(answer.server, G1);
    assert_eq!(answer.address, "8.8.8.8:53");
    assert_eq!(answer.name.as_deref(), Some("Google 1"));
    assert_eq!(answer.transport, "udp");
    assert_eq!(answer.outcome, AnswerOutcome::Answered);
    assert_eq!(answer.response_time_ms, Some(10.0));
    assert_eq!(
        answer.records,
        vec![DnsRecordDetail {
            name: "example.com".into(),
            record_type: "A".into(),
            ttl: 300,
            data: "192.0.2.10".into(),
            soa_serial_age_days: None,
        }]
    );
}

// C1 (R1): outcome per mhost result
#[test]
fn lookup_results_map_to_outcomes() {
    let cases = [
        ("NOERROR", AnswerOutcome::NoRecords, None),
        ("NXDOMAIN", AnswerOutcome::Nxdomain, None),
        ("SERVFAIL", AnswerOutcome::Unmeasured, Some("SERVFAIL")),
    ];
    for (code, outcome, response_code) in cases {
        let summary = summarise(&[batch("A", vec![(G1, nx(code))])]);
        let answer = &summary.types[0].answers[0];
        assert_eq!(answer.outcome, outcome, "{code}");
        assert_eq!(answer.response_code.as_deref(), response_code, "{code}");
        assert_eq!(
            answer.error.is_some(),
            outcome == AnswerOutcome::Unmeasured,
            "{code}"
        );
    }
}

// C1 (R1): records sorted by data
#[test]
fn records_of_an_answer_are_sorted_by_data() {
    let mx = |pref: u32, host: &str| {
        json!({"name": "example.com", "type": "MX", "ttl": 300,
               "data": {"MX": {"preference": pref, "exchange": host}}})
    };
    let summary = summarise(&[batch(
        "MX",
        vec![(
            G1,
            response(vec![mx(20, "mx2.example.com."), mx(10, "mx1.example.com.")]),
        )],
    )]);
    let data: Vec<&str> = summary.types[0].answers[0]
        .records
        .iter()
        .map(|r| r.data.as_str())
        .collect();
    assert_eq!(data, ["10 mx1.example.com.", "20 mx2.example.com."]);
}

// C3 (R3): comparisons sorted by record type
#[test]
fn comparisons_are_sorted_by_record_type() {
    let summary = summarise(&[
        batch("MX", vec![(G1, nx("NOERROR"))]),
        batch("A", vec![(G1, nx("NOERROR"))]),
        batch("AAAA", vec![(G1, nx("NOERROR"))]),
    ]);
    let types: Vec<&str> = summary
        .types
        .iter()
        .map(|t| t.record_type.as_str())
        .collect();
    assert_eq!(types, ["A", "AAAA", "MX"]);
}

// C3 (R3), C11, C12, C13: agreement per record type
#[test]
fn agreement_per_type() {
    let a10_300 = || response(vec![a_record("192.0.2.10", 300)]);
    let a10_60 = || response(vec![a_record("192.0.2.10", 60)]);
    let a20 = || response(vec![a_record("192.0.2.20", 300)]);
    let cases = [
        (
            "same records agree",
            vec![(G1, a10_300()), (G2, a10_300())],
            Agreement::Agree,
        ),
        (
            "different records diverge",
            vec![(G1, a10_300()), (G2, a20())],
            Agreement::Diverge,
        ),
        (
            "all NODATA agree",
            vec![(G1, nx("NOERROR")), (G2, nx("NOERROR"))],
            Agreement::Agree,
        ),
        (
            "one SERVFAIL is single",
            vec![(G1, a10_300()), (G2, nx("SERVFAIL"))],
            Agreement::Single,
        ),
        (
            "TTL is ignored",
            vec![(G1, a10_300()), (G2, a10_60())],
            Agreement::Agree,
        ),
        (
            "all SERVFAIL unmeasured",
            vec![(G1, nx("SERVFAIL")), (G2, nx("SERVFAIL"))],
            Agreement::Unmeasured,
        ),
    ];
    for (label, rows, expected) in cases {
        let summary = summarise(&[batch("A", rows)]);
        assert_eq!(summary.types[0].agreement, expected, "{label}");
        assert_eq!(agreement(&summary.types[0].answers), expected, "{label}");
    }
}

// C11: every server NODATA
#[test]
fn every_server_nodata_is_no_records() {
    let summary = summarise(&[batch(
        "AAAA",
        vec![(G1, nx("NOERROR")), (G2, nx("NOERROR"))],
    )]);
    let comparison = &summary.types[0];
    assert!(
        comparison
            .answers
            .iter()
            .all(|a| a.outcome == AnswerOutcome::NoRecords)
    );
    assert_eq!(comparison.agreement, Agreement::Agree);
}

// C4 (R4), C14
#[test]
fn median_of_response_times() {
    assert_eq!(median_ms(&[10.0, 30.0, 20.0]), Some(20.0));
    assert_eq!(median_ms(&[10.0, 30.0]), Some(20.0));
    assert_eq!(median_ms(&[]), None);
}

// C4 (R4), C14: a server with only errors has counts and no median
#[test]
fn a_server_with_only_errors_has_no_median() {
    let summary = summarise(&[
        batch("A", vec![(G1, json!({"Error": "Timeout"}))]),
        batch("AAAA", vec![(G1, json!({"Error": "Timeout"}))]),
    ]);
    let server = &summary.servers[0];
    assert_eq!(server.server, G1);
    assert_eq!(
        (server.lookups, server.measured, server.unmeasured),
        (2, 0, 2)
    );
    assert_eq!(server.median_response_time_ms, None);
}

// C2 + C15
#[test]
fn soa_serial_age_in_days() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
    assert_eq!(soa_serial_age_days(2026101501, today), Some(4));
    assert_eq!(soa_serial_age_days(12345, today), None);
    assert_eq!(soa_serial_age_days(2026139901, today), None);
    assert_eq!(soa_serial_age_days(2026023001, today), None);
}

// summarise totals over three types
#[test]
fn a_job_summary_counts_types_and_lookups() {
    let same = || response(vec![a_record("192.0.2.10", 300)]);
    let summary = summarise(&[
        batch("A", vec![(G1, same()), (G2, same())]),
        batch("AAAA", vec![(G1, same()), (G2, nx("SERVFAIL"))]),
        batch("MX", vec![(G1, nx("SERVFAIL")), (G2, nx("SERVFAIL"))]),
    ]);
    assert_eq!(summary.record_types, 3);
    assert_eq!(summary.lookups, 6);
    assert_eq!(
        (
            summary.agree,
            summary.diverge,
            summary.single,
            summary.unmeasured
        ),
        (1, 0, 1, 1)
    );
}

fn answer(
    source: &str,
    record_type: &str,
    data: &str,
) -> netray_dns::jobs::summary::DnsServerAnswer {
    netray_dns::jobs::summary::DnsServerAnswer {
        server: format!("udp:192.0.2.1:53 {source}"),
        address: "192.0.2.1:53".into(),
        name: None,
        transport: "udp".into(),
        source: Some(source.into()),
        outcome: AnswerOutcome::Answered,
        response_code: None,
        error: None,
        response_time_ms: Some(1.0),
        records: vec![DnsRecordDetail {
            name: "example.com".into(),
            record_type: record_type.into(),
            ttl: 300,
            data: data.into(),
            soa_serial_age_days: None,
        }],
    }
}

// Review fix (R3): recursive and authoritative records come from two printers that differ on
// DNSSEC types; across sources such a type compares by record count, not text.
#[test]
fn cross_source_dnssec_types_compare_by_count() {
    let recursive = answer("recursive", "RRSIG", "A 13 2 300 ...");
    let authoritative = answer("authoritative", "RRSIG", "A ECDSAP256SHA256 2 300 ...");
    assert_eq!(
        agreement(&[recursive.clone(), authoritative]),
        Agreement::Agree
    );
    let other = answer("recursive", "RRSIG", "A 8 2 300 ...");
    assert_eq!(agreement(&[recursive, other]), Agreement::Diverge);
    let a1 = answer("recursive", "A", "192.0.2.10");
    let a2 = answer("authoritative", "A", "192.0.2.20");
    assert_eq!(agreement(&[a1, a2]), Agreement::Diverge);
}
