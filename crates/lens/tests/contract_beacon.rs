//! Contract: lens's email backend must parse the SSE stream beacon actually sends
//! (golden: `tests/fixtures/contracts/beacon.sse`, produced by
//! `crates/beacon/tests/contract_golden.rs`).

use std::time::Duration;

use axum::Router;
use axum::response::IntoResponse;
use axum::routing::post;
use lens::backends::email::EmailBackend;
use lens::backends::{Backend, BackendContext, BackendExtra, BackendResult};
use lens::check::SectionError;
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
    serve_bytes(golden(name), hold_open).await
}

async fn serve_bytes(bytes: Vec<u8>, hold_open: bool) -> String {
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
    run_raw(base, timeout).await.map_err(|e| format!("{e:?}"))
}

async fn run_raw(base: String, timeout: Duration) -> Result<BackendResult, SectionError> {
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
    backend.run("example.com", &ctx).await
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
    // An mx_cname failure is not "no MX": no bucket is N/A for that reason. Buckets whose
    // categories are all Info (here transport and brand) are "not applicable" on their own
    // (requirement 9), like brand on beacon.sse.
    assert!(
        bucket_na(&res).values().all(|r| r != "no MX records"),
        "mx_cname is not 'no MX records'; got bucket_na {:?}",
        bucket_na(&res)
    );
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

// ---------------------------------------------------------------------------
// specs/features/email-scoring, Phase 2 (requirements 7-10)
// ---------------------------------------------------------------------------

/// Rewrite the golden's `data:` events and re-emit the stream with its framing untouched:
/// the stream is split on the blank-line separators, only the JSON of each block changes,
/// and the final blank line stays. `f` may edit or remove events.
fn variant(name: &str, f: impl FnOnce(&mut Vec<serde_json::Value>)) -> Vec<u8> {
    let text = String::from_utf8(golden(name)).unwrap();
    assert!(
        text.ends_with("\n\n"),
        "{name}: golden must end with a blank line"
    );
    let mut blocks: Vec<&str> = text.split("\n\n").collect();
    assert_eq!(blocks.pop(), Some(""), "{name}: trailing separator");
    let mut events: Vec<serde_json::Value> = blocks
        .iter()
        .map(|b| {
            let json = b
                .strip_prefix("data: ")
                .unwrap_or_else(|| panic!("{name}: block without data: prefix: {b}"));
            serde_json::from_str(json).unwrap()
        })
        .collect();
    f(&mut events);
    let mut out = String::new();
    for e in &events {
        out.push_str("data: ");
        out.push_str(&serde_json::to_string(e).unwrap());
        out.push_str("\n\n");
    }
    out.into_bytes()
}

fn category_mut<'a>(events: &'a mut [serde_json::Value], name: &str) -> &'a mut serde_json::Value {
    events
        .iter_mut()
        .find(|e| e["category"] == name)
        .unwrap_or_else(|| panic!("no category event {name}"))
}

fn summary_mut(events: &mut [serde_json::Value]) -> &mut serde_json::Value {
    events
        .iter_mut()
        .find(|e| e["type"] == "summary")
        .expect("summary event")
}

fn assert_na(res: &BackendResult, name: &str) {
    let b = bucket(res, name);
    assert!(
        matches!(b.verdict, CheckVerdict::Skip),
        "{name} must be N/A (Skip), got {:?}",
        b.verdict
    );
    assert!(
        bucket_na(res).contains_key(name),
        "{name} missing from bucket_na"
    );
}

// C2 / C6 parser part: beacon's own timeout summary is a Timeout, not N/A.
#[tokio::test]
async fn beacon_timeout_golden_is_section_timeout() {
    let base = serve("beacon-timeout.sse", false).await;
    let err = run_raw(base, Duration::from_secs(5))
        .await
        .expect_err("a skipped beacon run has no result");
    assert!(matches!(err, SectionError::Timeout), "got {err:?}");
}

// C7: a Null MX domain does not accept mail; authentication still scores and passes.
#[tokio::test]
async fn beacon_null_mx_golden_marks_three_buckets_na_and_auth_passes() {
    let base = serve("beacon-null-mx.sse", false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("result");
    for name in [
        "email_infrastructure",
        "email_transport",
        "email_brand_policy",
    ] {
        assert_na(&res, name);
        // The domain publishes `MX 0 .`: the message must say Null MX, not "No MX records".
        let msgs = &bucket(&res, name).messages;
        assert!(
            msgs.iter().any(|m| m.contains("Null MX"))
                && msgs.iter().all(|m| !m.contains("No MX records")),
            "{name}: Null MX message expected, got {msgs:?}"
        );
    }
    let auth = bucket(&res, "email_authentication");
    assert!(
        matches!(auth.verdict, CheckVerdict::Pass),
        "auth must Pass for a Null MX domain, got {:?} {:?}",
        auth.verdict,
        auth.messages
    );
}

// Requirement 9: Null MX makes the three buckets N/A for that reason, even when a
// receiving-side check fails (a stale MTA-STS record whose policy fetch fails).
#[tokio::test]
async fn null_mx_keeps_transport_na_when_a_receiving_check_fails() {
    let bytes = variant("beacon-null-mx.sse", |ev| {
        let sts = category_mut(ev, "mta_sts");
        sts["sub_checks"] = serde_json::json!([
            {"detail": "policy host not reachable", "name": "https_fetch_failed", "verdict": "fail"}
        ]);
        sts["verdict"] = "fail".into();
        summary_mut(ev)["verdicts"]["mta_sts"] = "fail".into();
    });
    let base = serve_bytes(bytes, false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("result");
    assert_na(&res, "email_transport");
    assert_ne!(
        bucket_na(&res).get("email_transport").map(String::as_str),
        Some("not applicable"),
        "transport is N/A because of Null MX, not because its categories are Info"
    );
}

// Requirement 10: with sends_no_mail, spf_mx_coverage does not reach authentication.
#[tokio::test]
async fn sends_no_mail_excludes_spf_mx_coverage() {
    let bytes = variant("beacon-null-mx.sse", |ev| {
        let cv = category_mut(ev, "cross_validation");
        let subs = cv["sub_checks"].as_array_mut().expect("sub_checks");
        subs.push(serde_json::json!(
            {"detail": "MX host IPs are not covered by SPF (only relevant if these MX hosts also send outbound mail)", "name": "spf_mx_coverage", "verdict": "warn"}
        ));
    });
    let base = serve_bytes(bytes, false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("result");
    let auth = bucket(&res, "email_authentication");
    assert!(
        matches!(auth.verdict, CheckVerdict::Pass),
        "spf_mx_coverage must not count for a domain that sends no mail, got {:?} {:?}",
        auth.verdict,
        auth.messages
    );
}

// C8: no MX records.
#[tokio::test]
async fn beacon_no_mx_golden_marks_three_buckets_na() {
    let base = serve("beacon-no-mx.sse", false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("result");
    for name in [
        "email_infrastructure",
        "email_transport",
        "email_brand_policy",
    ] {
        assert_na(&res, name);
    }
}

// C9 parser part: a BIMI-less domain has nothing to score for brand policy.
#[tokio::test]
async fn beacon_golden_brand_policy_is_not_applicable() {
    let base = serve("beacon.sse", false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("result");
    let brand = bucket(&res, "email_brand_policy");
    assert!(
        matches!(brand.verdict, CheckVerdict::Skip),
        "brand must be Skip without BIMI, got {:?}",
        brand.verdict
    );
    let reason = bucket_na(&res)
        .get("email_brand_policy")
        .expect("brand missing from bucket_na")
        .to_lowercase();
    assert!(reason.contains("not applicable"), "reason: {reason}");
}

// C11: a category that did not complete makes the section Errored.
#[tokio::test]
async fn beacon_partial_golden_errors_the_section() {
    let base = serve("beacon-partial.sse", false).await;
    let out = run_raw(base, Duration::from_secs(5)).await;
    assert!(out.is_err(), "a skipped category must Error the section");
    assert!(
        !matches!(
            out,
            Err(SectionError::Timeout) | Err(SectionError::NotApplicable { .. })
        ),
        "got {out:?}"
    );
}

// C13: a sender without DKIM but with p=reject gets the cross-validation warning in auth.
#[tokio::test]
async fn beacon_sending_no_dkim_golden_warns_authentication() {
    let base = serve("beacon-sending-no-dkim.sse", false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("result");
    let auth = bucket(&res, "email_authentication");
    assert!(
        matches!(auth.verdict, CheckVerdict::Warn),
        "auth must Warn, got {:?}",
        auth.verdict
    );
    assert!(
        auth.messages
            .iter()
            .any(|m| m.contains("DMARC p=reject and SPF -all but no DKIM keys found")),
        "auth messages must carry the reject_no_dkim detail, got {:?}",
        auth.messages
    );
}

// C10: a BIMI/DMARC cross-validation warning is routed to the brand bucket.
#[tokio::test]
async fn bimi_dmarc_policy_cross_check_warns_brand_bucket() {
    let bytes = variant("beacon.sse", |ev| {
        let cv = category_mut(ev, "cross_validation");
        cv["sub_checks"] = serde_json::json!([
            {"detail": "BIMI requires DMARC p=quarantine or reject", "name": "bimi_dmarc_policy", "verdict": "warn"}
        ]);
        cv["verdict"] = "warn".into();
        summary_mut(ev)["verdicts"]["cross_validation"] = "warn".into();
    });
    let base = serve_bytes(bytes, false).await;
    let res = run_backend(base, Duration::from_secs(5))
        .await
        .expect("result");
    let brand = bucket(&res, "email_brand_policy");
    assert!(
        matches!(brand.verdict, CheckVerdict::Warn),
        "brand must Warn, got {:?} {:?}",
        brand.verdict,
        brand.messages
    );
}

// C12: a summary verdict without its category event is a broken stream.
#[tokio::test]
async fn missing_category_event_errors_the_section() {
    let bytes = variant("beacon.sse", |ev| {
        ev.retain(|e| e["category"] != "dnsbl");
    });
    let base = serve_bytes(bytes, false).await;
    let out = run_raw(base, Duration::from_secs(5)).await;
    assert!(
        out.is_err(),
        "a missing category event must Error, got {out:?}"
    );
}

// C15: an unknown cross-validation sub-check is a contract break: Errored and counted.
#[tokio::test(flavor = "current_thread")]
async fn unknown_cross_validation_sub_check_errors_and_counts() {
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);

    let bytes = variant("beacon.sse", |ev| {
        let cv = category_mut(ev, "cross_validation");
        cv["sub_checks"] =
            serde_json::json!([{"detail": "new", "name": "brand_new_rule", "verdict": "warn"}]);
        cv["verdict"] = "warn".into();
        summary_mut(ev)["verdicts"]["cross_validation"] = "warn".into();
    });
    let base = serve_bytes(bytes, false).await;
    let out = run_raw(base, Duration::from_secs(5)).await;
    assert!(
        out.is_err(),
        "an unrouted sub-check must Error, got {out:?}"
    );

    let counted: u64 = snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter(|(key, ..)| {
            key.key().name() == "lens_unknown_verdict_total"
                && key
                    .key()
                    .labels()
                    .any(|l| l.key() == "section" && l.value() == "email")
        })
        .map(|(_, _, _, v)| match v {
            DebugValue::Counter(n) => n,
            _ => 0,
        })
        .sum();
    assert_eq!(
        counted, 1,
        "lens_unknown_verdict_total{{section=\"email\"}}"
    );
}

// C14: lens's routing tables cover everything beacon can emit, and its sentinels are beacon's.
#[test]
fn lens_routing_covers_beacon_vocabulary() {
    use lens::backends::email as e;
    let mut problems = Vec::new();
    for c in beacon::quality::types::Category::ALL {
        let name = serde_json::to_value(&c)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let known = e::BUCKETED.iter().any(|(cat, _)| *cat == name)
            || e::EXCLUDED.iter().any(|(cat, _)| *cat == name);
        if !known {
            problems.push(format!(
                "category `{name}` is neither BUCKETED nor EXCLUDED"
            ));
        }
    }
    for name in beacon::checks::cross_validation::CROSS_VALIDATION_CHECKS {
        if e::route_cross_validation(name).is_none() {
            problems.push(format!("cross-validation check `{name}` has no bucket"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert_eq!(e::BEACON_SKIPPED, beacon::checks::SKIPPED);
    assert_eq!(e::BEACON_NULL_MX, beacon::checks::mx::NULL_MX);
    assert_eq!(
        e::BEACON_SENDS_NO_MAIL,
        beacon::checks::cross_validation::SENDS_NO_MAIL
    );
}
