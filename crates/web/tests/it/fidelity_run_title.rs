//! The run title of a stored entry (spec v2-result-fidelity, requirements 6 and 9; C6, C17, C18,
//! C20): `GET /{domain}` through `netray_web::pages::result::router`.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const TTL: u64 = 300;
// 2020-09-13 12:26:40 UTC, years before any cache window; entry i is recorded i days later.
const OLD: i64 = 1_600_000_000;
const DAY: i64 = 86_400;

struct StubRuns;

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
        Box::pin(async { RunStart::NotStarted })
    }
}

/// The first `n` check ids of the catalogue, read from the file.
fn catalogue_ids(n: usize) -> Vec<String> {
    let toml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine/catalogue/checks.toml"
    ))
    .unwrap();
    let ids: Vec<String> = toml
        .lines()
        .filter_map(|l| l.strip_prefix("id = \""))
        .map(|l| l.trim_end_matches('"').to_string())
        .take(n)
        .collect();
    assert_eq!(ids.len(), n);
    ids
}

/// A run of `n` checks whose status flips with `flip`, so neighbours always differ.
fn run(n: usize, flip: usize) -> Run {
    let status = if flip % 2 == 0 {
        Status::Pass
    } else {
        Status::Warn
    };
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: catalogue_ids(n)
            .iter()
            .map(|id| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// One entry: `(checks, recorded at, duration)`; `None` duration goes through `record`.
type Spec = (usize, i64, Option<u64>);

async fn render(entries: &[Spec]) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for (i, (checks, at, duration)) in entries.iter().enumerate() {
        let r = run(*checks, i);
        let d = |p: &Run, c: &Run| diff(p, c, &[], None);
        match duration {
            None => store.record(DOMAIN, &r, &[], *at, d).await.unwrap(),
            Some(ms) => store
                .record_timed(DOMAIN, &r, &[], *at, Some(*ms), d)
                .await
                .unwrap(),
        };
    }
    let id = store.latest(DOMAIN).await.unwrap().unwrap().id;
    let app: Router = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), TTL));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (String::from_utf8_lossy(&bytes).into_owned(), id)
}

/// The text parts of `<p class="run-line run-line--title">`.
fn title_parts(body: &str) -> Vec<String> {
    let p = body
        .split("<p class=\"run-line run-line--title\">")
        .nth(1)
        .unwrap_or_else(|| panic!("no run title in {body}"));
    let p = p.split("</p>").next().unwrap();
    p.split("run-line__part\"")
        .skip(1)
        .map(|s| {
            s[s.find('>').unwrap() + 1..]
                .split("</span>")
                .next()
                .unwrap()
                .to_string()
        })
        .collect()
}

#[tokio::test]
async fn c6_run_title_parts_in_order_without_the_entry_id() {
    // One fresh entry with a duration: every optional part, in order.
    let (body, id) = render(&[(2, now() - 60, Some(2000))]).await;
    let parts = title_parts(&body);
    assert_eq!(parts.len(), 7, "{parts:?}");
    assert_eq!(parts[0], "Run 1");
    assert!(
        parts[1].len() == "2020-09-13 12:26 UTC".len() && parts[1].ends_with(" UTC"),
        "{parts:?}"
    );
    assert_eq!(
        parts[2..],
        ["2 checks", "profile 3", "2.0 s", "cached", "first run"]
    );
    let title = body
        .split("<p class=\"run-line run-line--title\">")
        .nth(1)
        .unwrap()
        .split("</p>")
        .next()
        .unwrap();
    assert!(!title.contains(&id), "entry id in the title: {title}");

    let css = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/static/run_line.css"))
        .unwrap();
    let rule = css
        .split(".run-line--title")
        .nth(1)
        .unwrap_or_else(|| panic!("no .run-line--title rule in {css}"))
        .split('}')
        .next()
        .unwrap();
    assert!(rule.contains("text-transform: uppercase"), "{rule}");
    assert!(
        rule.contains("letter-spacing: .12em")
            || rule.contains("letter-spacing: 0.12em")
            || rule.contains("letter-spacing: var(--font-tracking-caps)"),
        "{rule}"
    );
    assert!(
        rule.contains("font-size: 11px") || rule.contains("font-size: var(--font-size-caption)"),
        "{rule}"
    );
    assert!(rule.contains("font-family: var(--font-mono)"), "{rule}");
}

#[tokio::test]
async fn c17_twelfth_entry_title_reads_run_checks_profile_duration() {
    let mut entries: Vec<Spec> = (0..11).map(|i| (2, OLD + i * DAY, None)).collect();
    entries.push((40, OLD + 11 * DAY, Some(4210)));

    let (body, _) = render(&entries).await;

    assert_eq!(
        title_parts(&body),
        [
            "Run 12",
            "2020-09-24 12:26 UTC",
            "40 checks",
            "profile 3",
            "4.2 s"
        ]
    );
}

#[tokio::test]
async fn c18_entry_without_duration_has_no_seconds_part() {
    let entries: Vec<Spec> = (0..2).map(|i| (2, OLD + i * DAY, None)).collect();

    let (body, _) = render(&entries).await;

    let parts = title_parts(&body);
    assert_eq!(parts[0], "Run 2", "{parts:?}");
    assert!(parts.iter().all(|p| !p.ends_with(" s")), "{parts:?}");
}

#[tokio::test]
async fn c20_first_entry_title_ends_with_first_run() {
    let (body, _) = render(&[(2, OLD, Some(1500))]).await;

    let parts = title_parts(&body);
    assert_eq!(parts.last().unwrap(), "first run", "{parts:?}");
    assert_eq!(parts[0], "Run 1");
}
