//! The since block of the result page (spec v2-result-fidelity, requirements 25, 26, 29; C6, C7,
//! C16, C17): `GET /{domain}` through `netray_web::pages::result::router`.
//!
//! The line code is `netray_web::pages::result::views::line_code(&CheckId) -> Option<String>`.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::views::line_code;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

// 2027-01-15 08:00:00 UTC; entry 1 is recorded a day later.
const T: i64 = 1_800_000_000;
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

/// Entry 0: grade C, dkim warn. Entry 1: grade B, dkim pass.
fn run(i: usize) -> Run {
    let (grade, dkim) = if i == 0 {
        (Grade::C, Status::Warn)
    } else {
        (Grade::B, Status::Pass)
    };
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: vec![
            RunSection {
                protocol: Protocol::Dns,
                outcome: SectionKind::Measured,
                reason: None,
            },
            RunSection {
                protocol: Protocol::Email,
                outcome: SectionKind::Measured,
                reason: None,
            },
        ],
        checks: [("dns.dnssec_valid", Status::Pass), ("email.dkim", dkim)]
            .iter()
            .map(|(id, status)| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status: *status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

async fn page() -> (String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for i in 0..2 {
        store
            .record(
                "shop.example.com",
                &run(i),
                &[],
                T + i as i64 * DAY,
                |p, c| diff(p, c, &[], None),
            )
            .await
            .unwrap();
    }
    let app: Router = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600));
    let resp = app
        .oneshot(
            Request::get("/shop.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (String::from_utf8_lossy(&bytes).into_owned(), dir)
}

/// 1-based position of `id` in the spectrum's line order, from the catalogue.
fn expected_code(id: &str) -> String {
    let mut n = 0;
    for p in [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ] {
        for spec in catalogue().for_protocol(p).entries() {
            n += 1;
            if spec.id.to_string() == id {
                let tag = match p {
                    Protocol::Dns => "DNS",
                    Protocol::Tls => "TLS",
                    Protocol::Http => "HTTP",
                    Protocol::Email => "MAIL",
                    Protocol::Ip => "IP",
                };
                return format!("{tag}\u{b7}{n:02}");
            }
        }
    }
    panic!("{id} not in the catalogue");
}

#[test]
fn c17_line_code_is_protocol_tag_and_two_digit_line_position() {
    for id in ["dns.dnssec_valid", "tls.chain_trusted", "email.dkim"] {
        assert_eq!(
            line_code(&CheckId::parse(id).unwrap()),
            Some(expected_code(id)),
            "{id}"
        );
    }
}

#[tokio::test]
async fn c6_c16_since_heading_is_followed_by_meta_and_the_box_has_no_title() {
    let (body, _dir) = page().await;

    assert!(
        body.contains("<h2>Since run 1</h2>"),
        "no since heading: {body}"
    );
    assert!(
        body.contains("<p class=\"since-meta\">2027-01-15 \u{b7} grade C \u{2192} B</p>"),
        "no since meta: {body}"
    );
    assert!(!body.contains("diff-block__title"), "{body}");
    assert!(!body.contains("diff-block__summary"), "{body}");
}

#[tokio::test]
async fn c7_c17_fixed_row_carries_marker_kind_text_and_line_code() {
    let (body, _dir) = page().await;

    let entry = body
        .split("<li class=\"diff-block__entry\"")
        .nth(1)
        .unwrap_or_else(|| panic!("no diff entry: {body}"))
        .split("</li>")
        .next()
        .unwrap();
    assert!(
        entry.contains("<span class=\"diff-block__marker\""),
        "{entry}"
    );
    assert!(entry.contains("diff-block__kind\">fixed<"), "{entry}");
    assert!(entry.contains("diff-block__text"), "{entry}");
    let code = format!(
        "<span class=\"diff-block__code\">{}</span>",
        expected_code("email.dkim")
    );
    assert!(entry.contains(&code), "want {code} in {entry}");
}
