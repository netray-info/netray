//! V2 spec v2-inspector-shell, requirements 21 and 23 (C1, C3, C4, C5, C9, C10, C11): address and
//! host links come from one function over typed values and render in inspector runs and stored
//! evidence views; a TLS run of a host name ends with the related-records box.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Record, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{Grade, Protocol};
use netray_store::HistoryStore;
use netray_web::links::value_link;
use netray_web::pages::{
    EvidenceDeps, InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool,
    evidence_router, inspect_router,
};
use tower::ServiceExt;

fn addr(s: &str) -> Value {
    Value::Addr(s.parse().unwrap())
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn record(owner: &str, rtype: &str, data: Value) -> Record {
    Record {
        owner: text(owner),
        rtype: text(rtype),
        ttl: Value::Int(300),
        data,
    }
}

fn records_block(id: &str, rows: Vec<Record>) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: format!("Title of {id}"),
        shape: Shape::Records(rows),
    }
}

fn addresses_block(ip: &str) -> Block {
    Block {
        id: BlockId::parse("tls.addresses").unwrap(),
        title: "Addresses".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "port", "outcome", "message"]
                .map(String::from)
                .into(),
            rows: vec![vec![addr(ip), Value::Int(443), text("ok"), text("")]],
        },
    }
}

struct Stub {
    blocks: Vec<Block>,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let blocks = self.blocks.clone();
        let part = JobPart {
            label: "part".into(),
            output: Box::pin(async move { Ok(blocks) }),
        };
        Ok(Job::new(vec![part]))
    }
}

async fn body(app: &Router, req: Request<Body>) -> (StatusCode, String) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
    )
}

/// The html of every fragment of a live run, joined.
async fn live_html(blocks: Vec<Block>, tool: &str, form: &str) -> String {
    let app = inspect_router(InspectDeps::new(
        Arc::new(Stub { blocks }),
        InspectConfig::default(),
    ));
    let (status, text) = body(
        &app,
        Request::post(format!("/inspect/{tool}/run"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(form.to_owned()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let mut out = String::new();
    for raw in text.split("\n\n").filter(|e| e.contains("event: fragment")) {
        let data: String = raw
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect();
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        out.push_str(json["html"].as_str().unwrap_or_default());
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

/// The decoded hrefs of every anchor with the class, in document order.
fn hrefs(html: &str, class: &str) -> Vec<String> {
    let marker = format!("class=\"{class}\" href=\"");
    html.split(&marker)
        .skip(1)
        .map(|rest| percent_decode(&rest[..rest.find('"').unwrap()]).replace("&amp;", "&"))
        .collect()
}

#[tokio::test]
async fn dns_run_links_addresses_and_record_hosts() {
    let blocks = vec![
        records_block(
            "dns.a",
            vec![record("example.com.", "A", addr("192.0.2.24"))],
        ),
        records_block(
            "dns.ns",
            vec![record("example.com.", "NS", text("ns1.example.com."))],
        ),
        records_block(
            "dns.mx",
            vec![record("example.com.", "MX", text("10 mx.example.com."))],
        ),
    ];

    let html = live_html(blocks, "dns", "q=example.com").await;

    assert!(
        html.contains(
            r#"<a class="evidence-value__link" href="/inspect/ip?ip=192.0.2.24">192.0.2.24</a>"#
        ),
        "{html}"
    );
    let links = hrefs(&html, "evidence-value__link");
    for want in [
        "/inspect/dns?q=ns1.example.com",
        "/inspect/dns?q=mx.example.com",
    ] {
        assert!(links.iter().any(|l| l == want), "{want} not in {links:?}");
    }
}

#[tokio::test]
async fn tls_run_links_an_ipv6_address_percent_encoded() {
    let html = live_html(
        vec![addresses_block("2001:db8::24")],
        "tls",
        "h=example.com",
    )
    .await;

    assert!(
        html.contains(r#"href="/inspect/ip?ip=2001%3Adb8%3A%3A24""#),
        "{html}"
    );
}

#[test]
fn value_link_is_one_function_over_typed_values() {
    let dns_a = BlockId::parse("dns.a").unwrap();
    assert_eq!(
        value_link(&dns_a, "data", &addr("192.0.2.24")).as_deref(),
        Some("/inspect/ip?ip=192.0.2.24")
    );
    assert_eq!(value_link(&dns_a, "data", &text("just text")), None);
    let other = BlockId::parse("tls.handshake").unwrap();
    assert_eq!(value_link(&other, "sni", &text("plain")), None);
}

#[tokio::test]
async fn stored_evidence_view_links_addresses_without_badges() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let block = addresses_block("192.0.2.24");
    let run = Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: catalogue()
            .entries()
            .iter()
            .filter(|e| e.protocol == Protocol::Tls)
            .enumerate()
            .map(|(i, e)| RunCheck {
                id: e.id.clone(),
                status: netray_model::Status::Pass,
                findings: Vec::new(),
                evidence: if i == 0 {
                    vec![block.id.clone()]
                } else {
                    Vec::new()
                },
            })
            .collect(),
        blocks: vec![block],
    };
    store
        .record("example.com", &run, &[], 1_800_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let app = evidence_router(EvidenceDeps {
        history: Arc::new(store),
    });

    let (status, page) = body(
        &app,
        Request::get("/example.com/tls")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains(r#"href="/inspect/ip?ip=192.0.2.24""#),
        "{page}"
    );
    assert!(!page.contains("ip-badge"), "{page}");
}

#[test]
fn evidence_renderer_names_no_module_and_has_the_decorated_entry_point() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/evidence.rs");
    let source = std::fs::read_to_string(&path).unwrap();
    let code: String = source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        code.contains("pub fn block_view_with"),
        "no block_view_with"
    );
    for banned in [
        "\"dns.",
        "\"tls.",
        "\"http.",
        "\"email.",
        "\"ip.",
        "netray_dns",
        "netray_tls",
        "netray_http",
        "netray_email",
        "netray_ip",
    ] {
        assert!(!code.contains(banned), "evidence.rs names `{banned}`");
    }
}

#[tokio::test]
async fn tls_run_of_a_host_ends_with_the_related_records_box() {
    let rows: [(&str, &str, &str, bool); 3] = [
        ("tls", "h=example.com%3A443", "example.com CAA", true),
        ("tls", "h=192.0.2.1%3A443", "", false),
        ("http", "url=https%3A%2F%2Fexample.com", "", false),
    ];
    for (tool, form, want_q, present) in rows {
        let html = live_html(vec![addresses_block("192.0.2.24")], tool, form).await;

        assert_eq!(
            html.contains(r#"<aside class="inspector-related""#),
            present,
            "{form}: {html}"
        );
        if present {
            assert!(
                html.contains("Related records live in the DNS inspector"),
                "{html}"
            );
            assert!(
                html.contains(&format!(
                    "href=\"/inspect/dns?q={}\"",
                    want_q.replace(' ', "%20")
                )),
                "{html}"
            );
            // The stream sends the box twice: in the start fragment and filled when the part
            // arrives (v2-tls-ui requirement 12); the second replaces the first by id.
            let mut links = hrefs(&html, "inspector-related__link");
            links.dedup();
            assert_eq!(links, vec![format!("/inspect/dns?q={want_q}")]);
        }
    }
}
