//! Spec v2-ip-ui, requirement 14, Phase 3 C5: the IP protocol view claims `ip.addresses` and
//! renders the generic evidence block followed by the GeoIP attribution paragraph.
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::{Block, Value};
use netray_model::run::Run;
use netray_store::HistoryStore;
use netray_web::pages::views::{ViewContext, render};
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";
const T: i64 = 1_800_000_000;
const ATTRIBUTION: &str = r#"class="geoip-attribution""#;

fn run() -> Run {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/shop.example.com-3.json");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    serde_json::from_value(v["run"].clone()).unwrap()
}

fn block_of(run: &Run, id: &str) -> Block {
    run.blocks
        .iter()
        .find(|b| b.id.to_string() == id)
        .unwrap_or_else(|| panic!("fixture lacks {id}"))
        .clone()
}

fn view(block: &Block) -> Option<String> {
    let decorate = |_: &_, _: &str, _: &Value| None;
    render(
        block,
        &ViewContext {
            statuses: &[],
            decorate: &decorate,
            feeds: &[],
        },
    )
}

#[test]
fn c5a_the_ip_view_claims_addresses_with_generic_markup_then_attribution() {
    let html = view(&block_of(&run(), "ip.addresses")).expect("ip view must claim ip.addresses");
    let end = html.find("</section>").expect("section close");
    let at = html
        .find(ATTRIBUTION)
        .expect("attribution paragraph missing");
    assert!(html.contains(r#"id="ev-ip.addresses""#), "{html}");
    assert!(html.contains(r#"class="evidence-block""#), "{html}");
    assert!(html.contains("evidence-block__table"), "{html}");
    assert!(at > end, "attribution must follow </section>: {html}");
    assert!(html.contains(r#"href="https://www.maxmind.com""#), "{html}");
    assert!(html.contains("MaxMind"), "{html}");
}

#[test]
fn c5a_a_non_ip_block_gets_no_attribution() {
    let r = run();
    let dns = r
        .blocks
        .iter()
        .find(|b| b.id.to_string().starts_with("dns."))
        .expect("fixture has a dns block");
    if let Some(html) = view(dns) {
        assert!(!html.contains(ATTRIBUTION), "{html}");
    }
}

#[tokio::test]
async fn c5b_the_ip_evidence_page_puts_attribution_after_the_addresses_block() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record(DOMAIN, &run(), &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app = evidence_router(EvidenceDeps {
        history: Arc::new(store),
        v1: None,
    });
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}/ip"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    let ev = body
        .find(r#"id="ev-ip.addresses""#)
        .expect("ev-ip.addresses missing");
    let rest = &body[ev..];
    let at = rest
        .find(ATTRIBUTION)
        .expect("attribution missing after ev-ip.addresses");
    assert!(rest[..at].contains("</section>"), "{rest}");
}
