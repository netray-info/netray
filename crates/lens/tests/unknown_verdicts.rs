// Contract: a verdict value lens does not know makes its section Errored and increments
// `lens_unknown_verdict_total{section}`; the unchanged goldens stay Ok and count nothing.
// (specs/features/grade-integrity/spec.md, Phase 1, requirement 2: C2, C7, C8, C9.)
//
// Each row serves a committed golden from `tests/fixtures/contracts/`, with at most one
// verdict renamed, on a local stub and calls the section's public backend function. The
// counter is read from a per-test local recorder, so tests do not interfere.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::http::header;
use axum::routing::{get, post};
use lens::backends::dns::check_dns;
use lens::backends::email::EmailBackend;
use lens::backends::ip::check_ip;
use lens::backends::tls::check_tls;
use lens::backends::{Backend, BackendContext};
use lens::scoring::engine::CheckVerdict;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::translate;
use netray_model::Status;

const TIMEOUT: Duration = Duration::from_secs(5);
const COUNTER: &str = "lens_unknown_verdict_total";

fn public_or_documentation(ip: std::net::IpAddr) -> bool {
    match netray_common::target_policy::refusal_reason(ip) {
        None => true,
        Some(r) => r.starts_with("documentation"),
    }
}

fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()))
}

/// The golden with exactly one occurrence of `from` replaced by `to`.
fn renamed(name: &str, from: &str, to: &str) -> String {
    let body = golden(name);
    assert_eq!(
        body.matches(from).count(),
        1,
        "{name}: `{from}` must occur exactly once"
    );
    body.replace(from, to)
}

/// The JSON golden with the string at `pointer` set to `to`.
fn json_with(name: &str, pointer: &str, to: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(&golden(name)).unwrap();
    *v.pointer_mut(pointer)
        .unwrap_or_else(|| panic!("{name}: no value at {pointer}")) = to.into();
    v.to_string()
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Serve `body` at `path` with the given method and content type.
async fn stub(path: &str, is_post: bool, content_type: &'static str, body: String) -> String {
    let handler = move || {
        let body = body.clone();
        async move { ([(header::CONTENT_TYPE, content_type)], body) }
    };
    let route = if is_post { post(handler) } else { get(handler) };
    serve(Router::new().route(path, route)).await
}

/// Run one section against `body`; the checks as (name, verdict), or the error text.
async fn run(section: &str, body: String) -> Result<Vec<(String, CheckVerdict)>, String> {
    let client = reqwest::Client::new();
    let fwd = reqwest::header::HeaderMap::new();
    let pairs = |checks: &[lens::scoring::engine::CheckResult]| {
        checks
            .iter()
            .map(|c| (c.name.clone(), c.verdict.clone()))
            .collect::<Vec<_>>()
    };
    match section {
        "dns" => {
            let url = stub("/api/check", true, "text/event-stream", body).await;
            check_dns(&client, &url, "example.com", &[], TIMEOUT, &fwd)
                .await
                .map(|r| pairs(&r.checks))
                .map_err(|e| format!("{e:?}"))
        }
        "tls" => {
            let url = stub("/api/inspect", false, "application/json", body).await;
            check_tls(&client, &url, "example.com", TIMEOUT, &fwd)
                .await
                .map(|r| pairs(&r.checks))
                .map_err(|e| format!("{e:?}"))
        }
        "email" => {
            let url = stub("/inspect", true, "text/event-stream", body).await;
            let backend = EmailBackend {
                email_url: url,
                public_url: String::new(),
                timeout: TIMEOUT,
                client: client.clone(),
            };
            let ctx = BackendContext {
                resolved_ips: vec![],
                dkim_selectors: None,
                forward_headers: fwd,
            };
            backend
                .run("example.com", &ctx)
                .await
                .map(|r| pairs(&r.checks))
                .map_err(|e| format!("{e:?}"))
        }
        "ip" => {
            let url = stub("/json", false, "application/json", body).await;
            let ip: std::net::IpAddr = "203.0.113.42".parse().unwrap();
            check_ip(&client, &url, &[ip], TIMEOUT, &fwd, public_or_documentation)
                .await
                .map(|r| pairs(&r.checks))
                .map_err(|e| format!("{e:?}"))
        }
        other => panic!("unknown section {other}"),
    }
}

/// Sum of `lens_unknown_verdict_total` with label `section = <section>`.
fn unknown_count(snapshotter: &Snapshotter, section: &str) -> u64 {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter(|(key, ..)| {
            key.key().name() == COUNTER
                && key
                    .key()
                    .labels()
                    .any(|l| l.key() == "section" && l.value() == section)
        })
        .map(|(_, _, _, value)| match value {
            DebugValue::Counter(n) => n,
            _ => 0,
        })
        .sum()
}

fn unknown_total(snapshotter: &Snapshotter) -> u64 {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter(|(key, ..)| key.key().name() == COUNTER)
        .map(|(_, _, _, value)| match value {
            DebugValue::Counter(n) => n,
            _ => 0,
        })
        .sum()
}

// Local recorders are thread-local: a current-thread runtime keeps lens on this thread.
#[tokio::test(flavor = "current_thread")]
async fn unknown_verdict_errors_section_and_counts() {
    // (section, body with one verdict renamed to an unknown value)
    let rows: Vec<(&str, String)> = vec![
        // C7: prism lint `Ok` -> `Passed`
        (
            "dns",
            renamed(
                "prism.sse",
                r#"{"Ok":"Found exactly one SPF record"}"#,
                r#"{"Passed":"Found exactly one SPF record"}"#,
            ),
        ),
        // C7: tlsight port check status (chain_trusted, a hard-fail check) -> `passed`
        (
            "tls",
            json_with(
                "tlsight-inspect.json",
                "/ports/0/quality/checks/0/status",
                "passed",
            ),
        ),
        // C7: beacon category verdict `pass` -> `passed`
        (
            "email",
            renamed(
                "beacon.sse",
                r#""title":"SPF","type":"category","verdict":"pass""#,
                r#""title":"SPF","type":"category","verdict":"passed""#,
            ),
        ),
        // C7: beacon summary verdict (the scored map) `spf: pass` -> `passed`
        (
            "email",
            renamed("beacon.sse", r#""spf":"pass""#, r#""spf":"passed""#),
        ),
    ];

    let mut failures: Vec<String> = Vec::new();
    for (section, body) in rows {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);

        let outcome = run(section, body).await;
        if outcome.is_ok() {
            failures.push(format!(
                "{section}: an unknown verdict must make the section Errored, got {outcome:?}"
            ));
        }
        let counted = unknown_count(&snapshotter, section);
        if counted != 1 {
            failures.push(format!(
                "{section}: {COUNTER}{{section=\"{section}\"}} must increment by 1, got {counted}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// C9: the unchanged goldens stay Ok, scored as before, and count nothing.
#[tokio::test(flavor = "current_thread")]
async fn known_verdicts_stay_ok_and_count_nothing() {
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);

    let verdict_of = |checks: &[(String, CheckVerdict)], name: &str| {
        checks
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("check `{name}` missing in {checks:?}"))
            .1
            .clone()
    };

    let dns = run("dns", golden("prism.sse")).await.expect("dns Ok");
    assert_eq!(verdict_of(&dns, "ns"), CheckVerdict::Pass);
    assert_eq!(verdict_of(&dns, "caa"), CheckVerdict::Warn);

    let tls = run("tls", golden("tlsight-inspect.json"))
        .await
        .expect("tls Ok");
    assert_eq!(verdict_of(&tls, "ocsp_stapled"), CheckVerdict::Fail);

    let http = http_statuses(&golden("spectra-inspect.json")).expect("http Ok");
    assert_eq!(status_of(&http, "http.https_redirect"), Status::Pass);
    assert_eq!(status_of(&http, "http.security_headers"), Status::Warn);

    // golden: dkim=fail, mta_sts=warn; brand (bimi `absent`, info only) is Skip
    let email = run("email", golden("beacon.sse")).await.expect("email Ok");
    assert_eq!(
        verdict_of(&email, "email_authentication"),
        CheckVerdict::Fail
    );
    assert_eq!(verdict_of(&email, "email_transport"), CheckVerdict::Warn);
    assert_eq!(verdict_of(&email, "email_brand_policy"), CheckVerdict::Skip);

    let ip = run("ip", golden("ifconfig-json.json"))
        .await
        .expect("ip Ok");
    assert_eq!(verdict_of(&ip, "reputation"), CheckVerdict::Pass);

    assert_eq!(
        unknown_total(&snapshotter),
        0,
        "known verdicts must not increment {COUNTER}"
    );
}

/// The HTTP section's checks as (id, status), decoded and translated by `netray_http`.
fn http_statuses(body: &str) -> Result<Vec<(String, Status)>, String> {
    let resp: InspectResponse = serde_json::from_str(body).map_err(|e| e.to_string())?;
    match translate(&resp) {
        SectionOutcome::Measured { checks, .. } => Ok(checks
            .iter()
            .map(|c| (c.id.to_string(), c.status))
            .collect()),
        other => Err(format!("{other:?}")),
    }
}

fn status_of(checks: &[(String, Status)], id: &str) -> Status {
    checks
        .iter()
        .find(|(n, _)| n == id)
        .unwrap_or_else(|| panic!("check `{id}` missing in {checks:?}"))
        .1
}

// C8: a spectra check status lens does not know (`pass` -> `passed`) is refused when
// `netray_http` decodes the response, not translated into a verdict. The section-level
// `lens_unknown_verdict_total` counter is not asserted for HTTP: the decode now lives in
// `netray_http`, outside lens's metrics.
#[test]
fn http_unknown_status_is_refused_at_decode() {
    let body = json_with("spectra-inspect.json", "/quality/checks/0/status", "passed");
    let outcome = http_statuses(&body);
    assert!(
        outcome.is_err(),
        "an unknown spectra status must be refused, got {outcome:?}"
    );
}
