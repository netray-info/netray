//! v2-tls-ui Phase 4, requirement 15 (C1 and C2 server half, C3): `GET /inspect/tls` renders the
//! hidden port chooser in the form card, `data-max-ports` follows the job seam, no other inspector
//! renders one, and the TLS help names the `host:443,465` syntax.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

struct Stub {
    max: Option<usize>,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        Err("stub".into())
    }
    fn max_ports(&self) -> usize {
        self.max.unwrap_or(7)
    }
}

async fn page(max: Option<usize>, tool: &str) -> String {
    let app: Router = inspect_router(InspectDeps::new(
        Arc::new(Stub { max }),
        InspectConfig::default(),
    ));
    let res = app
        .oneshot(
            Request::get(format!("/inspect/{tool}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).replace("\r\n", "\n")
}

fn chooser(html: &str) -> &str {
    let a = html
        .find(r#"<div class="port-chooser" data-island="ports" data-max-ports="#)
        .unwrap_or_else(|| panic!("no port chooser: {html}"));
    let rest = &html[a..];
    let end = rest.find("</template>").expect("no template") + "</template>".len();
    &rest[..end]
}

#[tokio::test]
async fn c1_tls_page_holds_the_hidden_chooser_between_box_and_submit() {
    let html = page(None, "tls").await;
    let open = r#"<div class="port-chooser" data-island="ports" data-max-ports="7" hidden>"#;
    let at = html
        .find(open)
        .unwrap_or_else(|| panic!("no chooser opening tag: {html}"));
    let form = html
        .find("query-input--card")
        .expect("no query-input--card form");
    let bx = html.find("query-input__box").expect("no query-input__box");
    let submit = html[bx..]
        .find("query-input__submit")
        .expect("no query-input__submit")
        + bx;
    assert!(form < bx && bx < at && at < submit, "order: {html}");

    let c = chooser(&html);
    for want in [
        r#"<span class="port-chooser__label">Ports</span>"#,
        r#"data-port="443" aria-pressed="true""#,
        r#"data-port="8443" aria-pressed="false""#,
        r#"aria-label="Add port""#,
        r#"<p class="port-chooser__note" hidden>"#,
    ] {
        assert!(c.contains(want), "missing {want:?} in {c}");
    }
}

#[tokio::test]
async fn c2_chooser_presets_and_limit() {
    let html = page(None, "tls").await;
    let c = chooser(&html);
    for (ports, name) in [
        ("443", "HTTPS"),
        ("25,465,587,993,995", "E-Mail"),
        ("443,25,465,587,993,995,8443", "All common"),
    ] {
        let want = format!(r#"data-ports="{ports}">{name}</button>"#);
        assert!(c.contains(&want), "missing {want:?} in {c}");
    }

    let three = page(Some(3), "tls").await;
    assert!(
        three.contains(
            r#"<div class="port-chooser" data-island="ports" data-max-ports="3" hidden>"#
        ),
        "{three}"
    );
}

#[tokio::test]
async fn c3_other_inspectors_have_no_chooser_and_tls_help_names_the_syntax() {
    for tool in ["dns", "http", "email", "ip"] {
        let html = page(None, tool).await;
        assert!(!html.contains("port-chooser"), "{tool} renders a chooser");
    }
    let html = page(None, "tls").await;
    let a = html.find(r#"id="inspector-help""#).expect("no help dialog");
    let help = &html[a..a + html[a..].find("</dialog>").expect("unclosed dialog")];
    assert!(help.contains("host:443,465"), "{help}");
}
