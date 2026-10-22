//! v2-correctness Phase 4 (R8, R9): a DNS inspector query for an IP address looks up PTR at
//! its reverse name, and a job with `@system` answers instead of ending empty. Offline, over
//! `testing::stub_jobs()`.

use std::net::IpAddr;

use netray_dns::config::Config;
use netray_dns::security::RateLimitState;
use netray_dns::{DnsQueryResult, testing};
use serde_json::Value;

fn client() -> IpAddr {
    "203.0.113.7".parse().unwrap()
}

fn limiter() -> RateLimitState {
    RateLimitState::new(&Config::default().limits)
}

async fn run(query: &str, allow_system_resolvers: bool) -> DnsQueryResult {
    let mut config = Config::default();
    config.dns.allow_system_resolvers = allow_system_resolvers;
    let parsed = netray_dns::parser::parse(query).expect("query parses");
    testing::stub_jobs_with(config)
        .query_parsed(parsed, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await
}

/// Every `query.name` and `query.record_type` the batches' lookups carry.
fn lookup_queries(result: &DnsQueryResult) -> Vec<(String, String)> {
    fn walk(v: &Value, out: &mut Vec<(String, String)>) {
        match v {
            Value::Object(map) => {
                if let Some(Value::Object(q)) = map.get("query")
                    && let (Some(Value::String(n)), Some(Value::String(t))) =
                        (q.get("name"), q.get("record_type"))
                {
                    out.push((n.clone(), t.clone()));
                }
                map.values().for_each(|v| walk(v, out));
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&serde_json::to_value(&result.batches).unwrap(), &mut out);
    out
}

fn assert_all_looked_up(result: &DnsQueryResult, name: &str, record_type: &str) {
    let queries = lookup_queries(result);
    assert!(!queries.is_empty(), "no lookups in {:?}", result.batches);
    for (n, t) in queries {
        assert_eq!(t, record_type);
        assert_eq!(n.trim_end_matches('.'), name.trim_end_matches('.'));
    }
}

// C1, C3 (R8)
#[tokio::test]
async fn an_ipv4_query_looks_up_ptr_at_its_in_addr_arpa_name() {
    let result = run("192.0.2.1", false).await;
    assert_all_looked_up(&result, "1.2.0.192.in-addr.arpa", "PTR");
}

// C1, C4 (R8)
#[tokio::test]
async fn an_ipv6_query_looks_up_ptr_at_its_ip6_arpa_nibble_name() {
    let result = run("2001:db8::1", false).await;
    assert_all_looked_up(
        &result,
        "1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.b.d.0.1.0.0.2.ip6.arpa",
        "PTR",
    );
}

// C5 (R8)
#[tokio::test]
async fn a_name_query_with_ptr_keeps_its_lookup_name() {
    let result = run("example.com PTR", false).await;
    assert_all_looked_up(&result, "example.com", "PTR");
}

// C2, C7 (R9): on a machine with two or more system nameservers the job panics today and ends
// without `done`; with one system nameserver this holds already.
#[tokio::test]
async fn a_system_query_sends_batches_and_done() {
    let result = run("example.com A @system", true).await;
    assert_eq!(result.batches.len(), 1, "errors: {:?}", result.errors);
    assert!(result.done.is_some(), "no done event: {:?}", result.errors);
}
