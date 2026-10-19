//! V2 spec v2-inspectors, requirement 14 (C10, C12): the per-tool concurrency slot refuses
//! the next job at once with a busy fragment, and the per-tool deadline turns a part still
//! running into a "timed out" fragment while finished parts keep their blocks.
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_model::evidence::{Block, BlockId, Record, Shape, Value};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tokio::sync::Notify;
use tower::ServiceExt;

fn block(id: &str) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: "Title".into(),
        shape: Shape::Records(vec![Record {
            owner: Value::Text("www.example.com".into()),
            rtype: Value::Text("A".into()),
            ttl: Value::Int(60),
            data: Value::Addr("192.0.2.10".parse().unwrap()),
        }]),
    }
}

enum Plan {
    /// One part that finishes once the notify fires.
    Gate(Arc<Notify>),
    /// Two parts finishing after the given milliseconds, with these block ids.
    Timed([(u64, &'static str); 2]),
}

struct Stub {
    plan: Plan,
    starts: AtomicUsize,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }

    fn start(&self, _: JobRequest) -> Result<Job, String> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        let parts = match &self.plan {
            Plan::Gate(n) => {
                let n = n.clone();
                vec![JobPart {
                    label: "handshake".into(),
                    output: Box::pin(async move {
                        n.notified().await;
                        Ok(vec![block("tls.chain")])
                    }),
                }]
            }
            Plan::Timed(specs) => specs
                .iter()
                .map(|&(ms, id)| JobPart {
                    label: id.into(),
                    output: Box::pin(async move {
                        tokio::time::sleep(Duration::from_millis(ms)).await;
                        Ok(vec![block(id)])
                    }),
                })
                .collect(),
        };
        Ok(Job::new(parts))
    }
}

fn post() -> Request<Body> {
    Request::post("/inspect/tls/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("h=example.com"))
        .unwrap()
}

/// (event name, data) pairs of a collected SSE body.
fn events(body: &str) -> Vec<(String, serde_json::Value)> {
    body.split("\n\n")
        .filter(|e| !e.trim().is_empty())
        .map(|e| {
            let mut name = String::new();
            let mut data = serde_json::Value::Null;
            for line in e.lines() {
                if let Some(n) = line.strip_prefix("event: ") {
                    name = n.into();
                } else if let Some(d) = line.strip_prefix("data: ") {
                    data = serde_json::from_str(d).unwrap();
                }
            }
            (name, data)
        })
        .collect()
}

#[tokio::test]
async fn second_job_is_refused_at_once_while_the_slot_is_held() {
    let gate = Arc::new(Notify::new());
    let stub = Arc::new(Stub {
        plan: Plan::Gate(gate.clone()),
        starts: AtomicUsize::new(0),
    });
    let mut config = InspectConfig::default();
    config.tls.max_concurrent = 1;
    let app = inspect_router(InspectDeps::new(stub.clone(), config));

    // The first job holds the slot while its stream is open.
    let first = app.clone().oneshot(post()).await.unwrap();
    assert_eq!(first.status(), 200);
    let mut first_body = first.into_body();
    let _ = first_body.frame().await.expect("first frame").unwrap();

    let second = tokio::time::timeout(Duration::from_secs(1), app.clone().oneshot(post()))
        .await
        .expect("second POST answers at once")
        .unwrap();
    assert_eq!(second.status(), 503);
    let text = String::from_utf8(
        second
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    let ev = events(&text);
    assert_eq!(ev.len(), 2, "one fragment then end: {text}");
    assert_eq!(ev[0].0, "fragment");
    assert!(
        ev[0].1["html"]
            .as_str()
            .unwrap()
            .contains("busy, try again in a moment"),
        "{text}"
    );
    assert_eq!(ev[1].0, "end");
    assert_eq!(stub.starts.load(Ordering::SeqCst), 1);

    gate.notify_one();
    let rest = first_body.collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&rest).contains("event: end"));
}

#[tokio::test]
async fn part_past_the_deadline_shows_timed_out_and_the_stream_ends_early() {
    let stub = Arc::new(Stub {
        plan: Plan::Timed([(50, "tls.fast"), (500, "tls.slow")]),
        starts: AtomicUsize::new(0),
    });
    let mut config = InspectConfig::default();
    config.tls.deadline_ms = 100;
    let app = inspect_router(InspectDeps::new(stub, config));

    let began = Instant::now();
    let res = app.oneshot(post()).await.unwrap();
    assert_eq!(res.status(), 200);
    let text =
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(
        began.elapsed() < Duration::from_millis(400),
        "stream took {:?}",
        began.elapsed()
    );

    let ev = events(&text);
    let frag = |id: &str| {
        ev.iter()
            .find(|(n, d)| n == "fragment" && d["id"] == id)
            .map(|(_, d)| d["html"].as_str().unwrap().to_string())
            .unwrap_or_else(|| panic!("no fragment {id}: {text}"))
    };
    assert!(frag("inspect-part-0").contains("id=\"ev-tls.fast\""));
    assert!(frag("inspect-part-1").contains("timed out"));
    assert!(!text.contains("ev-tls.slow"));
    assert_eq!(ev.last().unwrap().0, "end");
}
