//! v2-dns-ui Phase 1 (C4, C5, C10, C16, C17): the DNS part view's type cards and answer groups,
//! from hand-built `dns.answers` blocks and through the inspector route over the DNS stubs.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, Response, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::links::decorate;
use netray_web::pages::views::{ViewContext, bar_width, render_part};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn pair(key: &str, value: Value) -> (String, Value) {
    (key.to_string(), value)
}

/// One server answer with one A record; `ms` is the response time.
fn server(address: &str, name: &str, ttl: i64, data: &str, ms: u64) -> Node {
    Node {
        fields: vec![
            pair("server", text(&format!("udp:{address}:53,name={name}"))),
            pair("address", text(&format!("{address}:53"))),
            pair("name", text(name)),
            pair("transport", text("udp")),
            pair("outcome", text("answered")),
            pair("response_time", Value::Duration(ms)),
        ],
        children: vec![Node {
            fields: vec![
                pair("name", text("example.com.")),
                pair("type", text("A")),
                pair("ttl", Value::Int(ttl)),
                pair("data", text(data)),
            ],
            children: vec![],
        }],
    }
}

fn type_node(record_type: &str, agreement: &str, servers: Vec<Node>) -> Node {
    Node {
        fields: vec![
            pair("record_type", text(record_type)),
            pair("agreement", text(agreement)),
        ],
        children: servers,
    }
}

fn answers(mode: &str, types: Vec<Node>) -> Block {
    let lookups: usize = types.iter().map(|t| t.children.len()).sum();
    Block {
        id: BlockId::parse("dns.answers").unwrap(),
        title: "Answers".into(),
        shape: Shape::Tree(Node {
            fields: vec![
                pair("mode", text(mode)),
                pair("finished", Value::Bool(true)),
                pair("record_types", Value::Int(types.len() as i64)),
                pair("lookups", Value::Int(lookups as i64)),
                pair("servers", Value::Int(3)),
                pair("agree", Value::Int(0)),
                pair("diverge", Value::Int(0)),
                pair("single", Value::Int(0)),
                pair("unmeasured", Value::Int(0)),
            ],
            children: types,
        }),
    }
}

fn html_of(block: Block) -> String {
    let statuses = vec![];
    let decorate = decorate(&[]);
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render_part(&[block], &ctx)
        .expect("dns.answers is claimed by the DNS part view")
        .html
}

/// The `dns-type` card of `record_type`.
fn card<'a>(html: &'a str, record_type: &str) -> &'a str {
    let marker = format!(r#"data-type="{record_type}""#);
    let at = html
        .find(&marker)
        .unwrap_or_else(|| panic!("no {record_type} card: {html}"));
    let start = html[..at]
        .rfind("<article")
        .expect("card opens with article");
    let len = html[at..].find("</article>").expect("card closes");
    &html[start..at + len]
}

#[test]
fn c4_one_card_per_type_in_summary_order_with_the_agreement_badge() {
    let cases = [
        ("agree", "pass", "agree"),
        ("diverge", "warn", "diverge"),
        ("single", "muted", "single answer"),
        ("unmeasured", "muted", "not measured"),
    ];
    let types = vec![
        type_node(
            "A",
            "agree",
            vec![server("8.8.8.8", "Google 1", 300, "192.0.2.10", 10)],
        ),
        type_node(
            "AAAA",
            "diverge",
            vec![server("8.8.8.8", "Google 1", 300, "192.0.2.10", 10)],
        ),
        type_node(
            "MX",
            "single",
            vec![server("8.8.8.8", "Google 1", 300, "192.0.2.10", 10)],
        ),
        type_node("TXT", "unmeasured", vec![]),
    ];
    let html = html_of(answers("query", types));

    assert_eq!(html.matches(r#"class="dns-type""#).count(), 4, "{html}");
    let order: Vec<usize> = ["A", "AAAA", "MX", "TXT"]
        .iter()
        .map(|t| html.find(&format!(r#"data-type="{t}""#)).unwrap())
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{html}");
    for (t, (agreement, tone, label)) in ["A", "AAAA", "MX", "TXT"].iter().zip(cases) {
        let c = card(&html, t);
        assert!(
            c.contains(&format!(r#"data-agreement="{agreement}""#)),
            "{c}"
        );
        assert!(
            c.contains(&format!(r#"<h4 class="dns-type__name">{t}</h4>"#)),
            "{c}"
        );
        assert!(
            c.contains(&format!(
                r#"class="dns-type__badge" data-tone="{tone}">{label}</span>"#
            )),
            "{c}"
        );
    }
}

#[test]
fn c5_groups_by_identical_answer_largest_first_with_ttl_range_and_servers() {
    let types = vec![type_node(
        "A",
        "diverge",
        vec![
            server("1.1.1.1", "Cloudflare 1", 60, "192.0.2.20", 20),
            server("8.8.8.8", "Google 1", 300, "192.0.2.10", 10),
            server("9.9.9.9", "Quad9 1", 60, "192.0.2.10", 40),
        ],
    )];
    let html = html_of(answers("query", types));
    let c = card(&html, "A");

    assert_eq!(c.matches(r#"class="dns-group""#).count(), 2, "{c}");
    let big = c.find("2 / 3 servers").expect("largest group count");
    let small = c.find("1 / 3 servers").expect("smaller group count");
    assert!(big < small, "{c}");
    assert!(
        c.contains(r#"class="dns-group__count">2 / 3 servers</span>"#),
        "{c}"
    );
    // The two-server group has TTLs 300 and 60; the other a single 60.
    assert!(
        c.contains(r#"class="dns-group__ttl">TTL 60 to 300 s</span>"#),
        "{c}"
    );
    assert!(
        c.contains(r#"class="dns-group__ttl">TTL 60 s</span>"#),
        "{c}"
    );
    assert_eq!(c.matches(r#"class="dns-group__server""#).count(), 3, "{c}");
    assert_eq!(
        c.matches(r#"class="dns-group__transport">udp</span>"#)
            .count(),
        3,
        "{c}"
    );
    let first_group = &c[big..small];
    assert!(
        first_group.contains("Google 1") && first_group.contains("Quad9 1"),
        "{c}"
    );
    assert!(c[small..].contains("Cloudflare 1"), "{c}");
}

#[test]
fn c5_a_server_without_a_name_shows_its_address() {
    let mut s = server("8.8.8.8", "x", 300, "192.0.2.10", 10);
    s.fields.retain(|(k, _)| k != "name");
    let html = html_of(answers("query", vec![type_node("A", "single", vec![s])]));

    assert!(card(&html, "A").contains("8.8.8.8"), "{html}");
}

#[test]
fn c16_bar_widths_scale_to_the_slowest_server() {
    let types = vec![type_node(
        "A",
        "diverge",
        vec![
            server("8.8.8.8", "Google 1", 300, "192.0.2.10", 10),
            server("1.1.1.1", "Cloudflare 1", 300, "192.0.2.20", 40),
        ],
    )];
    let html = html_of(answers("query", types));

    assert_eq!(
        html.matches(r#"class="dns-bar__fill" style="width: 100%""#)
            .count(),
        1,
        "{html}"
    );
    assert_eq!(
        html.matches(r#"class="dns-bar__fill" style="width: 25%""#)
            .count(),
        1,
        "{html}"
    );
    assert_eq!(bar_width(10, 40), 25);
    assert_eq!(bar_width(40, 40), 100);
    assert_eq!(bar_width(5, 0), 0);
}

#[test]
fn c17_the_agree_badge_names_the_comparison_mode() {
    for (mode, label) in [
        ("compare", "all transports agree"),
        ("authcompare", "auth and recursive agree"),
    ] {
        let types = vec![type_node(
            "A",
            "agree",
            vec![server("8.8.8.8", "Google 1", 300, "192.0.2.10", 10)],
        )];
        let html = html_of(answers(mode, types));

        assert!(
            card(&html, "A").contains(&format!(r#"data-tone="pass">{label}</span>"#)),
            "{mode}: {html}"
        );
    }
}

const PROXY: &str = "203.0.113.9";
const CALLER: &str = "198.51.100.1";

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
    let golden = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/contracts/prism.sse"
    ))
    .expect("prism check fixture");
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            dns: Some(netray_dns::testing::stub_jobs()),
            dns_module: Some(Arc::from(netray_dns::testing::golden_module(&golden))),
            ..Default::default()
        });
    let deps = InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
        .with_ip_extractor(extractor);
    inspect_router(deps)
}

async fn run_q(app: &Router, q: &str) -> String {
    let body = format!(
        "q={}",
        q.replace('+', "%2B").replace(' ', "+").replace('@', "%40")
    );
    let mut req = Request::post("/inspect/dns/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", CALLER)
        .body(Body::from(body))
        .unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp: Response<Body> = app.clone().oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes)
        .replace("\r\n", "\n")
        .replace("\\\"", "\"")
}

#[tokio::test]
async fn c10_two_servers_that_differ_make_a_diverge_card_with_two_groups() {
    let app = app();

    let s = run_q(&app, "diverge.example.com A @google @cloudflare").await;

    let c = card(&s, "A");
    assert!(c.contains(r#"data-agreement="diverge""#), "{s}");
    assert!(c.contains(r#"data-tone="warn">diverge</span>"#), "{s}");
    assert_eq!(c.matches(r#"class="dns-group""#).count(), 2, "{s}");
    assert_eq!(c.matches(r#"class="dns-group__server""#).count(), 2, "{s}");
    let g2 = c.rfind(r#"class="dns-group""#).unwrap();
    let (first, second) = c.split_at(g2);
    let has = |g: &str, ip: &str| g.contains(ip);
    assert!(
        (has(first, "8.8.8.8") && !has(first, "1.1.1.1") && has(second, "1.1.1.1"))
            || (has(second, "8.8.8.8") && !has(second, "1.1.1.1") && has(first, "1.1.1.1")),
        "{s}"
    );
}
