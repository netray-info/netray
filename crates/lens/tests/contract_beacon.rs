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

fn golden(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../../tests/fixtures/contracts/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("golden {path} unreadable: {e}"))
}

/// Serve the golden at POST /inspect; with `hold_open` the body never ends.
async fn serve(name: &str, hold_open: bool) -> String {
    let bytes = golden(name);
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
    // The reason lives in beacon's sub_checks[].detail, not in the category status line.
    assert!(
        auth.messages
            .iter()
            .any(|m| m.contains("RSA 512 bits < 1024")),
        "auth messages must carry the DKIM sub-check detail, got {:?}",
        auth.messages
    );
    assert!(
        transport
            .messages
            .iter()
            .any(|m| m.contains("mode: testing")),
        "transport messages must carry the MTA-STS sub-check detail, got {:?}",
        transport.messages
    );
}

fn bucket<'a>(res: &'a BackendResult, name: &str) -> &'a lens::scoring::engine::CheckResult {
    res.checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("{name} bucket"))
}

fn bucket_na(res: &BackendResult) -> &std::collections::HashMap<String, String> {
    match &res.extra {
        BackendExtra::Email { bucket_na, .. } => bucket_na,
        _ => panic!("expected BackendExtra::Email"),
    }
}

#[tokio::test]
async fn lens_parses_beacon_golden_verdicts() {
    let base = serve("beacon.sse", false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("backend result");
    assert_reflects_golden(&res);
}

#[tokio::test]
async fn lens_returns_at_summary_without_waiting_for_stream_end() {
    let base = serve("beacon.sse", true).await;
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

#[tokio::test]
async fn lens_marks_buckets_na_only_for_beacon_no_mx() {
    let base = serve("beacon-no-mx.sse", false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("backend result");
    for name in [
        "email_infrastructure",
        "email_transport",
        "email_brand_policy",
    ] {
        let b = bucket(&res, name);
        assert!(
            matches!(b.verdict, CheckVerdict::Skip),
            "{name} must be N/A (Skip) for no_mx, got {:?}",
            b.verdict
        );
        assert!(
            b.messages.iter().any(|m| m.contains("No MX records")),
            "{name} messages: {:?}",
            b.messages
        );
        assert!(
            bucket_na(&res).contains_key(name),
            "{name} missing from bucket_na"
        );
    }
}

#[tokio::test]
async fn lens_does_not_treat_mx_cname_failure_as_no_mx() {
    let base = serve("beacon-mx-cname.sse", false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("backend result");
    assert!(
        bucket_na(&res).is_empty(),
        "mx_cname is not 'no MX records'; got bucket_na {:?}",
        bucket_na(&res)
    );
    for name in ["email_transport", "email_brand_policy"] {
        let b = bucket(&res, name);
        assert!(
            !matches!(b.verdict, CheckVerdict::Skip),
            "{name} must not be N/A when MX records exist"
        );
    }
    let infra = bucket(&res, "email_infrastructure");
    assert!(
        matches!(infra.verdict, CheckVerdict::Fail),
        "infra must reflect the mx_cname failure, got {:?}",
        infra.verdict
    );
    assert!(
        infra
            .messages
            .iter()
            .any(|m| m.contains("points to a CNAME")),
        "infra messages must carry the mx_cname detail, got {:?}",
        infra.messages
    );
    assert!(
        infra
            .messages
            .iter()
            .any(|m| m.contains("listed in zen.spamhaus.org")),
        "infra messages must carry the DNSBL listing, got {:?}",
        infra.messages
    );
}

#[tokio::test]
async fn lens_keeps_utf8_char_split_across_chunks() {
    let bytes = golden("beacon-mx-cname.sse");
    let at = bytes
        .windows(2)
        .position(|w| w == [0xC2, 0xA7])
        .expect("golden contains the section sign")
        + 1;
    let (first, second) = (bytes[..at].to_vec(), bytes[at..].to_vec());
    let app = Router::new().route(
        "/inspect",
        post(move || {
            let (first, second) = (first.clone(), second.clone());
            async move {
                let stream = futures::stream::unfold(0u8, move |state| {
                    let (first, second) = (first.clone(), second.clone());
                    async move {
                        match state {
                            0 => Some((
                                Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(first)),
                                1,
                            )),
                            1 => {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                                Some((Ok(axum::body::Bytes::from(second)), 2))
                            }
                            _ => None,
                        }
                    }
                });
                (
                    [("content-type", "text/event-stream")],
                    axum::body::Body::from_stream(stream),
                )
                    .into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("backend result");
    let infra = bucket(&res, "email_infrastructure");
    assert!(
        infra
            .messages
            .iter()
            .any(|m| m.contains("points to a CNAME (RFC 5321 \u{a7}5.1)")),
        "infra messages must carry the exact mx_cname detail, got {:?}",
        infra.messages
    );
    assert!(
        !infra.messages.iter().any(|m| m.contains('\u{FFFD}')),
        "no U+FFFD allowed in messages, got {:?}",
        infra.messages
    );
}
