// Contract: a verdict value lens does not know makes its section Errored; the unchanged goldens
// stay Ok and count nothing in `lens_unknown_verdict_total{section}`.
// (specs/features/grade-integrity/spec.md, Phase 1, requirement 2: C2, C7, C8, C9.)
//
// Each row runs a committed golden from `tests/fixtures/contracts/`, with at most one verdict
// renamed, through a golden module and runs the section. The counter is read from a per-test
// local recorder, so tests do not interfere.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

mod common;

use lens::modules::ModuleSection;
use lens::modules::{Backend, BackendContext};
use lens::scoring::engine::CheckVerdict;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use netray_email::quality::SseEvent;
use netray_engine::{Registry, SectionOutcome};
use netray_http::inspect::assembler::InspectResponse;
use netray_http::translate;
use netray_model::{Protocol, Status};

const TIMEOUT: Duration = Duration::from_secs(5);
const COUNTER: &str = "lens_unknown_verdict_total";

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

/// Run one section against `body`; the checks as (name, verdict), or the error text.
async fn run(section: &str, body: String) -> Result<Vec<(String, CheckVerdict)>, String> {
    let fwd = reqwest::header::HeaderMap::new();
    let pairs = |checks: &[lens::scoring::engine::CheckResult]| {
        checks
            .iter()
            .map(|c| (c.name.clone(), c.verdict.clone()))
            .collect::<Vec<_>>()
    };
    match section {
        "dns" => common::run_dns(netray_dns::testing::golden_module(&body), TIMEOUT)
            .await
            .map(|r| pairs(&r.checks))
            .map_err(|e| format!("{e:?}")),
        "tls" => common::run_tls(netray_tls::testing::golden_module(&body), TIMEOUT)
            .await
            .map(|r| pairs(&r.checks))
            .map_err(|e| format!("{e:?}")),
        "email" => {
            let module = netray_email::testing::golden_module(&body);
            let section = ModuleSection {
                registry: Arc::new(
                    Registry::new()
                        .with(module)
                        .with_facts(common::facts_with_ips(&[])),
                ),
                protocol: Protocol::Email,
                timeout: TIMEOUT,
                public_url: String::new(),
            };
            let ctx = BackendContext {
                dkim_selectors: None,
                forward_headers: fwd,
            };
            section
                .run("example.com", &ctx)
                .await
                .map(|r| pairs(&r.checks))
                .map_err(|e| format!("{e:?}"))
        }
        "ip" => {
            let section = ModuleSection {
                registry: Arc::new(
                    Registry::new()
                        .with(netray_ip::testing::golden_module(&body))
                        .with_facts(common::facts_with_ips(&["1.1.1.1"])),
                ),
                protocol: Protocol::Ip,
                timeout: TIMEOUT,
                public_url: String::new(),
            };
            // A public address: the module's own target policy refuses documentation ranges.
            let ctx = BackendContext {
                dkim_selectors: None,
                forward_headers: fwd,
            };
            section
                .run("example.com", &ctx)
                .await
                .map(|r| pairs(&r.checks))
                .map_err(|e| format!("{e:?}"))
        }
        other => panic!("unknown section {other}"),
    }
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

/// The `data:` lines of a beacon golden decoded as the typed events `netray_email` translates.
fn email_events(body: &str) -> Result<Vec<SseEvent>, String> {
    body.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|d| serde_json::from_str(d.trim()).map_err(|e| e.to_string()))
        .collect()
}

// C7 for email: in-process the verdicts are typed. A beacon verdict value lens does not know
// (category `pass` -> `passed`, summary `spf: pass` -> `passed`) is refused when `netray_email`
// decodes the stream, not translated into a verdict. The `lens_unknown_verdict_total{section}`
// counter is not asserted for email: the decode lives outside lens's metrics, as for HTTP.
#[test]
fn email_unknown_verdict_is_refused_at_decode() {
    let rows = [
        renamed(
            "beacon.sse",
            r#""title":"SPF","type":"category","verdict":"pass""#,
            r#""title":"SPF","type":"category","verdict":"passed""#,
        ),
        renamed("beacon.sse", r#""spf":"pass""#, r#""spf":"passed""#),
    ];
    for body in rows {
        let outcome = email_events(&body);
        assert!(
            outcome.is_err(),
            "an unknown beacon verdict must be refused, got {outcome:?}"
        );
    }
    assert!(
        email_events(&golden("beacon.sse")).is_ok(),
        "the unchanged golden decodes"
    );
}

// C7 for TLS: in-process the verdicts are typed. A tlsight check status lens does not know
// (`fail` -> `passed` on a port quality check) is refused when `netray_tls` decodes the golden,
// or makes the section Errored; it is never translated into a verdict. The
// `lens_unknown_verdict_total{section}` counter is not asserted for TLS: the decode lives
// outside lens's metrics, as for HTTP and email.
#[tokio::test]
async fn tls_unknown_status_is_refused_at_decode() {
    let body = json_with(
        "tlsight-inspect.json",
        "/ports/0/quality/checks/0/status",
        "passed",
    );
    let built = std::panic::catch_unwind(|| netray_tls::testing::golden_module(&body));
    if let Ok(module) = built {
        let outcome = common::run_tls(module, TIMEOUT).await;
        assert!(
            outcome.is_err(),
            "an unknown tlsight status must be refused, got {:?}",
            outcome.map(|r| r.checks.len())
        );
    }
}

// C7 for DNS: in-process the verdicts are typed. A prism lint verdict lens does not know
// (`Ok` -> `Passed`) is refused when `netray_dns` decodes the stream, or makes the section
// Errored; it is never translated into a verdict. The `lens_unknown_verdict_total{section}`
// counter is not asserted for DNS: the decode lives outside lens's metrics, as for TLS, HTTP and
// email.
#[tokio::test]
async fn dns_unknown_verdict_is_refused_at_decode() {
    let body = renamed(
        "prism.sse",
        r#"{"Ok":"Found exactly one SPF record"}"#,
        r#"{"Passed":"Found exactly one SPF record"}"#,
    );
    let built = std::panic::catch_unwind(|| netray_dns::testing::golden_module(&body));
    if let Ok(module) = built {
        let outcome = common::run_dns(module, TIMEOUT).await;
        assert!(
            outcome.is_err(),
            "an unknown prism verdict must be refused, got {:?}",
            outcome.map(|r| r.checks.len())
        );
    }
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
