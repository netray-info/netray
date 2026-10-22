//! The selector inputs and copy (spec v2-site-nav, Phase 5, requirements 13-15; C1-C7): the idle
//! form's custom-selector disclosure, the email panel's selector form, the run title copy and the
//! selector-list refusal page.
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
use netray_web::pages::{EvidenceDeps, evidence_router};
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
        if domain.parse::<IpAddr>().is_ok() {
            return Err(DomainRefusal {
                code: "DOMAIN_INVALID",
                message: "an IP address is not a domain".into(),
            });
        }
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
    let store = Arc::new(store);
    let app = router(ResultState::new(store.clone(), runs.clone(), 300))
        .merge(evidence_router(EvidenceDeps { history: store }));
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

const FORM_OPEN: &str = r#"<form class="selector-form" action="/example.com" method="get">"#;
const DETAILS: &str = concat!(
    r#"<details class="query-input__selectors"><summary>Custom DKIM selectors</summary>"#,
    r#"<label class="query-input__label" for="query-input-dkim_selectors">DKIM selectors</label>"#,
    r#"<input class="query-input__field" id="query-input-dkim_selectors" type="text" name="dkim_selectors" placeholder="selector1, selector2">"#,
    r#"<p class="query-input__hint">Up to 5, comma-separated. A run with custom selectors is not recorded.</p></details>"#,
);

/// The `selector-form` element up to its closing tag; panics when absent.
fn selector_form(html: &str) -> &str {
    let at = html
        .find(r#"<form class="selector-form""#)
        .unwrap_or_else(|| panic!("no selector form in {html}"));
    let rest = &html[at..];
    &rest[..rest.find("</form>").expect("form not closed") + 7]
}

fn run_line(body: &str) -> &str {
    let at = body
        .find(r#"<div id="run-line""#)
        .unwrap_or_else(|| panic!("no run-line in {body}"));
    let rest = &body[at..];
    &rest[..rest.find("</div>").expect("run-line not closed")]
}

#[tokio::test]
async fn c1_the_idle_form_holds_the_custom_selector_disclosure_after_the_submit_button() {
    let (app, _runs, _dir) = app().await;

    let (status, _, body) = send(&app, get("/")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let open = body
        .find(r#"<form class="query-input" action="/" method="get">"#)
        .expect("idle form");
    let form = &body[open..open + body[open..].find("</form>").expect("form closed")];
    assert!(form.contains(DETAILS), "{form}");
    let submit = form.find(r#"type="submit""#).expect("submit button");
    assert!(form.find(DETAILS).unwrap() > submit, "{form}");
}

#[tokio::test]
async fn c2_the_idle_redirect_keeps_the_list() {
    let (app, _runs, _dir) = app().await;

    let (status, headers, _) = send(&app, get("/?d=example.com&dkim_selectors=s1,s2")).await;

    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    let loc = headers.get(header::LOCATION).unwrap().to_str().unwrap();
    assert!(
        loc == "/example.com?dkim_selectors=s1%2Cs2" || loc == "/example.com?dkim_selectors=s1,s2",
        "{loc}"
    );
}

#[tokio::test]
async fn c3_the_result_page_email_panel_holds_the_selector_form_and_no_other_panel_does() {
    let (app, _runs, _dir) = app().await;

    let (status, _, body) = send(&app, get("/example.com")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let start = body.find(r#"id="evidence-email""#).expect("email panel");
    let end = body.find(r#"id="evidence-ip""#).expect("ip panel");
    assert!(start < end);
    let panel = &body[start..end];
    assert!(panel.contains(FORM_OPEN), "{panel}");
    let form = selector_form(panel);
    assert!(
        form.contains(r#"<label for="selector-form-input">Check custom DKIM selectors</label>"#),
        "{form}"
    );
    assert!(
        form.contains(r#"<input id="selector-form-input" type="text" name="dkim_selectors" placeholder="selector1, selector2">"#),
        "{form}"
    );
    assert!(
        form.contains(
            r#"<button class="button button--secondary" type="submit">Run with selectors</button>"#
        ),
        "{form}"
    );
    assert_eq!(
        body.matches("selector-form\"").count(),
        1,
        "one form only: {body}"
    );
    assert_eq!(body.matches(r#"<form class="selector-form""#).count(), 1);
}

#[tokio::test]
async fn c3_a_selector_page_prefills_the_normalised_list() {
    let (app, _runs, _dir) = app().await;

    let (status, _, body) = send(&app, get("/example.com?dkim_selectors=s1,%20s2")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let form = selector_form(&body);
    assert!(
        form.contains(r#"placeholder="selector1, selector2" value="s1,s2">"#),
        "{form}"
    );
}

#[tokio::test]
async fn c4_the_email_evidence_views_end_with_the_selector_form_and_dns_has_none() {
    let (app, _runs, _dir) = app().await;
    let (_, _, home) = send(&app, get("/example.com")).await;
    let _ = home;
    // The id of the stored entry, from the history route's latest entry.
    let id = {
        let (_, _, page) = send(&app, get("/example.com")).await;
        let at = page.find("/r/").expect("entry link on the page") + 3;
        page[at..at + 8].to_string()
    };

    for uri in ["/example.com/email".to_string(), format!("/r/{id}/email")] {
        let (status, _, body) = send(&app, get(&uri)).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        let form_at = body
            .find(r#"<form class="selector-form""#)
            .unwrap_or_else(|| panic!("{uri}: no form"));
        let main_end = body.find("</main>").expect("main closed");
        assert!(form_at < main_end, "{uri}");
        assert!(body.contains(FORM_OPEN), "{uri}: {body}");
        let section_end = body.find("</section>").expect("evidence section closed");
        assert!(
            form_at > section_end,
            "{uri}: form after the evidence section"
        );
    }

    let (status, _, body) = send(&app, get("/example.com/dns")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("selector-form"), "{body}");
}

#[tokio::test]
async fn c5_the_selector_page_run_line_names_the_selectors_and_that_it_is_not_recorded() {
    let (app, _runs, _dir) = app().await;

    let (status, _, body) = send(&app, get("/example.com?dkim_selectors=s1,s2")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let line = strip_tags(run_line(&body));
    assert!(line.contains("DKIM selectors s1, s2"), "{line}");
    assert!(line.contains("not recorded"), "{line}");
}

#[tokio::test]
async fn c6_the_selector_run_stream_run_line_names_the_selectors_and_a_plain_run_does_not() {
    let (app, _runs, _dir) = app().await;

    let (_, _, sel_body) = send(&app, post("trigger=recheck&dkim_selectors=s1,s2", true)).await;
    let (_, _, plain_body) = send(&app, post("trigger=recheck", true)).await;

    let lines: Vec<String> = sel_body
        .split("\n\n")
        .filter(|e| e.contains("run-line"))
        .map(|e| strip_tags(&e.replace("data:", " ")))
        .collect();
    assert!(!lines.is_empty(), "{sel_body}");
    assert!(
        lines.iter().any(|l| l.contains("DKIM selectors s1, s2")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("not recorded")),
        "{lines:?}"
    );
    assert!(!plain_body.contains("DKIM selectors"), "{plain_body}");
}

#[tokio::test]
async fn c7_a_bad_selector_list_has_its_own_title_and_sentence_and_a_bad_domain_keeps_its_own() {
    let (app, _runs, _dir) = app().await;

    let (status, _, body) = send(&app, get("/example.com?dkim_selectors=bad.sel")).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.contains("<title>Not a list of DKIM selectors - netray</title>"),
        "{body}"
    );
    assert!(
        strip_tags(&body).contains("A DKIM selector list is 1 to 5 names of letters, digits and hyphens, separated by commas."),
        "{body}"
    );

    let (status, _, body) = send(&app, get("/1.2.3.4")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.contains("<title>Not a domain name - netray</title>"),
        "{body}"
    );
}

/// A run backend that refuses every run with a per-address refusal.
struct RefusingRuns;

impl DomainRuns for RefusingRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.trim().to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async {
            RunStart::Refused(netray_web::api::RunRefusal {
                reason: netray_web::api::RefusalReason::Budget,
                retry_after_s: 30,
                last_run_ago_s: None,
                last_incomplete: false,
            })
        })
    }
}

/// Review F1: a refused selector run without the stream island keeps the refusal line on its
/// 429 page; the stored head is shown, but no run is announced.
#[tokio::test]
async fn c5_a_refused_selector_form_post_keeps_the_refusal_line() {
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
    let app = router(ResultState::new(
        Arc::new(store),
        Arc::new(RefusingRuns),
        300,
    ));
    let req = Request::post(format!("/{DOMAIN}/run"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("trigger=recheck&dkim_selectors=s1,s2"))
        .unwrap();

    let (status, _, body) = send(&app, req).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    let line = &body[body.find(r#"<div id="run-line">"#).expect("run line")..];
    let line = &line[..line.find("</div>").unwrap()];
    assert!(line.contains("too many checks from your address"), "{line}");
    assert!(!line.contains("until this run ends"), "{line}");
}
