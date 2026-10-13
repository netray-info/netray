// `facts_from_lookups` sets `Facts::no_address_definite` only when the A lookup and the AAAA
// lookup of the queried name both got a definite negative (NXDOMAIN, or NODATA that is not a
// referral) from every resolver asked. Spec: specs/features/v2-unmeasured/spec.md, requirement 4.

use mhost::resolver::Lookups;
use netray_dns::facts_from_lookups;
use serde_json::{Value, json};

const NAME: &str = "example.com.";

fn resp_time() -> Value {
    json!({ "secs": 0, "nanos": 12_000_000 })
}

fn nodata() -> Value {
    json!({ "NxDomain": { "response_time": resp_time(), "response_code": "NOERROR" } })
}

fn nxdomain() -> Value {
    json!({ "NxDomain": { "response_time": resp_time(), "response_code": "NXDOMAIN" } })
}

fn referral() -> Value {
    json!({ "NxDomain": {
        "response_time": resp_time(), "response_code": "NOERROR", "referral": true
    }})
}

fn servfail() -> Value {
    json!({ "NxDomain": { "response_time": resp_time(), "response_code": "SERVFAIL" } })
}

fn timeout() -> Value {
    json!({ "Error": "Timeout" })
}

fn address(record_type: &str, ip: &str) -> Value {
    json!({ "Response": {
        "records": [{ "data": { record_type: ip }, "name": NAME, "ttl": 300, "type": record_type }],
        "response_time": resp_time(),
        "valid_until": "2030-01-01T00:00:00Z"
    }})
}

/// One lookup per `(resolver, result)` pair, all for `record_type` of the queried name.
fn lookups(record_type: &str, results: &[(&str, Value)]) -> Lookups {
    let entries: Vec<Value> = results
        .iter()
        .map(|(server, result)| {
            json!({
                "query": { "name": NAME, "record_type": record_type },
                "name_server": format!("udp:{server}:53"),
                "result": result
            })
        })
        .collect();
    serde_json::from_value(json!({ "lookups": entries }))
        .expect("fixture must deserialize into mhost Lookups")
}

const R1: &str = "192.0.2.53";
const R2: &str = "192.0.2.54";

#[test]
fn no_address_definite_follows_the_negatives_of_both_address_lookups() {
    struct Row {
        name: &'static str,
        a: Lookups,
        aaaa: Lookups,
        definite: bool,
        has_a: bool,
    }
    let rows = [
        Row {
            name: "A NODATA + AAAA NODATA on every resolver",
            a: lookups("A", &[(R1, nodata()), (R2, nodata())]),
            aaaa: lookups("AAAA", &[(R1, nodata()), (R2, nodata())]),
            definite: true,
            has_a: false,
        },
        Row {
            name: "A NXDOMAIN + AAAA NXDOMAIN",
            a: lookups("A", &[(R1, nxdomain())]),
            aaaa: lookups("AAAA", &[(R1, nxdomain())]),
            definite: true,
            has_a: false,
        },
        Row {
            name: "A NODATA + AAAA SERVFAIL",
            a: lookups("A", &[(R1, nodata())]),
            aaaa: lookups("AAAA", &[(R1, servfail())]),
            definite: false,
            has_a: false,
        },
        Row {
            name: "A NXDOMAIN + AAAA timeout",
            a: lookups("A", &[(R1, nxdomain())]),
            aaaa: lookups("AAAA", &[(R1, timeout())]),
            definite: false,
            has_a: false,
        },
        Row {
            name: "A NODATA from one resolver, a referral from another",
            a: lookups("A", &[(R1, nodata()), (R2, referral())]),
            aaaa: lookups("AAAA", &[(R1, nodata()), (R2, nodata())]),
            definite: false,
            has_a: false,
        },
        Row {
            name: "A has an address",
            a: lookups("A", &[(R1, address("A", "192.0.2.10"))]),
            aaaa: lookups("AAAA", &[(R1, nodata())]),
            definite: false,
            has_a: true,
        },
    ];

    for row in rows {
        let facts = facts_from_lookups(&row.a.merge(row.aaaa));
        assert_eq!(facts.no_address_definite, row.definite, "{}", row.name);
        assert_eq!(!facts.a.is_empty(), row.has_a, "{}: a", row.name);
        assert!(facts.aaaa.is_empty(), "{}: aaaa", row.name);
    }
}
