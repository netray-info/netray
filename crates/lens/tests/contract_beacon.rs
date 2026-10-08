//! Contract: lens's email backend must parse the SSE stream beacon actually sends
//! (golden: `tests/fixtures/contracts/beacon.sse`, produced by
//! `crates/beacon/tests/contract_golden.rs`).

use std::time::Duration;

use axum::Router;
use axum::response::IntoResponse;
use axum::routing::post;
use lens::backends::email::EmailBackend;
use lens::backends::{Backend, BackendContext, BackendExtra, BackendResult};
use lens::scoring::engine::CheckVerdict;

fn golden() -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/contracts/beacon.sse"
    );
    std::fs::read(path).unwrap_or_else(|e| panic!("golden {path} unreadable: {e}"))
}

/// Serve the golden at POST /inspect; with `hold_open` the body never ends.
async fn serve(hold_open: bool) -> String {
    let bytes = golden();
    let app = Router::new().route(
        "/inspect",
        post(move || {
            let bytes = bytes.clone();
            async move {
                let head = futures::stream::once(async move {
                    Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(bytes))
                });
                let body = if hold_open {
                    let tail = futures::stream::once(async {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        Ok::<_, std::convert::Infallible>(axum::body::Bytes::new())
                    });
                    axum::body::Body::from_stream(futures::StreamExt::chain(head, tail))
                } else {
                    axum::body::Body::from_stream(head)
                };
                ([("content-type", "text/event-stream")], body).into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    base
}

async fn run_backend(base: String, timeout: Duration) -> Result<BackendResult, String> {
    let backend = EmailBackend {
        email_url: base,
        public_url: String::new(),
        timeout,
        client: reqwest::Client::new(),
    };
    let ctx = BackendContext {
        resolved_ips: vec![],
        dkim_selectors: None,
        forward_headers: reqwest::header::HeaderMap::new(),
    };
    backend
        .run("example.com", &ctx)
        .await
        .map_err(|e| format!("{e:?}"))
}

fn assert_reflects_golden(res: &BackendResult) {
    let auth = res
        .checks
        .iter()
        .find(|c| c.name == "email_authentication")
        .expect("email_authentication bucket");
    // golden: dkim=fail, dmarc=warn, spf=pass
    assert!(
        matches!(auth.verdict, CheckVerdict::Fail),
        "auth bucket must be Fail (golden dkim=fail), got {:?}",
        auth.verdict
    );
    // golden: mta_sts=warn, tls_rpt=warn, dane=pass
    let transport = res
        .checks
        .iter()
        .find(|c| c.name == "email_transport")
        .expect("email_transport bucket");
    assert!(
        matches!(transport.verdict, CheckVerdict::Warn),
        "transport bucket must be Warn (golden mta_sts/tls_rpt=warn), got {:?}",
        transport.verdict
    );
    match &res.extra {
        BackendExtra::Email { grade, .. } => assert_eq!(grade.as_deref(), Some("D")),
        _ => panic!("expected BackendExtra::Email"),
    }
}

#[tokio::test]
async fn lens_parses_beacon_golden_verdicts() {
    let base = serve(false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("backend result");
    assert_reflects_golden(&res);
}

#[tokio::test]
async fn lens_returns_at_summary_without_waiting_for_stream_end() {
    let base = serve(true).await;
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        run_backend(base, Duration::from_secs(20)),
    )
    .await;
    let res = outcome
        .expect("lens must return at the summary event, not wait for the stream to close")
        .expect("backend result");
    assert_reflects_golden(&res);
}
