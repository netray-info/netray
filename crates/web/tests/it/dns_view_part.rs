//! v2-dns-ui Phase 1 (C2, C9, C18, C19, C20): the DNS part view claims dns.answers, dns.servers,
//! dns.warnings and every job block it fully shows, loses no record value, and renders for a
//! cached result opened by key.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_dns::config::Config;
use netray_dns::security::RateLimitState;
use netray_dns::{PostQueryRequest, testing};
use netray_model::evidence::{Block, BlockId, Node, Record, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
    router,
};
use netray_web::evidence::display_value;
use netray_web::links::decorate;
use netray_web::pages::views::{PartView, ViewContext, render_part};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use serde_json::{Value as Json, json};
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _: &'a str,
        _: IpAddr,
        _: RunTrigger,
        _: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn api_state() -> ApiState {
    let config: ApiConfig =
        toml::from_str("[dns]\nper_ip_burst = 200\nglobal_burst = 200").expect("[api.dns] parses");
    ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        dns: Some(testing::stub_jobs()),
        ..Default::default()
    })
}

fn shell_over(state: &ApiState) -> Router {
    inspect_router(
        InspectDeps::new(
            Arc::new(InspectJobs::new(state.clone())),
            InspectConfig::default(),
        )
        .with_dns_results(state.dns_results().clone()),
    )
}

fn with_peer(mut req: Request<Body>) -> Request<Body> {
    req.extensions_mut()
        .insert(ConnectInfo("198.51.100.1:0".parse::<std::net::SocketAddr>().unwrap()));
    req
}

async fn body_of(app: &Router, req: Request<Body>) -> (StatusCode, String) {
    let resp = app.clone().oneshot(with_peer(req)).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        String::from_utf8_lossy(&bytes)
            .replace("\r\n", "\n")
            .replace("\\\"", "\""),
    )
}

fn client() -> IpAddr {
    "203.0.113.7".parse().unwrap()
}

fn limiter() -> RateLimitState {
    RateLimitState::new(&Config::load(None).expect("default config").limits)
}

fn escaped(text: &str) -> String {
    // askama 0.16's HTML escaping.
    text.replace('&', "&#38;")
        .replace('<', "&#60;")
        .replace('>', "&#62;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

fn part_of(blocks: &[Block]) -> Option<PartView> {
    let statuses: Vec<(CheckId, Status)> = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render_part(blocks, &ctx)
}

fn claimed_ids(part: &PartView) -> Vec<String> {
    part.claimed.iter().map(|b| b.to_string()).collect()
}

async fn query_blocks_of(q: &str) -> Vec<Block> {
    let parsed = netray_dns::parser::parse(q).expect("query parses");
    let result = testing::stub_jobs()
        .query_parsed(parsed, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    netray_dns::query_page_blocks(&result)
}

async fn compare_blocks_of() -> Vec<Block> {
    let req: PostQueryRequest =
        serde_json::from_value(json!({"domain": "example.com", "record_types": ["A"]})).unwrap();
    let result = testing::stub_jobs()
        .compare(&req, client(), &limiter(), "rid".into())
        .await
        .expect("job starts")
        .collect()
        .await;
    netray_dns::compare_page_blocks(&result)
}

#[tokio::test]
async fn c2_the_part_view_claims_answers_servers_and_the_job_blocks_it_shows() {
    let blocks = query_blocks_of("example.com A MX").await;

    let part = part_of(&blocks).expect("dns.answers is present, the part view claims");

    let claimed = claimed_ids(&part);
    for id in ["dns.answers", "dns.servers", "dns.a", "dns.mx"] {
        assert!(claimed.iter().any(|c| c == id), "{id} not in {claimed:?}");
    }
    for id in &claimed {
        assert!(
            part.html.contains(&format!("id=\"ev-{id}\"")),
            "no id=\"ev-{id}\" in {}",
            part.html
        );
    }
}

#[tokio::test]
async fn c9_a_query_streams_a_dns_type_card_with_one_agreeing_group() {
    let shell = shell_over(&api_state());
    let req = Request::post("/inspect/dns/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("q=example.com+A"))
        .unwrap();

    let (status, s) = body_of(&shell, req).await;

    assert_eq!(status, StatusCode::OK, "{s}");
    assert!(s.contains(r#"class="dns-type""#), "{s}");
    assert!(s.contains(r#"data-type="A""#), "{s}");
    assert!(s.contains(r#"dns-type__badge" data-tone="pass">agree<"#), "{s}");
    assert_eq!(s.matches(r#"class="dns-group""#).count(), 1, "{s}");
    assert!(s.contains("192.0.2.10"), "{s}");
    let after = s
        .split(r#"dns-group__count">"#)
        .nth(1)
        .unwrap_or_else(|| panic!("no dns-group__count in {s}"));
    let count = &after[..after.find('<').unwrap()];
    let (m, rest) = count.split_once(" / ").unwrap_or_else(|| panic!("{count:?}"));
    let n = rest.strip_suffix(" servers").unwrap_or_else(|| panic!("{count:?}"));
    assert!(!m.is_empty() && m.bytes().all(|b| b.is_ascii_digit()), "{count:?}");
    assert_eq!(m, n, "{count:?}");
    assert!(!s.contains(r#"id="ev-dns.a" data-shape="#), "{s}");
}

fn txt(s: &str) -> Value {
    Value::Text(s.into())
}

fn field(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

fn record(data: &str) -> Record {
    Record {
        owner: txt("example.com"),
        rtype: txt("A"),
        ttl: Value::Int(300),
        data: txt(data),
    }
}

fn block(id: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: id.into(),
        shape,
    }
}

#[test]
fn c18_a_records_block_with_data_no_answer_holds_is_not_claimed() {
    let answer = Node {
        fields: vec![
            field("name", txt("example.com")),
            field("type", txt("A")),
            field("ttl", Value::Int(300)),
            field("data", txt("192.0.2.10")),
        ],
        children: vec![],
    };
    let server = Node {
        fields: vec![
            field("server", txt("udp:192.0.2.53:53")),
            field("address", txt("192.0.2.53:53")),
            field("transport", txt("udp")),
            field("outcome", txt("answered")),
        ],
        children: vec![answer],
    };
    let a_type = Node {
        fields: vec![field("record_type", txt("A")), field("agreement", txt("single"))],
        children: vec![server],
    };
    let root = Node {
        fields: vec![
            field("mode", txt("query")),
            field("finished", Value::Bool(true)),
            field("record_types", Value::Int(1)),
            field("lookups", Value::Int(1)),
            field("servers", Value::Int(1)),
            field("agree", Value::Int(0)),
            field("diverge", Value::Int(0)),
            field("single", Value::Int(1)),
            field("unmeasured", Value::Int(0)),
        ],
        children: vec![a_type],
    };
    let blocks = vec![
        block("dns.answers", Shape::Tree(root)),
        block("dns.a", Shape::Records(vec![record("198.51.100.99")])),
    ];

    let part = part_of(&blocks).expect("dns.answers is present, the part view claims");

    let claimed = claimed_ids(&part);
    assert!(claimed.iter().any(|c| c == "dns.answers"), "{claimed:?}");
    assert!(!claimed.iter().any(|c| c == "dns.a"), "{claimed:?}");
}

/// Every `data` text of the `dns.answers` tree and of each claimed records block.
fn data_values(blocks: &[Block], claimed: &[String]) -> Vec<String> {
    fn walk(node: &Node, out: &mut Vec<String>) {
        for (k, v) in &node.fields {
            if k == "data" {
                out.push(display_value(v));
            }
        }
        for c in &node.children {
            walk(c, out);
        }
    }
    let mut out = vec![];
    for b in blocks {
        let id = b.id.to_string();
        if !claimed.contains(&id) {
            continue;
        }
        match &b.shape {
            Shape::Records(rs) => out.extend(rs.iter().map(|r| display_value(&r.data))),
            Shape::Tree(n) if id == "dns.answers" => walk(n, &mut out),
            _ => {}
        }
    }
    out
}

#[tokio::test]
async fn c19_the_part_view_loses_no_record_data_of_a_claimed_block() {
    for (label, blocks) in [
        ("query", query_blocks_of("example.com A MX TXT").await),
        ("compare", compare_blocks_of().await),
    ] {
        let part = part_of(&blocks).unwrap_or_else(|| panic!("{label}: not claimed"));

        let values = data_values(&blocks, &claimed_ids(&part));

        assert!(!values.is_empty(), "{label}: no record data in the blocks");
        for v in values {
            assert!(
                part.html.contains(&escaped(&v)),
                "{label}: {v:?} missing in: {}",
                part.html
            );
        }
    }
}

#[tokio::test]
async fn c20_a_cached_dns_result_opens_as_the_dns_view_with_a_results_tab() {
    let state = api_state();
    let req = Request::post("/dns/query")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"domain": "example.com", "record_types": ["A"]}).to_string(),
        ))
        .unwrap();
    let (status, body) = body_of(&router(state.clone()), req).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let key = serde_json::from_str::<Json>(&body).unwrap()["done"]["cache_key"]
        .as_str()
        .unwrap_or_else(|| panic!("done has no cache_key: {body}"))
        .to_owned();

    let (status, page) = body_of(
        &shell_over(&state),
        Request::get(format!("/inspect/dns?r={key}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains(r#"class="dns-view""#), "{page}");
    assert!(page.contains(r#"data-tab="results""#), "{page}");
}
