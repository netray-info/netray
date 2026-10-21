//! v2-tls-ui Phase 3 (requirements 13 and 14; C1-C8): each TLS part carries its port's checks,
//! several ports render as tabs with one panel per port, a one-port run keeps the band under the h1.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn block(id: &str, title: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: title.into(),
        shape,
    }
}

fn addresses(port: u16, outcome: &str, message: &str) -> Block {
    block(
        &format!("tls.addresses@{port}"),
        "Addresses",
        Shape::Table {
            key: "address".into(),
            columns: ["address", "port", "outcome", "message"]
                .map(String::from)
                .to_vec(),
            rows: vec![vec![
                Value::Addr("192.0.2.1".parse().unwrap()),
                Value::Int(port as i64),
                text(outcome),
                text(message),
            ]],
        },
    )
}

fn cert() -> Vec<(String, Value)> {
    vec![
        ("subject".into(), text("CN=example.com")),
        ("issuer".into(), text("CN=Example Intermediate CA")),
        (
            "not_before".into(),
            Value::Time("2026-10-07T00:00:00Z".into()),
        ),
        (
            "not_after".into(),
            Value::Time("2027-01-06T00:00:00Z".into()),
        ),
        ("days_remaining".into(), Value::Int(89)),
        ("key".into(), text("ECDSA P-256")),
        ("signature_algorithm".into(), text("ecdsa-with-SHA256")),
        ("fingerprint_sha256".into(), text("4F:9A:00:11")),
        ("serial".into(), text("01AA")),
        ("self_signed".into(), Value::Bool(false)),
        ("expired".into(), Value::Bool(false)),
    ]
}

/// A minimal TLS port; `failed` swaps the handshake for a HANDSHAKE_FAILED row only in the
/// addresses table, keeping the chain so the trust text can carry the check's tone.
fn port_blocks(port: u16, starttls: Option<&str>) -> Vec<Block> {
    let mut hs = vec![("protocol".to_string(), text("TLSv1.3"))];
    if let Some(s) = starttls {
        hs.push(("starttls".into(), text(s)));
    }
    vec![
        addresses(port, "ok", ""),
        block(
            &format!("tls.chain@192.0.2.1:{port}"),
            "Chain",
            Shape::Chain(vec![cert()]),
        ),
        block(
            &format!("tls.handshake@192.0.2.1:{port}"),
            "Handshake",
            Shape::Fields(hs),
        ),
    ]
}

fn check(id: &str, status: Status) -> CheckResult {
    CheckResult {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: Vec::new(),
        evidence: Vec::new(),
    }
}

fn checks(failing: bool) -> Vec<CheckResult> {
    let s = if failing { Status::Fail } else { Status::Pass };
    vec![
        check("tls.tls_reachable", s),
        check("tls.chain_trusted", s),
        check("tls.chain_complete", Status::Pass),
    ]
}

fn part(label: &str, blocks: Vec<Block>) -> JobPart {
    JobPart {
        label: label.into(),
        output: Box::pin(async move { Ok(blocks) }),
    }
}

struct Three;
impl InspectorJobs for Three {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let job = Job::new(vec![
            part("port 443", port_blocks(443, None)),
            part("port 465", port_blocks(465, None)),
            part("port 587", port_blocks(587, Some("smtp"))),
        ]);
        Ok(job
            .with_part_checks(0, Box::pin(async { Ok(checks(false)) }))
            .with_part_checks(1, Box::pin(async { Ok(checks(true)) }))
            .with_part_checks(2, Box::pin(async { Ok(checks(false)) }))
            .with_band(Protocol::Tls, Box::pin(async { Ok(checks(false)) })))
    }
}

struct One;
impl InspectorJobs for One {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let job = Job::new(vec![part("handshake", port_blocks(443, None))]);
        Ok(job
            .with_part_checks(0, Box::pin(async { Ok(checks(false)) }))
            .with_band(Protocol::Tls, Box::pin(async { Ok(checks(false)) })))
    }
}

fn app(jobs: impl InspectorJobs) -> Router {
    inspect_router(InspectDeps::new(Arc::new(jobs), InspectConfig::default()))
}

async fn send(app: &Router, req: Request<Body>) -> String {
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).replace("\r\n", "\n")
}

async fn run(app: &Router) -> String {
    send(
        app,
        Request::post("/inspect/tls/run")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("h=example.com"))
            .unwrap(),
    )
    .await
}

fn fragments(stream: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in stream.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let data: String = raw
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(|l| l.trim_start())
            .collect();
        if let Ok(j) = serde_json::from_str::<serde_json::Value>(&data) {
            out.push((
                j["id"].as_str().unwrap_or_default().to_owned(),
                j["html"].as_str().unwrap_or_default().to_owned(),
            ));
        }
    }
    out
}

fn frag<'a>(frags: &'a [(String, String)], id: &str) -> &'a str {
    &frags
        .iter()
        .find(|(i, _)| i == id)
        .unwrap_or_else(|| panic!("no fragment {id}: {frags:?}"))
        .1
}

async fn shared_page(app: &Router, stream: &str) -> String {
    let marker = "data-share-key=\\\"";
    let a = stream.find(marker).expect("no share key") + marker.len();
    let key = &stream[a..a + 12];
    send(
        app,
        Request::get(format!("/inspect/tls?r={key}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

fn tab(html: &str, port: u16) -> &str {
    let a = html
        .find(&format!(r#"data-tab="{port}""#))
        .unwrap_or_else(|| panic!("no tab {port}: {html}"));
    let start = html[..a].rfind("<a ").unwrap();
    let end = a + html[a..].find("</a>").unwrap();
    &html[start..end]
}

#[tokio::test]
async fn c1_c2_c3_three_ports_render_tabs_and_panels_with_their_own_bands() {
    let app = app(Three);
    let stream = run(&app).await;
    let frags = fragments(&stream);
    let start = &frags[0].1;

    assert!(
        start.contains(r#"class="inspector-ports" data-island="tabs""#),
        "{start}"
    );
    assert!(!start.contains(r#"id="inspect-band""#), "{start}");
    for (n, port) in [(0, 443), (1, 465), (2, 587)] {
        let t = tab(start, port);
        assert!(t.contains(&format!(r##"href="#inspect-part-{n}""##)), "{t}");
        assert!(
            t.contains(&format!(
                r#"<span class="inspector-ports__name">{port}</span>"#
            )),
            "{t}"
        );
        assert_eq!(t.contains(r#"aria-current="true""#), port == 443, "{t}");
        assert!(
            start.contains(&format!(r#"id="inspect-part-{n}""#)),
            "{start}"
        );
        assert!(
            start.contains(&format!(r#"data-panel="{port}""#)),
            "{start}"
        );

        let panel = frag(&frags, &format!("inspect-part-{n}"));
        assert!(
            panel.contains(&format!(r#"id="inspect-band-{n}""#)),
            "{panel}"
        );
        assert!(
            panel.contains(&format!(r#"id="inspect-checks-{n}""#)),
            "{panel}"
        );
        assert!(!panel.contains(r#"id="inspect-band""#), "{panel}");
    }
    let meta587 = frag(&frags, "inspect-port-meta-2");
    assert!(meta587.contains("· STARTTLS"), "{meta587}");
    assert!(!frag(&frags, "inspect-port-meta-0").contains("STARTTLS"));
}

#[tokio::test]
async fn c4_a_failing_port_carries_the_fail_tone_and_counts_on_its_tab_only() {
    let stream = run(&app(Three)).await;
    let frags = fragments(&stream);
    let m465 = frag(&frags, "inspect-port-meta-1");
    assert!(m465.contains(r#"data-tone="fail""#), "{m465}");
    assert!(m465.contains("2 fail"), "{m465}");
    let m443 = frag(&frags, "inspect-port-meta-0");
    assert!(!m443.contains(r#"data-tone="fail""#), "{m443}");
    assert!(!m443.contains("fail"), "{m443}");
}

#[tokio::test]
async fn c5_a_one_port_run_has_no_tab_row_and_one_band_under_the_h1() {
    let stream = run(&app(One)).await;
    let start = &fragments(&stream)[0].1;
    assert!(!start.contains("inspector-ports"), "{start}");
    assert_eq!(start.matches(r#"id="inspect-band""#).count(), 1, "{start}");
}

#[tokio::test]
async fn c6_a_part_carries_its_own_ports_tones_and_is_not_resent_at_band_end() {
    let stream = run(&app(Three)).await;
    let frags = fragments(&stream);
    let p465 = frag(&frags, "inspect-part-1");
    assert!(
        p465.contains(r#"<span class="tls-chain__trust" data-tone="fail">"#),
        "{p465}"
    );
    let p443 = frag(&frags, "inspect-part-0");
    assert!(
        p443.contains(r#"<span class="tls-chain__trust" data-tone="pass">"#),
        "{p443}"
    );
    for n in 0..3 {
        let id = format!("inspect-part-{n}");
        assert_eq!(
            frags.iter().filter(|(i, _)| *i == id).count(),
            1,
            "{id}: {stream}"
        );
    }
}

#[tokio::test]
async fn c7_c8_a_three_port_share_key_renders_all_panels_in_order_without_javascript() {
    let app = app(Three);
    let stream = run(&app).await;
    let page = shared_page(&app, &stream).await;

    assert!(!page.contains(r#"id="inspect-band""#), "{page}");
    let mut last = 0;
    for n in 0..3 {
        let at = page
            .find(&format!(r#"id="inspect-part-{n}""#))
            .unwrap_or_else(|| panic!("no panel {n}: {page}"));
        assert!(at > last, "panel {n} out of order");
        last = at;
        assert!(
            page.contains(&format!(r#"id="inspect-band-{n}""#)),
            "{page}"
        );
        assert!(
            page.contains(&format!(r#"id="inspect-checks-{n}""#)),
            "{page}"
        );
        assert!(
            page.contains(&format!(r##"href="#inspect-part-{n}""##)),
            "{page}"
        );
        let open = page[..page.find(&format!(r#"id="inspect-part-{n}""#)).unwrap()]
            .rfind("<section")
            .unwrap();
        let tag = &page[open..page[open..].find('>').unwrap() + open];
        assert!(!tag.contains("hidden"), "{tag}");
    }
    let p465 = &page[page.find(r#"id="inspect-part-1""#).unwrap()
        ..page.find(r#"id="inspect-part-2""#).unwrap()];
    assert!(
        p465.contains(r#"<span class="tls-chain__trust" data-tone="fail">"#),
        "{p465}"
    );
    assert!(page.contains(r#"data-tone="fail""#), "{page}");
}
