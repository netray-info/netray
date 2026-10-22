//! V2 spec v2-inspector-shell, requirement 10 (C3, C7): for one input, the live check list and
//! the stored `/<domain>/<protocol>` view of a run with the same statuses and findings render
//! the same check rows, text included. A table over the TLS and HTTP fixtures.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::pages::{
    EvidenceDeps, InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool,
    evidence_router, inspect_router,
};
use tower::ServiceExt;

struct Row {
    protocol: Protocol,
    tool: &'static str,
    form: &'static str,
    block: &'static str,
}

const ROWS: [Row; 2] = [
    Row {
        protocol: Protocol::Tls,
        tool: "tls",
        form: "h=example.com",
        block: "tls.chain@192.0.2.1:443",
    },
    Row {
        protocol: Protocol::Http,
        tool: "http",
        form: "url=https%3A%2F%2Fexample.com",
        block: "http.headers",
    },
];

fn block(id: &str) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: format!("Title of {id}"),
        shape: Shape::Text(format!("body of {id}")),
    }
}

/// Every catalogue check of the protocol with statuses given out round the kinds, a finding
/// on all but the untested ones; the first check references the block.
fn checks(protocol: Protocol, block: &str) -> Vec<CheckResult> {
    let kinds = [
        Status::Fail,
        Status::Warn,
        Status::Pass,
        Status::NotApplicable,
        Status::Unmeasured,
    ];
    catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == protocol)
        .enumerate()
        .map(|(i, e)| CheckResult {
            id: e.id.clone(),
            status: kinds[i % kinds.len()].clone(),
            findings: vec![format!("finding {i} of {}", e.id)],
            evidence: if i == 0 {
                vec![BlockId::parse(block).unwrap()]
            } else {
                Vec::new()
            },
        })
        .collect()
}

struct Stub {
    protocol: Protocol,
    block: &'static str,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let b = block(self.block);
        let part = JobPart {
            label: "part".into(),
            output: Box::pin(async move { Ok(vec![b]) }),
        };
        let band = checks(self.protocol, self.block);
        Ok(Job::new(vec![part]).with_band(self.protocol, Box::pin(async move { Ok(band) })))
    }
}

fn stored_run(protocol: Protocol, block_id: &str) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: checks(protocol, block_id)
            .into_iter()
            .map(|c| RunCheck {
                id: c.id,
                status: c.status,
                findings: c.findings,
                evidence: c.evidence,
            })
            .collect(),
        blocks: vec![block(block_id)],
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
async fn live_html(app: &Router, tool: &str, form: &str) -> String {
    let (status, text) = body(
        app,
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

/// The check-row articles of a page, whitespace normalised.
fn articles(html: &str) -> Vec<String> {
    html.split(r#"<article class="check-row""#)
        .skip(1)
        .map(|rest| {
            let end = rest.find("</article>").unwrap_or(rest.len());
            let a = format!(r#"<article class="check-row"{}</article>"#, &rest[..end]);
            a.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .collect()
}

#[tokio::test]
async fn live_list_and_stored_view_render_the_same_rows() {
    for row in &ROWS {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(&dir.path().join("history.db"))
            .await
            .unwrap();
        store.migrate().await.unwrap();
        let run = stored_run(row.protocol, row.block);
        store
            .record("example.com", &run, &[], 1_800_000_000, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
        let store = Arc::new(store);

        let live_app = inspect_router(
            InspectDeps::new(
                Arc::new(Stub {
                    protocol: row.protocol,
                    block: row.block,
                }),
                InspectConfig::default(),
            )
            .with_history(Some(store.clone())),
        );
        let stored_app = evidence_router(EvidenceDeps { history: store });

        let live = articles(&live_html(&live_app, row.tool, row.form).await);
        let (status, page) = body(
            &stored_app,
            Request::get(format!("/example.com/{}", row.tool))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{}: {page}", row.tool);
        let stored = articles(&page);

        let want = catalogue()
            .entries()
            .iter()
            .filter(|e| e.protocol == row.protocol)
            .count();
        assert_eq!(stored.len(), want, "{}: stored rows", row.tool);
        assert_eq!(live.len(), want, "{}: live rows", row.tool);
        assert_eq!(live, stored, "{}", row.tool);
        let link = format!("href=\"#ev-{}\"", row.block);
        assert!(live[0].contains(&link), "{}: {}", row.tool, live[0]);
    }
}
