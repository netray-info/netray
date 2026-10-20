//! Result fidelity, Phase 7 (spec v2-result-fidelity, requirement 41): the Share control opens a
//! dialog with the snapshot link, the re-run link, the badge snippets and the social card, all on
//! the canonical origin; an incomplete run and a never-checked domain carry neither.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";
const ORIGIN: &str = "https://netray.info";

struct StubRuns {
    incomplete: bool,
}

impl DomainRuns for StubRuns {
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
        if !self.incomplete {
            return Box::pin(async { RunStart::NotStarted });
        }
        let events = vec![
            RunEvent::Done(run(false)),
            RunEvent::Recorded(Some(Recorded::NotStored { diff: None })),
        ];
        Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
    }
}

fn run(complete: bool) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: if complete { Grade::B } else { Grade::Incomplete },
        complete,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.caa").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

async fn app(incomplete: bool, store_entry: bool) -> (Router, Option<String>) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    std::mem::forget(dir);
    let mut id = None;
    if store_entry {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let rec = store
            .record(DOMAIN, &run(true), &[], now - 60, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
        id = match rec {
            Recorded::First { id } => Some(id),
            other => panic!("unexpected {other:?}"),
        };
    }
    let router = router(ResultState::new(
        Arc::new(store),
        Arc::new(StubRuns { incomplete }),
        300,
    ));
    (router, id)
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

async fn get_page(app: Router, path: &str) -> String {
    let resp = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned()
}

#[tokio::test]
async fn c1_c2_c4_c7_a_stored_entry_offers_the_share_dialog() {
    let (app, id) = app(false, true).await;
    let id = id.unwrap();
    let page = get_page(app, &format!("/{DOMAIN}")).await;

    // the dialog uses the origin the canonical link names
    let canonical = format!(r#"<link rel="canonical" href="{ORIGIN}/{DOMAIN}">"#);
    assert!(page.contains(&canonical), "{page}");

    let share = &page[page.find(r#"<div id="result-share">"#).expect("result-share")..];
    let share = &share[..share.find("</p>").unwrap_or(share.len())];
    let link = format!(
        r#"<a id="share" class="button button--secondary" href="/r/{id}" data-island="share" aria-haspopup="dialog">Share</a>"#
    );
    assert!(share.contains(&link), "{share}");

    let start = page
        .find(r#"<dialog id="share-dialog" class="share-dialog" aria-labelledby="share-dialog-title">"#)
        .expect("share dialog");
    let dialog = &page[start..];
    let dialog = &dialog[..dialog.find("</dialog>").expect("dialog end")];
    assert!(
        page[..start].rfind(r#"<div id="result-share">"#).is_some()
            && !page[page.find(r#"<div id="result-share">"#).unwrap()..start].contains("</div>")
            || share.contains("<dialog"),
        "the dialog sits inside #result-share"
    );
    assert!(
        dialog.contains(r#"<h2 id="share-dialog-title">Share and embed</h2>"#),
        "{dialog}"
    );
    assert!(
        dialog.contains(r#"<button type="button" class="share-dialog__close" aria-label="Close">"#),
        "{dialog}"
    );
    assert_eq!(dialog.matches(r#"role="tablist""#).count(), 1, "{dialog}");
    assert_eq!(dialog.matches(r#"role="tab""#).count(), 4, "{dialog}");
    assert_eq!(dialog.matches(r#"role="tabpanel""#).count(), 4, "{dialog}");
    for label in ["Snapshot", "Re-run link", "Badge", "Social card"] {
        assert!(dialog.contains(&format!(">{label}</button>")), "{label}: {dialog}");
    }
    assert!(dialog.matches("share-dialog__copy").count() >= 5, "{dialog}");
    assert!(dialog.matches("data-copy=\"").count() >= 5, "{dialog}");

    let text = unescape(dialog);
    let md = |style: &str| {
        format!("[![netray grade]({ORIGIN}/badge/{DOMAIN}.svg{style})]({ORIGIN}/{DOMAIN})")
    };
    let html = |style: &str| {
        format!(
            r#"<a href="{ORIGIN}/{DOMAIN}"><img src="{ORIGIN}/badge/{DOMAIN}.svg{style}" alt="netray grade"></a>"#
        )
    };
    let ftb = "?style=for-the-badge";
    let wants = [
        format!("{ORIGIN}/r/{id}"),
        format!("{ORIGIN}/{DOMAIN}"),
        format!("{ORIGIN}/r/{id}/og.png"),
        md(""),
        html(""),
        md(ftb),
        html(ftb),
        r#"<img class="share-dialog__badge" src="/badge/example.com.svg""#.to_string(),
        format!(r#"<img class="share-dialog__card" src="/r/{id}/og.png" alt=""#),
        r#"name="badge-style""#.to_string(),
        r#"value="flat""#.to_string(),
        r#"value="for-the-badge""#.to_string(),
        "data-markdown=\"".to_string(),
        "data-html=\"".to_string(),
    ];
    for want in wants {
        assert!(text.contains(&want), "missing {want}\n{text}");
    }
    assert!(
        text.contains("readonly"),
        "fields are read-only: {text}"
    );
}

#[tokio::test]
async fn c8_an_incomplete_run_and_a_never_checked_domain_carry_no_share() {
    // incomplete live run: the end fragment holds no link and no dialog
    let (app1, _) = app(true, false).await;
    let resp = app1
        .oneshot(
            Request::post(format!("/{DOMAIN}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::from("trigger=visit"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut share_html = None;
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let data: String = raw
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect();
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        if j["id"] == "result-share" {
            share_html = j["html"].as_str().map(str::to_owned);
        }
    }
    let share = share_html.expect("a result-share fragment");
    assert!(share.contains(r#"id="result-share""#), "{share}");
    for banned in ["<a", "<dialog", "share-dialog", "/r/"] {
        assert!(!share.contains(banned), "{banned}: {share}");
    }

    // never-checked domain
    let (app2, _) = app(false, false).await;
    let page = get_page(app2, "/example.com").await;
    for banned in ["share-dialog", r#"id="share""#, r#"href="/r/"#] {
        assert!(!page.contains(banned), "{banned}: {page}");
    }
}
