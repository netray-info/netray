//! The result page's DKIM selectors (spec v2-dkim-legacy-urls, Phase 3; R5-R7, C1-C3, C5-C12):
//! the run form carries a selector list to `start_with`, the page echoes a valid list into the
//! recheck form, and the idle `/?d=` redirect keeps it.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunSection, SectionKind};
use netray_model::{Grade, Protocol};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";

/// (trigger, selectors) of each call; `None` selectors marks a plain `start`.
type Call = (RunTrigger, Option<Vec<String>>);

struct StubRuns {
    calls: Mutex<Vec<Call>>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn done_run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::NotApplicable,
            reason: Some("none".into()),
        }],
        checks: vec![],
        blocks: vec![],
    }
}

fn admitted() -> RunStart {
    let events = vec![
        RunEvent::Section(SectionEvent {
            protocol: Protocol::Tls,
            outcome: SectionOutcome::NotApplicable {
                reason: "none".into(),
            },
        }),
        RunEvent::Done(done_run()),
        RunEvent::Recorded(None),
    ];
    RunStart::Admitted(Box::pin(futures::stream::iter(events)))
}

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.trim().to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        self.calls.lock().unwrap().push((trigger, None));
        Box::pin(async { admitted() })
    }
    fn start_with<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        trigger: RunTrigger,
        _prefetch: bool,
        dkim_selectors: &'a [String],
    ) -> BoxFuture<'a, RunStart> {
        self.calls
            .lock()
            .unwrap()
            .push((trigger, Some(dkim_selectors.to_vec())));
        Box::pin(async { admitted() })
    }
}

/// An app whose store holds a fresh head entry for `DOMAIN`.
async fn app() -> (Router, Arc<StubRuns>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record(DOMAIN, &done_run(), &[], now() - 10, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let runs = Arc::new(StubRuns {
        calls: Mutex::new(vec![]),
    });
    let app = router(ResultState::new(Arc::new(store), runs.clone(), 300));
    (app, runs, dir)
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    (
        parts.status,
        parts.headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn post(form: &str, sse: bool) -> Request<Body> {
    let mut b = Request::post(format!("/{DOMAIN}/run"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if sse {
        b = b.header(header::ACCEPT, "text/event-stream");
    }
    b.body(Body::from(form.to_owned())).unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

fn sel(list: &[&str]) -> Option<Vec<String>> {
    Some(list.iter().map(|s| s.to_string()).collect())
}

/// The `<form id="recheck"` element up to its closing tag.
fn recheck_form(body: &str) -> &str {
    let at = body
        .find(r#"<form id="recheck""#)
        .unwrap_or_else(|| panic!("no recheck form in {body}"));
    let rest = &body[at..];
    &rest[..rest.find("</form>").expect("form not closed")]
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[tokio::test]
async fn c5_a_visit_with_a_fresh_head_and_selectors_starts_a_selector_run_over_sse() {
    let (app, runs, _dir) = app().await;

    let (status, _, body) = send(&app, post("trigger=visit&dkim_selectors=s1", true)).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("event: end") || body.contains("event:end"),
        "{body}"
    );
    assert_eq!(
        *runs.calls.lock().unwrap(),
        vec![(RunTrigger::Visit, sel(&["s1"]))]
    );
}

#[tokio::test]
async fn c1_a_recheck_with_selectors_starts_a_selector_run_over_sse() {
    let (app, runs, _dir) = app().await;

    let (status, _, body) = send(&app, post("trigger=recheck&dkim_selectors=a,%20b", true)).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("event: end") || body.contains("event:end"),
        "{body}"
    );
    assert_eq!(
        *runs.calls.lock().unwrap(),
        vec![(RunTrigger::Recheck, sel(&["a", "b"]))]
    );
}

#[tokio::test]
async fn c6_an_invalid_selector_list_answers_400_and_starts_nothing() {
    for trigger in ["recheck", "visit"] {
        let (app, runs, _dir) = app().await;

        let (status, _, body) = send(
            &app,
            post(&format!("trigger={trigger}&dkim_selectors=bad.sel"), true),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{trigger}: {body}");
        assert!(runs.calls.lock().unwrap().is_empty(), "{trigger}");
    }
}

#[tokio::test]
async fn c7_a_visit_with_an_empty_selector_value_takes_the_plain_path() {
    let (app, runs, _dir) = app().await;

    let (status, _, body) = send(&app, post("trigger=visit&dkim_selectors=", true)).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let calls = runs.calls.lock().unwrap();
    assert!(
        calls.is_empty()
            || calls
                .iter()
                .all(|(_, s)| s.as_ref().is_none_or(|v| v.is_empty())),
        "{calls:?}"
    );
}

#[tokio::test]
async fn c8_a_selector_run_without_sse_answers_the_full_page_no_store() {
    let (app, runs, _dir) = app().await;

    let (status, headers, body) =
        send(&app, post("trigger=recheck&dkim_selectors=s1", false)).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert!(body.contains(r#"id="result""#), "{body}");
    assert!(body.contains(r#"grade__letter">A<"#), "{body}");
    assert!(strip_tags(&body).contains("not recorded"), "{body}");
    assert_eq!(
        *runs.calls.lock().unwrap(),
        vec![(RunTrigger::Recheck, sel(&["s1"]))]
    );
}

#[tokio::test]
async fn c2_c9_a_page_with_selectors_echoes_the_normalised_list_into_the_recheck_form() {
    let (app, _runs, _dir) = app().await;

    let (status, headers, body) = send(&app, get("/example.com?dkim_selectors=s1,%20s2")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    let form = recheck_form(&body);
    assert!(form.contains(r#"data-stream-autostart="visit""#), "{form}");
    assert!(
        form.contains(r#"<input type="hidden" name="dkim_selectors" value="s1,s2">"#),
        "{form}"
    );
}

#[tokio::test]
async fn c10_a_page_with_an_invalid_selector_list_answers_400() {
    let (app, _runs, _dir) = app().await;

    let (status, _, body) = send(&app, get("/example.com?dkim_selectors=bad.sel")).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn c11_a_plain_page_has_no_selector_input() {
    let (app, _runs, _dir) = app().await;

    let (status, _, body) = send(&app, get("/example.com")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains(r#"name="dkim_selectors""#), "{body}");
}

#[tokio::test]
async fn c3_c12_the_idle_redirect_keeps_the_selector_parameter() {
    let (app, _runs, _dir) = app().await;

    let (status, headers, _) = send(&app, get("/?d=example.com&dkim_selectors=google,s1")).await;

    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    let loc = headers.get(header::LOCATION).unwrap().to_str().unwrap();
    assert!(
        loc == "/example.com?dkim_selectors=google%2Cs1"
            || loc == "/example.com?dkim_selectors=google,s1",
        "{loc}"
    );
}

#[tokio::test]
async fn c12_the_idle_redirect_without_selectors_goes_to_the_bare_path() {
    let (app, _runs, _dir) = app().await;

    let (status, headers, _) = send(&app, get("/?d=example.com")).await;

    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(headers.get(header::LOCATION).unwrap(), "/example.com");
}

/// The redirect to the canonical domain keeps the selector list (Phase 3 review finding).
#[tokio::test]
async fn the_canonical_redirect_keeps_the_selector_parameter() {
    let (app, _runs, _dir) = app().await;

    // The stub's validate lowercases only; lens's also strips a trailing dot.
    for uri in ["/Example.com?dkim_selectors=s1"] {
        let (status, headers, _) = send(&app, get(uri)).await;
        assert_eq!(status, StatusCode::MOVED_PERMANENTLY, "{uri}");
        let loc = headers.get(header::LOCATION).unwrap().to_str().unwrap();
        assert_eq!(loc, "/example.com?dkim_selectors=s1", "{uri}");
    }
}
