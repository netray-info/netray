//! v2-tls-ui Phase 2, requirement 12 (C11, C3): the inspector's related-records summary over the
//! band port's blocks, and the box filled live and in a shared run.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_model::Protocol;
use netray_model::evidence::{Block, BlockId, Record, Shape, Value};
use netray_web::pages::views::tls_related_summary;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn records(id: &str, n: usize) -> Block {
    let rows = (0..n)
        .map(|i| Record {
            owner: text("example.com"),
            rtype: text("CAA"),
            ttl: Value::Int(300),
            data: text(&format!("0 issue \"ca{i}.example.net\"")),
        })
        .collect();
    Block {
        id: BlockId::parse(id).unwrap(),
        title: "Records".into(),
        shape: Shape::Records(rows),
    }
}

fn handshake(ech: Option<bool>) -> Block {
    let mut fields = vec![("protocol".to_string(), text("TLSv1.3"))];
    if let Some(e) = ech {
        fields.push(("ech".to_string(), Value::Bool(e)));
    }
    Block {
        id: BlockId::parse("tls.handshake@192.0.2.1:443").unwrap(),
        title: "Handshake".into(),
        shape: Shape::Fields(fields),
    }
}

fn addresses() -> Block {
    Block {
        id: BlockId::parse("tls.addresses").unwrap(),
        title: "Addresses".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "port", "outcome", "message"]
                .map(String::from)
                .to_vec(),
            rows: vec![vec![
                Value::Addr("192.0.2.1".parse().unwrap()),
                Value::Int(443),
                text("ok"),
                text(""),
            ]],
        },
    }
}

#[test]
fn c11_summary_reads_caa_tlsa_and_https_from_the_blocks() {
    let cases: Vec<(&str, Vec<Block>, &str)> = vec![
        ("two CAA", vec![records("tls.caa", 2)], "CAA (2 records)"),
        (
            "no TLSA block",
            vec![records("tls.caa", 2)],
            "TLSA (not measured)",
        ),
        (
            "empty TLSA",
            vec![records("tls.tlsa@443", 0)],
            "TLSA (none)",
        ),
        ("one CAA", vec![records("tls.caa", 1)], "CAA (1 record)"),
        (
            "empty CAA",
            vec![records("tls.caa", 0)],
            "CAA (none published)",
        ),
        ("ech true", vec![handshake(Some(true))], "HTTPS (ech=)"),
        ("ech false", vec![handshake(Some(false))], "HTTPS (no ech=)"),
        ("no ech", vec![handshake(None)], "HTTPS (not measured)"),
    ];
    for (name, blocks, want) in cases {
        let got = tls_related_summary(&blocks);
        assert!(got.contains(want), "{name}: want {want:?} in {got:?}");
    }
}

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let blocks = vec![addresses(), handshake(Some(false)), records("tls.caa", 1)];
        let part = JobPart {
            label: "handshake".into(),
            output: Box::pin(async move { Ok(blocks) }),
        };
        Ok(Job::new(vec![part]).with_band(Protocol::Tls, Box::pin(async { Ok(vec![]) })))
    }
}

async fn send(app: &Router, req: Request<Body>) -> String {
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).replace("\r\n", "\n")
}

/// (id, html) of every fragment event.
fn fragments(stream: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in stream.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let data: String = raw
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(|l| l.trim_start())
            .collect();
        if let Ok(j) = serde_json::from_str::<serde_json::Value>(&data) {
            out.push((
                j["id"].as_str().unwrap_or_default().to_owned(),
                j["html"].as_str().unwrap_or_default().to_owned(),
            ));
        }
    }
    out
}

const FILLED: &str = "Related records live in the DNS inspector: CAA (1 record)";

#[tokio::test]
async fn c3_related_box_is_filled_live_and_in_a_shared_run() {
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let stream = send(
        &app,
        Request::post("/inspect/tls/run")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("h=example.com"))
            .unwrap(),
    )
    .await;
    let frags = fragments(&stream);

    let start = &frags[0].1;
    assert!(start.contains(r#"id="inspect-related""#), "{start}");
    assert!(start.contains("CAA · TLSA · HTTPS"), "{start}");

    let live = frags
        .iter()
        .find(|(id, _)| id == "inspect-related")
        .unwrap_or_else(|| panic!("no inspect-related fragment: {stream}"));
    assert!(live.1.contains(FILLED), "{}", live.1);

    let marker = "data-share-key=\\\"";
    let a = stream.find(marker).expect("no share key") + marker.len();
    let key = &stream[a..a + 12];
    let page = send(
        &app,
        Request::get(format!("/inspect/tls?r={key}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert!(page.contains(r#"id="inspect-related""#), "{page}");
    assert!(page.contains(FILLED), "{page}");
}
