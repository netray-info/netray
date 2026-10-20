//! V2 spec v2-inspector-shell, requirement 6 (C6, C17, C18): the run's start fragment holds the
//! run title, the h1 naming the target, and the back link with the stored grade letter.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        Ok(Job::new(vec![]))
    }
}

fn graded_run(grade: Grade) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

/// An app over a temp store; `entry` is a domain recorded with a grade, `None` for an empty store.
async fn app(entry: Option<(&str, Grade)>, with_history: bool) -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    if let Some((domain, grade)) = entry {
        store
            .record(domain, &graded_run(grade), &[], 1_800_000_000, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    let mut deps = InspectDeps::new(Arc::new(Stub), InspectConfig::default());
    if with_history {
        deps = deps.with_history(Some(Arc::new(store)));
    }
    (inspect_router(deps), dir)
}

/// The html of the first fragment of a run's stream.
async fn start_fragment(app: &Router, body: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::post("/inspect/tls/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let raw = text
        .split("\n\n")
        .find(|e| e.contains("event: fragment"))
        .unwrap();
    let data: String = raw
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim_start)
        .collect();
    let json: serde_json::Value = serde_json::from_str(&data).unwrap();
    assert_eq!(json["id"], "inspect-result");
    json["html"].as_str().unwrap().to_owned()
}

/// True when `s` looks like `YYYY-MM-DD HH:MM UTC`.
fn is_stamp(s: &str) -> bool {
    let pattern = b"dddd-dd-dd dd:dd UTC";
    s.len() == pattern.len()
        && s.bytes().zip(pattern).all(|(c, p)| match p {
            b'd' => c.is_ascii_digit(),
            _ => c == *p,
        })
}

#[tokio::test]
async fn c17_start_fragment_has_title_target_and_back_link() {
    let (app, _dir) = app(None, true).await;

    let html = start_fragment(&app, "h=example.com%3A443").await;

    assert!(
        html.contains(r#"<h1 class="inspector-run__target">example.com:443</h1>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<a class="inspector-backlink" href="/example.com">"#),
        "{html}"
    );
    assert!(
        html.contains("Back to the result of example.com →"),
        "{html}"
    );
    let title = html
        .split(r#"<p class="run-line run-line--title">"#)
        .nth(1)
        .unwrap_or_else(|| panic!("no run title in {html}"))
        .split("</p>")
        .next()
        .unwrap();
    let parts: Vec<&str> = title
        .split(r#"run-line__part">"#)
        .skip(1)
        .map(|s| s.split("</span>").next().unwrap())
        .collect();
    assert_eq!(parts.len(), 3, "{title}");
    assert_eq!(parts[0], "live");
    assert!(is_stamp(parts[1]), "{parts:?}");
    assert_eq!(parts[2], "not recorded");

    // The title and the back link share the head's line.
    let head = html.split(r#"class="inspector-run__head""#).nth(1).unwrap();
    let head = head.split("<h1").next().unwrap();
    assert!(
        head.contains("run-line--title") && head.contains("inspector-backlink"),
        "{html}"
    );

    let html = start_fragment(&app, "h=example.com").await;
    assert!(
        html.contains(r#"<h1 class="inspector-run__target">example.com:443</h1>"#),
        "{html}"
    );

    let html = start_fragment(&app, "h=192.0.2.1").await;
    assert!(!html.contains("inspector-backlink"), "{html}");
}

#[tokio::test]
async fn c17_the_old_backlink_paragraph_is_gone_from_the_page() {
    let (app, _dir) = app(None, true).await;
    let resp = app
        .oneshot(
            Request::get("/inspect/tls?h=example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(!body.contains("inspect-backlink"), "{body}");
}

#[tokio::test]
async fn c18_back_link_carries_the_stored_grade_only_with_an_entry() {
    let (with_entry, _d1) = app(Some(("example.com", Grade::B)), true).await;
    let html = start_fragment(&with_entry, "h=example.com").await;
    assert!(
        html.contains(r#"<span class="inspector-backlink__grade">B</span>"#),
        "{html}"
    );
    let link = html
        .split(r#"<a class="inspector-backlink""#)
        .nth(1)
        .unwrap();
    assert!(
        link.split('>')
            .nth(1)
            .unwrap()
            .starts_with(r#"<span class="inspector-backlink__grade">B"#),
        "{link}"
    );

    let (no_entry, _d2) = app(Some(("other.example.org", Grade::B)), true).await;
    let html = start_fragment(&no_entry, "h=example.com").await;
    assert!(html.contains("Back to the result of example.com"), "{html}");
    assert!(!html.contains("inspector-backlink__grade"), "{html}");

    let (no_history, _d3) = app(Some(("example.com", Grade::B)), false).await;
    let html = start_fragment(&no_history, "h=example.com").await;
    assert!(html.contains("Back to the result of example.com"), "{html}");
    assert!(!html.contains("inspector-backlink__grade"), "{html}");
}
