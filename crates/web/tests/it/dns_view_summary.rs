//! v2-dns-ui Phase 1, requirement 3: the DNS part view's summary line and notice lines (C3, C15),
//! and a type whose lookups all failed reads "not measured" with its reason (C11).
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, render_part};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;

const PROXY: &str = "203.0.113.9";
const CALLER: &str = "198.51.100.1";

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn field(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

fn block(id: &str, title: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: title.into(),
        shape,
    }
}

/// A `dns.answers` block: root fields after the fixed counts, one child per (type, agreement).
fn answers(finished: bool, extra: Vec<(String, Value)>, types: &[(&str, &str)]) -> Block {
    let mut fields = vec![
        field("mode", text("query")),
        field("finished", Value::Bool(finished)),
        field("record_types", Value::Int(types.len() as i64)),
        field("lookups", Value::Int(6)),
        field("servers", Value::Int(2)),
        field("agree", Value::Int(2)),
        field("diverge", Value::Int(0)),
        field("single", Value::Int(0)),
        field("unmeasured", Value::Int(0)),
    ];
    fields.extend(extra);
    let children = types
        .iter()
        .map(|(ty, agreement)| Node {
            fields: vec![
                field("record_type", text(ty)),
                field("agreement", text(agreement)),
            ],
            children: vec![],
        })
        .collect();
    block(
        "dns.answers",
        "Answers",
        Shape::Tree(Node { fields, children }),
    )
}

fn render_blocks(blocks: &[Block]) -> (String, Vec<String>) {
    let statuses = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    let part = render_part(blocks, &ctx).expect("the DNS part view claims a dns.answers block");
    let claimed = part.claimed.iter().map(|id| id.to_string()).collect();
    (part.html, claimed)
}

/// The text of the first element opened by `open`, up to its closing tag.
fn element<'a>(html: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = html.find(open)? + open.len();
    let len = html[start..].find(close)?;
    Some(&html[start..start + len])
}

fn summary(html: &str) -> Option<&str> {
    element(html, "<p class=\"dns-view__summary\">", "</p>")
}

#[test]
fn c3_summary_line_lists_counts_transport_and_duration() {
    let extra = vec![
        field("duration", Value::Duration(12)),
        field("total_queries", Value::Int(6)),
        field("transport", text("udp")),
    ];
    let mut b = answers(
        true,
        extra,
        &[("A", "agree"), ("AAAA", "agree"), ("MX", "unmeasured")],
    );
    // One type did not answer: unmeasured 1 (replaces the zero default).
    if let Shape::Tree(node) = &mut b.shape {
        for f in &mut node.fields {
            if f.0 == "unmeasured" {
                f.1 = Value::Int(1);
            }
        }
    }

    let (html, _) = render_blocks(&[b]);

    assert_eq!(
        summary(&html),
        Some(
            "3 types · 6 lookups · 2 servers · udp · 2 agree · 0 diverge · 1 not measured · 12 ms"
        ),
        "{html}"
    );
}

#[test]
fn c3_zero_counts_are_left_out_except_agree_and_diverge() {
    let extra = vec![
        field("duration", Value::Duration(12)),
        field("transport", text("udp")),
    ];
    let b = answers(true, extra, &[("A", "agree"), ("AAAA", "agree")]);

    let (html, _) = render_blocks(&[b]);

    let s = summary(&html).unwrap_or_else(|| panic!("no summary line: {html}"));
    assert_eq!(
        s, "2 types · 6 lookups · 2 servers · udp · 2 agree · 0 diverge · 12 ms",
        "{html}"
    );
    assert!(!s.contains("not measured"), "{s}");
}

#[test]
fn c3_compare_lists_transports_and_authcompare_lists_servers() {
    let compare = answers(
        true,
        vec![
            field("duration", Value::Duration(12)),
            field("transport", text("udp")),
            field("transport", text("tcp")),
        ],
        &[("A", "agree")],
    );
    let auth = answers(
        true,
        vec![
            field("duration", Value::Duration(12)),
            field("auth_server", text("ns1.example.com")),
            field("auth_server", text("ns2.example.com")),
        ],
        &[("A", "agree")],
    );

    let (compare_html, _) = render_blocks(&[compare]);
    let (auth_html, _) = render_blocks(&[auth]);

    let s = summary(&compare_html).unwrap_or_else(|| panic!("no summary: {compare_html}"));
    assert!(s.contains("udp") && s.contains("tcp"), "{s}");
    let s = summary(&auth_html).unwrap_or_else(|| panic!("no summary: {auth_html}"));
    assert!(
        s.contains("ns1.example.com") && s.contains("ns2.example.com"),
        "{s}"
    );
}

#[test]
fn c3_degraded_provider_shows_a_notice() {
    let b = answers(
        true,
        vec![
            field("duration", Value::Duration(12)),
            field("transport", text("udp")),
            field("degraded_provider", text("Example DNS")),
        ],
        &[("A", "agree")],
    );

    let (html, _) = render_blocks(&[b]);

    assert!(
        html.contains("<p class=\"dns-view__notice\">Example DNS skipped: circuit open</p>"),
        "{html}"
    );
}

#[test]
fn c3_an_unfinished_job_shows_a_notice_and_no_summary() {
    let b = answers(false, vec![], &[("A", "agree")]);

    let (html, _) = render_blocks(&[b]);

    assert!(
        html.contains(
            "<p class=\"dns-view__notice\">The query ended before every server answered</p>"
        ),
        "{html}"
    );
    assert!(!html.contains("dns-view__summary"), "{html}");
}

#[test]
fn c15_a_warning_shows_as_a_notice_line_and_is_claimed() {
    let warnings = block(
        "dns.warnings",
        "Warnings",
        Shape::Table {
            key: "warning".into(),
            columns: vec!["warning".into()],
            rows: vec![vec![text("server group truncated")]],
        },
    );
    let a = answers(
        true,
        vec![
            field("duration", Value::Duration(12)),
            field("transport", text("udp")),
        ],
        &[("A", "agree")],
    );

    let (html, claimed) = render_blocks(&[a, warnings]);

    assert!(
        html.contains("<p class=\"dns-view__notice\">server group truncated</p>"),
        "{html}"
    );
    assert!(claimed.contains(&"dns.warnings".to_string()), "{claimed:?}");
    assert!(
        !html.contains("<table"),
        "a generic Warnings table remains: {html}"
    );
}

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        unreachable!("no run is started by the DNS inspector")
    }
}

fn app() -> Router {
    let sse = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/contracts/prism.sse"),
    )
    .unwrap();
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            dns: Some(netray_dns::testing::stub_jobs()),
            dns_module: Some(Arc::from(netray_dns::testing::golden_module(&sse))),
            ..Default::default()
        });
    let deps = InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
        .with_ip_extractor(extractor);
    inspect_router(deps)
}

async fn post_run(app: &Router, q: &str) -> (StatusCode, String) {
    let body = format!("q={}", q.replace('+', "%2B").replace(' ', "+"));
    let mut req = Request::post("/inspect/dns/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", CALLER)
        .body(Body::from(body))
        .unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes)
        .replace("\r\n", "\n")
        .replace("\\\"", "\"");
    (status, text)
}

#[tokio::test]
async fn c11_an_unanswered_type_card_reads_not_measured_with_its_reason() {
    let app = app();

    let (status, s) = post_run(&app, "partial.example.net A AAAA").await;

    assert_eq!(status, 200, "{s}");
    let card = element(&s, "<article class=\"dns-type\"", "</article>")
        .and_then(|_| {
            let start = s.find("data-type=\"AAAA\"")?;
            let rest = &s[start..];
            Some(&rest[..rest.find("</article>")?])
        })
        .unwrap_or_else(|| panic!("no AAAA card: {s}"));
    assert!(card.contains("dns-type__badge"), "{card}");
    assert!(card.contains("not measured"), "{card}");
    assert!(card.contains("SERVFAIL"), "{card}");
    let line = summary(&s).unwrap_or_else(|| panic!("no summary line: {s}"));
    assert!(line.contains("1 not measured"), "{line}");
}
