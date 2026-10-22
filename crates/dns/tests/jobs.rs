// V2 Phase 7, requirements 30 and 34: the DNS jobs called directly, no axum type, over
// `testing::stub_jobs()` (offline: fixed zone for example.com, raw outbound stub).

use std::collections::BTreeSet;
use std::net::IpAddr;

use futures::StreamExt;
use netray_dns::config::{Config, ModuleConfig};
use netray_dns::error::ApiError;
use netray_dns::security::RateLimitState;
use netray_dns::{
    DnsAuthCompareEvent, DnsCompareEvent, DnsDnssecEvent, DnsJob, DnsJobEvent, DnsJobs, DnsModule,
    DnsQueryEvent, DnsRefusal, DnsTraceEvent, DnssecRequest, PostQueryRequest, TraceRequest,
    testing,
};
use serde_json::{Value, json};

fn client() -> IpAddr {
    "203.0.113.7".parse().unwrap()
}

fn limiter() -> RateLimitState {
    RateLimitState::new(&Config::load(None).expect("default config").limits)
}

fn post(v: Value) -> PostQueryRequest {
    serde_json::from_value(v).expect("request parses")
}

fn trace_req(v: Value) -> TraceRequest {
    serde_json::from_value(v).expect("trace request parses")
}

fn dnssec_req(v: Value) -> DnssecRequest {
    serde_json::from_value(v).expect("dnssec request parses")
}

async fn drain<E: DnsJobEvent>(mut job: DnsJob<E>) -> Vec<E> {
    let mut out = Vec::new();
    while let Some(e) = job.next().await {
        out.push(e);
    }
    out
}

fn js<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap()
}

fn sorted(mut v: Vec<Value>) -> Vec<String> {
    let mut out: Vec<String> = v.drain(..).map(|x| x.to_string()).collect();
    out.sort();
    out
}

fn arbitrary_jobs() -> DnsJobs {
    let mut cfg = Config::load(None).expect("default config");
    cfg.dns.allow_arbitrary_servers = true;
    testing::stub_jobs_with(cfg)
}

#[tokio::test]
async fn query_example_com_a_yields_one_batch_and_done() {
    let jobs = testing::stub_jobs();
    let req = post(json!({"domain": "example.com", "record_types": ["A"]}));
    let events: Vec<DnsQueryEvent> = drain(
        jobs.query(&req, client(), &limiter(), "rid".into())
            .await
            .expect("job starts"),
    )
    .await;
    let batches: Vec<&DnsQueryEvent> = events
        .iter()
        .filter(|e| matches!(e, DnsQueryEvent::Batch(_)))
        .collect();
    assert_eq!(batches.len(), 1, "one batch for one record type");
    assert!(js(batches[0]).to_string().contains("192.0.2.10"));
    let done = events
        .iter()
        .find_map(|e| match e {
            DnsQueryEvent::Done(d) => Some(js(d)),
            _ => None,
        })
        .expect("done event");
    assert_eq!(done["total_queries"], 1);
}

#[tokio::test]
async fn compare_yields_one_batch_per_available_transport() {
    let jobs = testing::stub_jobs();
    let req = post(json!({"domain": "example.com", "record_types": ["A"]}));
    let events = drain(
        jobs.compare(&req, client(), &limiter(), "rid".into())
            .await
            .expect("job starts"),
    )
    .await;
    let transports: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            DnsCompareEvent::Batch(b) => b.transport.clone(),
            _ => None,
        })
        .collect();
    let done = events
        .iter()
        .find_map(|e| match e {
            DnsCompareEvent::Done(d) => Some(js(d)),
            _ => None,
        })
        .expect("done event");
    let available: Vec<String> = done["transports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_string())
        .collect();
    assert!(!available.is_empty());
    assert_eq!(transports.len(), available.len());
    assert_eq!(
        transports.iter().collect::<BTreeSet<_>>(),
        available.iter().collect::<BTreeSet<_>>()
    );
}

#[tokio::test]
async fn authcompare_yields_recursive_and_authoritative_batches() {
    let jobs = testing::stub_jobs();
    let req = post(json!({"domain": "example.com", "record_types": ["A"]}));
    let events = drain(
        jobs.authcompare(&req, client(), &limiter(), "rid".into())
            .await
            .expect("job starts"),
    )
    .await;
    let sources: BTreeSet<String> = events
        .iter()
        .filter_map(|e| match e {
            DnsAuthCompareEvent::Batch(b) => b.source.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(
        sources,
        BTreeSet::from(["authoritative".to_string(), "recursive".to_string()])
    );
    let done = events
        .iter()
        .find_map(|e| match e {
            DnsAuthCompareEvent::Done(d) => Some(js(d)),
            _ => None,
        })
        .expect("done event");
    assert!(
        done["auth_servers"]
            .to_string()
            .contains("ns1.example.com."),
        "auth_servers: {}",
        done["auth_servers"]
    );
}

#[tokio::test]
async fn trace_yields_hops_and_done_counts_them() {
    let jobs = testing::stub_jobs();
    let req = trace_req(json!({"domain": "example.com"}));
    let events = drain(
        jobs.trace(&req, client(), &limiter(), "rid".into())
            .await
            .expect("job starts"),
    )
    .await;
    let hops = events
        .iter()
        .filter(|e| matches!(e, DnsTraceEvent::Hop(_)))
        .count();
    assert!(hops >= 1);
    let done = events
        .iter()
        .find_map(|e| match e {
            DnsTraceEvent::Done(d) => Some(js(d)),
            _ => None,
        })
        .expect("done event");
    assert_eq!(done["hops"], hops);
}

#[tokio::test]
async fn dnssec_yields_at_least_one_level() {
    let jobs = testing::stub_jobs();
    let req = dnssec_req(json!({"domain": "example.com"}));
    let events = drain(
        jobs.dnssec(&req, client(), &limiter(), "rid".into())
            .await
            .expect("job starts"),
    )
    .await;
    let levels = events
        .iter()
        .filter(|e| matches!(e, DnsDnssecEvent::Chain(_)))
        .count();
    assert!(levels >= 1);
}

#[tokio::test]
async fn collect_equals_the_streamed_events() {
    let jobs = testing::stub_jobs();
    let t = trace_req(json!({"domain": "example.com"}));
    let streamed = drain(
        jobs.trace(&t, client(), &limiter(), "rid".into())
            .await
            .unwrap(),
    )
    .await;
    let collected = jobs
        .trace(&t, client(), &limiter(), "rid".into())
        .await
        .unwrap()
        .collect()
        .await;
    let hops: Vec<Value> = streamed
        .iter()
        .filter_map(|e| match e {
            DnsTraceEvent::Hop(h) => Some(js(h)),
            _ => None,
        })
        .collect();
    let got = js(&collected);
    assert_eq!(got["hops"], Value::Array(hops));
    assert_eq!(got["done"]["hops"], got["hops"].as_array().unwrap().len());

    let q = post(json!({"domain": "example.com", "record_types": ["A"]}));
    let streamed = drain(
        jobs.query(&q, client(), &limiter(), "rid".into())
            .await
            .unwrap(),
    )
    .await;
    let collected = jobs
        .query(&q, client(), &limiter(), "rid".into())
        .await
        .unwrap()
        .collect()
        .await;
    let batches: Vec<Value> = streamed
        .iter()
        .filter_map(|e| match e {
            DnsQueryEvent::Batch(b) => Some(js(b)),
            _ => None,
        })
        .collect();
    let got = js(&collected);
    assert_eq!(
        sorted(got["batches"].as_array().unwrap().clone()),
        sorted(batches)
    );
    assert_eq!(got["done"]["total_queries"], 1);
}

#[tokio::test]
async fn invalid_record_type_is_refused() {
    let jobs = testing::stub_jobs();
    let req = post(json!({"domain": "example.com", "record_types": ["NOPE"]}));
    let err = jobs
        .query(&req, client(), &limiter(), "rid".into())
        .await
        .err()
        .expect("refused");
    assert!(matches!(err, ApiError::InvalidRecordType(_)), "{err:?}");
    assert_eq!(
        DnsRefusal::from(err),
        DnsRefusal::InvalidInput("invalid record type: NOPE".into())
    );
}

#[tokio::test]
async fn arbitrary_server_is_refused_by_default() {
    let jobs = testing::stub_jobs();
    let req = post(json!({"domain": "example.com", "servers": ["192.0.2.1"]}));
    let err = jobs
        .query(&req, client(), &limiter(), "rid".into())
        .await
        .err()
        .expect("refused");
    assert!(matches!(err, ApiError::ArbitraryServersDisabled), "{err:?}");
}

#[tokio::test]
async fn private_server_is_blocked_when_arbitrary_servers_are_allowed() {
    let jobs = arbitrary_jobs();
    let req = post(json!({"domain": "example.com", "servers": ["10.0.0.1"]}));
    let err = jobs
        .query(&req, client(), &limiter(), "rid".into())
        .await
        .err()
        .expect("refused");
    assert!(matches!(err, ApiError::BlockedTargetIp { .. }), "{err:?}");
    assert!(matches!(DnsRefusal::from(err), DnsRefusal::Blocked(_)));
}

#[tokio::test]
async fn trace_axfr_is_refused() {
    let jobs = testing::stub_jobs();
    let req = trace_req(json!({"domain": "example.com", "record_type": "AXFR"}));
    let err = jobs
        .trace(&req, client(), &limiter(), "rid".into())
        .await
        .err()
        .expect("refused");
    assert!(matches!(err, ApiError::InvalidRecordType(_)), "{err:?}");
}

#[tokio::test]
async fn spent_limiter_burst_is_rate_limited() {
    let jobs = testing::stub_jobs();
    let mut limits = Config::load(None).expect("default config").limits;
    limits.per_ip_burst = 16;
    limits.per_ip_per_minute = 16;
    let limiter = RateLimitState::new(&limits);
    let req = trace_req(json!({"domain": "example.com"}));
    let first = jobs
        .trace(&req, client(), &limiter, "rid".into())
        .await
        .expect("first job starts");
    first.collect().await;
    let err = jobs
        .trace(&req, client(), &limiter, "rid".into())
        .await
        .err()
        .expect("second refused");
    assert!(matches!(err, ApiError::RateLimited { .. }), "{err:?}");
    assert!(matches!(
        DnsRefusal::from(err),
        DnsRefusal::RateLimited { .. }
    ));
}

#[tokio::test]
async fn module_builds_its_jobs() {
    let cfg: ModuleConfig = serde_json::from_str("{}").unwrap();
    let module = DnsModule::new(cfg, None).await.expect("module builds");
    let _jobs: DnsJobs = module.jobs();
}

// spec v2-input-limits, Phase 2 (R4, R5): `+dnssec` adds DNSKEY and DS before the record-type
// limit and the cost, so ten types plus `+dnssec` are refused, not run as twelve.

const TEN_TYPES: [&str; 10] = [
    "A", "AAAA", "MX", "TXT", "NS", "SOA", "CAA", "CNAME", "PTR", "SRV",
];

fn assert_too_many_12<T>(r: Result<T, ApiError>, what: &str) {
    match r {
        Err(ApiError::TooManyRecordTypes { requested, max }) => {
            assert_eq!((requested, max), (12, 10), "{what}");
        }
        Err(e) => panic!("{what}: expected TooManyRecordTypes, got {e:?}"),
        Ok(_) => panic!("{what}: expected TooManyRecordTypes, the job started"),
    }
}

#[tokio::test]
async fn input_limits_ten_types_plus_dnssec_query_is_refused() {
    let jobs = testing::stub_jobs();
    let req = post(json!({"domain": "example.com", "record_types": TEN_TYPES, "dnssec": true}));
    assert_too_many_12(
        jobs.query(&req, client(), &limiter(), "rid".into()).await,
        "query",
    );
}

#[tokio::test]
async fn input_limits_ten_types_plus_dnssec_compare_is_refused() {
    let jobs = testing::stub_jobs();
    let req = post(json!({"domain": "example.com", "record_types": TEN_TYPES, "dnssec": true}));
    assert_too_many_12(
        jobs.compare(&req, client(), &limiter(), "rid".into()).await,
        "compare",
    );
}

#[tokio::test]
async fn input_limits_ten_types_plus_dnssec_query_language_is_refused() {
    let jobs = testing::stub_jobs();
    let parsed =
        netray_dns::parser::parse("example.com A AAAA MX TXT NS SOA CAA CNAME PTR SRV +dnssec")
            .expect("query parses");
    assert_too_many_12(
        jobs.query_parsed(parsed, client(), &limiter(), "rid".into())
            .await,
        "query_parsed",
    );
}

#[tokio::test]
async fn input_limits_eight_types_plus_dnssec_run_ten_batches() {
    let jobs = testing::stub_jobs();
    let req =
        post(json!({"domain": "example.com", "record_types": &TEN_TYPES[..8], "dnssec": true}));
    let events = drain(
        jobs.query(&req, client(), &limiter(), "rid".into())
            .await
            .expect("eight types plus DNSKEY and DS start"),
    )
    .await;
    let batches = events
        .iter()
        .filter(|e| matches!(e, DnsQueryEvent::Batch(_)))
        .count();
    assert_eq!(batches, 10, "one batch per type, DNSKEY and DS included");
}

#[tokio::test]
async fn input_limits_dnssec_types_already_requested_are_not_counted_twice() {
    let jobs = testing::stub_jobs();
    let mut types = TEN_TYPES[..8].to_vec();
    types.extend(["DNSKEY", "DS"]);
    let req = post(json!({"domain": "example.com", "record_types": types, "dnssec": true}));
    assert!(
        jobs.query(&req, client(), &limiter(), "rid".into())
            .await
            .is_ok(),
        "ten types including DNSKEY and DS start"
    );
}

// spec v2-input-edges, Phase 2 (R6): a cost above a burst can never be admitted by waiting, so it
// is refused with `CostExceedsLimit` before any limiter is charged.

#[tokio::test]
async fn cost_limit_c5_query_above_global_burst_is_refused_and_charges_nothing() {
    let jobs = arbitrary_jobs();
    let mut limits = Config::load(None).expect("default config").limits;
    limits.global_burst = 20;
    let limiter = RateLimitState::new(&limits);
    let req = post(json!({
        "domain": "example.com",
        "record_types": &TEN_TYPES[..8],
        "dnssec": true,
        "servers": ["cloudflare", "google", "quad9"],
    }));
    match jobs.query(&req, client(), &limiter, "rid".into()).await {
        Err(ApiError::CostExceedsLimit { cost, limit }) => assert_eq!((cost, limit), (30, 20)),
        Err(e) => panic!("expected CostExceedsLimit, got {e:?}"),
        Ok(_) => panic!("expected CostExceedsLimit, the job started"),
    }
    assert!(
        limiter.check_query_cost(client(), &[], 20, 1).is_ok(),
        "the refused request charged nothing: a cost of 20 is still admitted"
    );
}

#[tokio::test]
async fn cost_limit_c6_compare_above_per_target_burst_is_refused() {
    let jobs = testing::stub_jobs();
    let mut limits = Config::load(None).expect("default config").limits;
    limits.per_target_burst = 5;
    let limiter = RateLimitState::new(&limits);
    let req = post(json!({
        "domain": "example.com",
        "record_types": ["A", "AAAA", "MX", "TXT"],
        "dnssec": true,
    }));
    match jobs.compare(&req, client(), &limiter, "rid".into()).await {
        Err(ApiError::CostExceedsLimit { cost, limit }) => assert_eq!((cost, limit), (6, 5)),
        Err(e) => panic!("expected CostExceedsLimit, got {e:?}"),
        Ok(_) => panic!("expected CostExceedsLimit, the job started"),
    }
}

#[test]
fn cost_limit_cost_exceeds_limit_maps_to_invalid_input() {
    assert!(matches!(
        DnsRefusal::from(ApiError::CostExceedsLimit {
            cost: 30,
            limit: 20
        }),
        DnsRefusal::InvalidInput(_)
    ));
}
