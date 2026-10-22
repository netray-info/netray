// `facts_from_lookups` projects the shared lookups into the DNS-owned records blocks `dns.a`,
// `dns.aaaa`, `dns.mx`, `dns.caa`, `dns.ns` and `dns.https`. A lookup that answered (a response
// or a definite NXDOMAIN/NODATA) gives a block, possibly with zero records; a type whose lookups
// all errored gives none (Rule M). Spec: specs/features/v2-evidence/spec.md, requirement 3a.

use mhost::resolver::Lookups;
use netray_dns::facts_from_lookups;
use netray_dns::module::REGISTER;
use netray_model::evidence::{Block, BlockId, Shape, Value as Ev, validate};
use serde_json::{Value, json};

const NAME: &str = "example.com.";
const R1: &str = "192.0.2.53";
const R2: &str = "192.0.2.54";

fn resp_time() -> Value {
    json!({ "secs": 0, "nanos": 12_000_000 })
}

fn nodata() -> Value {
    json!({ "NxDomain": { "response_time": resp_time(), "response_code": "NOERROR" } })
}

fn servfail() -> Value {
    json!({ "NxDomain": { "response_time": resp_time(), "response_code": "SERVFAIL" } })
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

fn block<'a>(blocks: &'a [Block], id: &str) -> Option<&'a Block> {
    let id = BlockId::parse(id).unwrap();
    blocks.iter().find(|b| b.id == id)
}

#[test]
fn facts_blocks_follow_the_answered_lookups() {
    let a = lookups("A", &[(R1, address("A", "192.0.2.10"))]);
    let caa_servfail = lookups("CAA", &[(R1, servfail()), (R2, servfail())]);
    let caa_nodata = lookups("CAA", &[(R1, nodata()), (R2, nodata())]);

    // (name, lookups, whether a `dns.caa` block is expected)
    let cases = [
        (
            "CAA SERVFAIL on every resolver",
            a.clone().merge(caa_servfail),
            false,
        ),
        ("CAA definite NODATA", a.merge(caa_nodata), true),
    ];

    for (name, lookups, caa_expected) in cases {
        let blocks = facts_from_lookups(&lookups).blocks;

        let dns_a = block(&blocks, "dns.a").unwrap_or_else(|| panic!("{name}: no dns.a block"));
        let Shape::Records(records) = &dns_a.shape else {
            panic!("{name}: dns.a is not a records block");
        };
        assert_eq!(records.len(), 1, "{name}: dns.a records");
        assert_eq!(records[0].ttl, Ev::Int(300), "{name}: ttl");
        assert_eq!(
            records[0].data,
            Ev::Text("192.0.2.10".into()),
            "{name}: data"
        );

        match (block(&blocks, "dns.caa"), caa_expected) {
            (None, false) => {}
            (Some(b), true) => {
                assert_eq!(
                    b.shape,
                    Shape::Records(vec![]),
                    "{name}: dns.caa holds no records"
                );
            }
            (got, _) => panic!("{name}: dns.caa present = {}", got.is_some()),
        }

        validate("dns", &REGISTER, &blocks)
            .unwrap_or_else(|e| panic!("{name}: blocks must validate: {e:?}"));
    }
}
