// Contract test: lens parses the golden its DNS backend writes; the TLS module (`netray_tls`)
// and `netray_http` translate the TLS and HTTP goldens (both sections run in-process).
//
// Each backend's own `tests/contract_golden.rs` writes its golden under
// `tests/fixtures/contracts/` from its real response types. Here a local HTTP server
// serves the DNS golden at the path and method lens calls, and lens's public `check_dns` must
// produce checks and no error; the TLS module runs the tlsight golden through lens's
// `ModuleSection`. Both must carry values that come from the golden rather than defaults.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

mod common;

use axum::Router;
use axum::http::header;
use axum::routing::post;
use common::{run_tls, tls_golden};
use lens::backends::BackendExtra;
use lens::backends::dns::check_dns;
use lens::scoring::engine::CheckVerdict;
use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::translate;
use netray_model::Status;

const TIMEOUT: Duration = Duration::from_secs(5);

fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()))
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn lens_parses_prism_golden() {
    let body = golden("prism.sse");
    let app = Router::new().route(
        "/api/check",
        post(move || {
            let body = body.clone();
            async move { ([(header::CONTENT_TYPE, "text/event-stream")], body) }
        }),
    );
    let url = serve(app).await;

    let result = check_dns(
        &reqwest::Client::new(),
        &url,
        "example.com",
        &[],
        TIMEOUT,
        &reqwest::header::HeaderMap::new(),
    )
    .await
    .expect("lens must parse prism's golden without a section error");

    assert!(
        !result.checks.is_empty(),
        "no checks parsed from prism's golden"
    );
    let ns = result
        .checks
        .iter()
        .find(|c| c.name == "ns")
        .expect("lint finding `ns` missing");
    assert_eq!(ns.verdict, CheckVerdict::Pass);
    let caa = result
        .checks
        .iter()
        .find(|c| c.name == "caa")
        .expect("lint finding `caa` missing");
    assert_eq!(caa.verdict, CheckVerdict::Warn);
    assert!(
        result
            .resolved_ips
            .iter()
            .any(|ip| ip.to_string() == "192.0.2.10"),
        "resolved IPs {:?} do not come from the golden's A batch",
        result.resolved_ips
    );
}

#[tokio::test]
async fn lens_parses_tlsight_golden() {
    let result = run_tls(tls_golden("tlsight-inspect.json"), TIMEOUT)
        .await
        .expect("lens must run the TLS module on tlsight's golden without a section error");

    assert!(
        !result.checks.is_empty(),
        "no checks translated from tlsight's golden"
    );
    let ocsp = result
        .checks
        .iter()
        .find(|c| c.name == "ocsp_stapled")
        .expect("port quality check `ocsp_stapled` missing");
    assert_eq!(ocsp.verdict, CheckVerdict::Fail);
    // HTTP owns hsts and https_redirect; the TLS section no longer copies tlsight's
    // hostname checks (grade-integrity requirement 9).
    for owned_by_http in ["hsts", "https_redirect"] {
        assert!(
            result.checks.iter().all(|c| c.name != owned_by_http),
            "TLS section must not carry `{owned_by_http}`"
        );
    }
    let BackendExtra::Tls { raw_headline, .. } = &result.extra else {
        panic!("expected BackendExtra::Tls");
    };
    assert!(
        raw_headline.contains("TLSv1.3") && raw_headline.contains("60d"),
        "headline `{raw_headline}` does not carry the golden's version and days remaining",
    );
}

#[test]
fn netray_http_translates_spectra_golden() {
    let resp: InspectResponse = serde_json::from_str(&golden("spectra-inspect.json"))
        .expect("netray-http must decode spectra's golden");

    let (checks, presentation) = match translate(&resp) {
        SectionOutcome::Measured {
            checks,
            presentation,
        } => (checks, presentation),
        other => panic!("expected Measured, got {other:?}"),
    };

    assert!(
        !checks.is_empty(),
        "no checks translated from spectra's golden"
    );
    let check = |id: &str| {
        checks
            .iter()
            .find(|c| c.id.to_string() == id)
            .unwrap_or_else(|| panic!("check {id} missing"))
    };
    assert_eq!(check("http.https_redirect").status, Status::Pass);
    let headers = check("http.security_headers");
    assert_eq!(headers.status, Status::Warn);
    assert!(
        headers
            .findings
            .iter()
            .any(|m| m.contains("Content-Security-Policy")),
        "findings {:?} do not come from the golden's csp check",
        headers.findings
    );
    assert_eq!(presentation["status_code"], 200);
    assert_eq!(presentation["server_org"], "Example Hosting");
}
