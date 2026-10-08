// Contract test: lens parses the goldens its DNS, TLS and HTTP backends write.
//
// Each backend's own `tests/contract_golden.rs` writes its golden under
// `tests/fixtures/contracts/` from its real response types. Here a local HTTP server
// serves the committed golden at the path and method lens calls, and lens's public
// `check_dns` / `check_tls` / `check_http` must produce checks and no error, with values
// that come from the golden rather than defaults.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::http::header;
use axum::routing::{get, post};
use lens::backends::BackendExtra;
use lens::backends::dns::check_dns;
use lens::backends::http::check_http;
use lens::backends::tls::check_tls;
use lens::scoring::engine::CheckVerdict;

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
    let body = golden("tlsight-inspect.json");
    let app = Router::new().route(
        "/api/inspect",
        get(move || {
            let body = body.clone();
            async move { ([(header::CONTENT_TYPE, "application/json")], body) }
        }),
    );
    let url = serve(app).await;

    let result = check_tls(
        &reqwest::Client::new(),
        &url,
        "example.com",
        TIMEOUT,
        &reqwest::header::HeaderMap::new(),
    )
    .await
    .expect("lens must parse tlsight's golden without a section error");

    assert!(
        !result.checks.is_empty(),
        "no checks parsed from tlsight's golden"
    );
    let ocsp = result
        .checks
        .iter()
        .find(|c| c.name == "ocsp_stapled")
        .expect("port quality check `ocsp_stapled` missing");
    assert_eq!(ocsp.verdict, CheckVerdict::Fail);
    assert!(
        result.checks.iter().any(|c| c.name == "hsts"),
        "hostname quality check `hsts` missing"
    );
    assert!(
        result.raw_headline.contains("TLSv1.3") && result.raw_headline.contains("60d"),
        "headline `{}` does not carry the golden's version and days remaining",
        result.raw_headline
    );
}

#[tokio::test]
async fn lens_parses_spectra_golden() {
    let body = golden("spectra-inspect.json");
    let app = Router::new().route(
        "/api/inspect",
        get(move || {
            let body = body.clone();
            async move { ([(header::CONTENT_TYPE, "application/json")], body) }
        }),
    );
    let url = serve(app).await;

    let result = check_http(
        &reqwest::Client::new(),
        &url,
        "example.com",
        TIMEOUT,
        &reqwest::header::HeaderMap::new(),
    )
    .await
    .expect("lens must parse spectra's golden without a section error");

    assert!(
        !result.checks.is_empty(),
        "no checks parsed from spectra's golden"
    );
    let redirect = result
        .checks
        .iter()
        .find(|c| c.name == "https_redirect")
        .unwrap();
    assert_eq!(redirect.verdict, CheckVerdict::Pass);
    let headers = result
        .checks
        .iter()
        .find(|c| c.name == "security_headers")
        .unwrap();
    assert_eq!(headers.verdict, CheckVerdict::Warn);
    assert!(
        headers
            .messages
            .iter()
            .any(|m| m.contains("Content-Security-Policy")),
        "messages {:?} do not come from the golden's csp check",
        headers.messages
    );
    match result.extra {
        BackendExtra::Http {
            status_code,
            server_org,
            ..
        } => {
            assert_eq!(status_code, Some(200));
            assert_eq!(server_org.as_deref(), Some("Example Hosting"));
        }
        _ => panic!("expected BackendExtra::Http"),
    }
}
