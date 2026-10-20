//! Result polish, Phase 1 (spec v2-result-polish, requirements 8-9; C8, C9, C17, C18): the email
//! and IP panels open on a compact table of their records and addresses.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::run::Run;
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";
const T: i64 = 1_800_000_000;

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

/// The shop fixture's run as JSON, so a scenario edits blocks as the store reads them.
fn fixture_json() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/shop.example.com-3.json");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    v["run"].clone()
}

fn run_of(v: serde_json::Value) -> Run {
    serde_json::from_value(v).unwrap()
}

async fn page(run: &Run) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record(DOMAIN, run, &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600));
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
    String::from_utf8_lossy(&bytes).into_owned()
}

fn panel<'a>(body: &'a str, protocol: &str) -> &'a str {
    let at = body.find(r#"id="evidence""#).expect("#evidence missing");
    let ev = &body[at..];
    let open = format!(
        r#"<div id="evidence-{protocol}" class="evidence-tabs__panel" data-panel="{protocol}""#
    );
    let at = ev
        .find(&open)
        .unwrap_or_else(|| panic!("panel `{open}` missing: {ev}"));
    let rest = &ev[at + open.len()..];
    let end = rest
        .find(r#"class="evidence-tabs__panel""#)
        .unwrap_or(rest.len());
    &rest[..end]
}

fn caption(text: &str) -> String {
    format!(r#"<p class="evidence-tabs__caption">{text}</p>"#)
}

/// Asserts the needles occur in `hay` in the given order.
fn in_order(hay: &str, needles: &[&str]) {
    let mut at = 0;
    for n in needles {
        at += hay[at..]
            .find(n)
            .unwrap_or_else(|| panic!("`{n}` missing or out of order in: {hay}"))
            + n.len();
    }
}

#[tokio::test]
async fn c8_the_email_panel_lists_the_mail_records() {
    let body = page(&run_of(fixture_json())).await;
    let email = panel(&body, "email");
    assert!(
        email.contains(&caption("Mail records \u{b7} 1 MX host")),
        "{email}"
    );
    assert!(
        email.contains(r#"<table class="evidence-tabs__compact">"#),
        "{email}"
    );
    assert!(email.contains("<th>Record</th><th>Value</th>"), "{email}");
    in_order(
        email,
        &[
            "<td>MX</td><td>10 mx1.shop.example.com.</td>",
            "<td>SPF</td><td>v=spf1 mx -all</td>",
            "<td>DMARC</td><td>v=DMARC1;p=reject;sp=reject;adkim=s;aspf=s</td>",
            "<td>MTA-STS</td><td>v=STSv1; id=20261018</td>",
            "<td>TLS-RPT</td><td>v=TLSRPTv1; rua=mailto:tlsrpt@shop.example.com</td>",
            "<td>DKIM</td><td>mail: valid, rsa 2048</td>",
        ],
    );
    assert!(
        !email.contains("<td>BIMI</td>"),
        "empty BIMI block: {email}"
    );
}

#[tokio::test]
async fn c17_a_sparse_email_run_keeps_the_present_rows_in_order() {
    let mut v = fixture_json();
    let keep = ["email.mx", "email.spf", "email.dmarc", "email.dkim"];
    v["blocks"].as_array_mut().unwrap().retain(|b| {
        let id = b["id"].as_str().unwrap();
        !id.starts_with("email.") || keep.contains(&id)
    });
    for c in v["checks"].as_array_mut().unwrap() {
        if let Some(refs) = c["evidence"].as_array_mut() {
            refs.retain(|r| {
                let id = r.as_str().unwrap();
                !id.starts_with("email.") || keep.contains(&id)
            });
        }
    }
    let body = page(&run_of(v)).await;
    let email = panel(&body, "email");
    assert!(
        email.contains(&caption("Mail records \u{b7} 1 MX host")),
        "{email}"
    );
    in_order(
        email,
        &[
            "<td>MX</td>",
            "<td>SPF</td>",
            "<td>DMARC</td><td>v=DMARC1;p=reject;sp=reject;adkim=s;aspf=s</td>",
            "<td>DKIM</td><td>mail: valid, rsa 2048</td>",
        ],
    );
    for absent in ["<td>MTA-STS</td>", "<td>TLS-RPT</td>", "<td>BIMI</td>"] {
        assert!(!email.contains(absent), "{absent} present: {email}");
    }
}

/// Two address rows, tor set on the first.
fn ip_run() -> Run {
    let mut v = fixture_json();
    let block = v["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["id"] == "ip.addresses")
        .expect("ip.addresses missing");
    let rows = block["shape"]["data"]["rows"].as_array_mut().unwrap();
    rows.truncate(2);
    // columns: address asn org prefix network_type country city spamhaus c2 tor vpn
    rows[0][9] = serde_json::json!({"t": "bool", "v": true});
    run_of(v)
}

fn row_of<'a>(panel: &'a str, address: &str) -> &'a str {
    panel
        .split("<tr>")
        .find(|r| {
            r.contains(&format!("<td>{address}</td>"))
                || r.contains(&format!("<strong>{address}</strong>"))
        })
        .map(|r| r.split("</tr>").next().unwrap_or(r))
        .unwrap_or_else(|| panic!("row {address} missing: {panel}"))
}

#[tokio::test]
async fn c9_c18_the_ip_panel_lists_addresses_with_their_listings() {
    let body = page(&ip_run()).await;
    let ip = panel(&body, "ip");
    assert!(
        ip.contains(&caption("Addresses \u{b7} 2 addresses")),
        "{ip}"
    );
    assert!(
        ip.contains(
            "<th>Address</th><th>ASN</th><th>Org</th><th>Country</th><th>Network</th><th>Listed</th>"
        ),
        "{ip}"
    );
    let listed = row_of(ip, "192.0.2.10");
    assert!(listed.ends_with("<td>tor</td>"), "{listed}");
    assert!(
        listed.contains("<td>AS64500</td><td>Example Networks</td><td>US</td><td>cloud</td>"),
        "{listed}"
    );
    let clean = row_of(ip, "198.51.100.10");
    assert!(clean.ends_with("<td>none</td>"), "{clean}");
}
