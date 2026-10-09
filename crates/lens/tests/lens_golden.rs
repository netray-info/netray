//! Golden test: the verdict lens computes from the committed backend goldens.
//!
//! Local stub servers serve the backend goldens from `tests/fixtures/contracts/` at the
//! paths and methods lens calls. lens runs with the backend and scoring settings of
//! `tests/fixtures/lens.production.toml` (backend URLs pointed at the stubs), and
//! `POST /api/check` is driven through its router in-process. A projection of the SSE
//! events (grades, statuses, check verdicts; no prose, durations or ids) is compared with
//! `tests/fixtures/contracts/lens-<fixture>.json`. `UPDATE_GOLDEN=1` rewrites those files.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use lens::config::Config;
use lens::routes::api_router;
use lens::state::AppState;
use serde::Serialize;
use serde_json::Value;
use tower::ServiceExt;

/// One fixture: a name and the golden file each backend serves. `None` for tls or http means
/// that backend answers HTTP 500.
struct Fixture {
    name: &'static str,
    dns: &'static str,
    tls: Option<&'static str>,
    http: Option<&'static str>,
    email: &'static str,
    ip: &'static str,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "healthy",
        dns: "prism.sse",
        tls: Some("tlsight-inspect.json"),
        http: Some("spectra-inspect.json"),
        email: "beacon.sse",
        ip: "ifconfig-json.json",
    },
    Fixture {
        name: "no-mx",
        dns: "prism.sse",
        tls: Some("tlsight-inspect.json"),
        http: Some("spectra-inspect.json"),
        email: "beacon-no-mx.sse",
        ip: "ifconfig-json.json",
    },
    Fixture {
        name: "mx-cname",
        dns: "prism.sse",
        tls: Some("tlsight-inspect.json"),
        http: Some("spectra-inspect.json"),
        email: "beacon-mx-cname.sse",
        ip: "ifconfig-json.json",
    },
    Fixture {
        name: "no-address-records",
        dns: "prism-no-address.sse",
        tls: None,
        http: None,
        email: "beacon.sse",
        ip: "ifconfig-json.json",
    },
    Fixture {
        name: "http-only",
        dns: "prism.sse",
        tls: Some("tlsight-unreachable.json"),
        http: Some("spectra-inspect.json"),
        email: "beacon.sse",
        ip: "ifconfig-json.json",
    },
    Fixture {
        name: "no-weighted-tls",
        dns: "prism.sse",
        tls: Some("tlsight-not-tested.json"),
        http: Some("spectra-inspect.json"),
        email: "beacon.sse",
        ip: "ifconfig-json.json",
    },
    Fixture {
        name: "null-mx",
        dns: "prism.sse",
        tls: Some("tlsight-inspect.json"),
        http: Some("spectra-inspect.json"),
        email: "beacon-null-mx.sse",
        ip: "ifconfig-json.json",
    },
];

fn fixture(name: &str) -> &'static Fixture {
    FIXTURES
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("unknown fixture {name}"))
}

fn contracts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/contracts")
}

fn golden(name: &str) -> String {
    let path = contracts_dir().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()))
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}")
}

/// The DNS golden's A record (192.0.2.10) is a documentation address, which lens's production
/// address policy does not enrich (spec backend-correctness, requirement 3). The harness
/// serves it with a public stand-in so the IP section stays scored.
const DNS_DOCUMENTATION_A: &str = r#"{"A":"192.0.2.10"}"#;
const DNS_PUBLIC_A: &str = r#"{"A":"1.1.1.1"}"#;

/// A stub serving `file` at `path`, with the given content type and method.
async fn stub(
    path: &'static str,
    post_method: bool,
    content_type: &'static str,
    file: &str,
) -> String {
    stub_body(path, post_method, content_type, golden(file)).await
}

/// A stub serving `body` at `path`, with the given content type and method.
async fn stub_body(
    path: &'static str,
    post_method: bool,
    content_type: &'static str,
    body: String,
) -> String {
    let handler = move || {
        let body = body.clone();
        async move { ([(header::CONTENT_TYPE, content_type)], body) }
    };
    let route = if post_method {
        post(handler)
    } else {
        get(handler)
    };
    serve(Router::new().route(path, route)).await
}

/// A stub answering HTTP 500 at `path`.
async fn failing_stub(path: &'static str, post_method: bool) -> String {
    let handler = || async { StatusCode::INTERNAL_SERVER_ERROR };
    let route = if post_method {
        post(handler)
    } else {
        get(handler)
    };
    serve(Router::new().route(path, route)).await
}

async fn production_config(f: &Fixture) -> Config {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");

    let dns = golden(f.dns).replace(DNS_DOCUMENTATION_A, DNS_PUBLIC_A);
    config.backends.dns.url = Some(stub_body("/api/check", true, "text/event-stream", dns).await);
    config.backends.tls.url = Some(match f.tls {
        Some(file) => stub("/api/inspect", false, "application/json", file).await,
        None => failing_stub("/api/inspect", false).await,
    });
    config.backends.ip.url = Some(stub("/json", false, "application/json", f.ip).await);
    config
        .backends
        .http
        .as_mut()
        .expect("http backend configured")
        .url = Some(match f.http {
        Some(file) => stub("/api/inspect", false, "application/json", file).await,
        None => failing_stub("/api/inspect", false).await,
    });
    config
        .backends
        .email
        .as_mut()
        .expect("email backend configured")
        .url = Some(stub("/inspect", true, "text/event-stream", f.email).await);

    config.snapshots.enabled = false;
    // Every run computes the verdict afresh instead of answering from the cache.
    config.cache.enabled = false;
    config
}

#[derive(Serialize)]
struct Summary {
    grade: String,
    score: f64,
    overall: String,
    complete: Option<bool>,
    sections: BTreeMap<String, String>,
    section_grades: BTreeMap<String, String>,
    hard_fail: bool,
    hard_fail_checks: Vec<String>,
    not_applicable: Vec<String>,
}

#[derive(Serialize)]
struct Check {
    name: String,
    verdict: String,
}

#[derive(Serialize)]
struct Section {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    grade: Option<String>,
    checks: Vec<Check>,
}

#[derive(Serialize)]
struct Projection {
    summary: Summary,
    sections: BTreeMap<String, Section>,
}

fn str_of(v: &Value, key: &str) -> String {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("`{key}` is not a string in {v}"))
        .to_string()
}

fn string_map(v: &Value) -> BTreeMap<String, String> {
    v.as_object()
        .unwrap_or_else(|| panic!("not an object: {v}"))
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
        .collect()
}

/// Parse an SSE body into `(event, data)` pairs.
fn parse_sse(body: &str) -> Vec<(String, Value)> {
    body.replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|block| {
            let mut event = None;
            let mut data = Vec::new();
            for line in block.lines() {
                if let Some(e) = line.strip_prefix("event:") {
                    event = Some(e.trim().to_string());
                } else if let Some(d) = line.strip_prefix("data:") {
                    data.push(d.strip_prefix(' ').unwrap_or(d).to_string());
                }
            }
            let event = event?;
            let data = serde_json::from_str(&data.join("\n"))
                .unwrap_or_else(|e| panic!("event {event} carries invalid JSON: {e}"));
            Some((event, data))
        })
        .collect()
}

fn project(events: &[(String, Value)]) -> Projection {
    let mut summary = None;
    let mut sections = BTreeMap::new();
    for (name, data) in events {
        match name.as_str() {
            "summary" => {
                summary = Some(Summary {
                    grade: str_of(data, "grade"),
                    score: data["score"].as_f64().expect("summary score"),
                    overall: str_of(data, "overall"),
                    complete: data["complete"].as_bool(),
                    sections: string_map(&data["sections"]),
                    section_grades: string_map(&data["section_grades"]),
                    hard_fail: data["hard_fail"].as_bool().expect("hard_fail"),
                    hard_fail_checks: data["hard_fail_checks"]
                        .as_array()
                        .expect("hard_fail_checks")
                        .iter()
                        .map(|c| c.as_str().unwrap_or_default().to_string())
                        .collect(),
                    not_applicable: string_map(&data["not_applicable"]).into_keys().collect(),
                });
            }
            "done" => {}
            _ => {
                let checks = data["checks"]
                    .as_array()
                    .unwrap_or_else(|| panic!("section {name} has no checks array"))
                    .iter()
                    .map(|c| Check {
                        name: str_of(c, "name"),
                        verdict: str_of(c, "verdict"),
                    })
                    .collect();
                sections.insert(
                    name.clone(),
                    Section {
                        status: str_of(data, "status"),
                        grade: data["grade"].as_str().map(str::to_string),
                        checks,
                    },
                );
            }
        }
    }
    Projection {
        summary: summary.expect("no summary event"),
        sections,
    }
}

/// Run one fixture through `POST /api/check` and return the serialised projection.
async fn run_fixture(name: &str) -> String {
    let config = production_config(fixture(name)).await;
    let state = AppState::new(config).unwrap();
    let (routes, _) = api_router().split_for_parts();
    let app = Router::new()
        .merge(routes.with_state(state))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));

    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"domain":"example.com"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    let events = parse_sse(core::str::from_utf8(&bytes).unwrap());

    let mut out = serde_json::to_string_pretty(&project(&events)).unwrap();
    out.push('\n');
    out
}

/// JSON paths where `expected` and `actual` differ, each as `path: expected X, actual Y`.
fn diff_fields(expected: &Value, actual: &Value) -> Vec<String> {
    fn walk(path: &str, e: &Value, a: &Value, out: &mut Vec<String>) {
        match (e, a) {
            (Value::Object(eo), Value::Object(ao)) => {
                let keys: std::collections::BTreeSet<_> = eo.keys().chain(ao.keys()).collect();
                for k in keys {
                    let p = format!("{path}.{k}");
                    match (eo.get(k), ao.get(k)) {
                        (Some(x), Some(y)) => walk(&p, x, y, out),
                        (Some(x), None) => out.push(format!("{p}: expected {x}, actual <missing>")),
                        (None, Some(y)) => out.push(format!("{p}: expected <missing>, actual {y}")),
                        (None, None) => {}
                    }
                }
            }
            (Value::Array(ea), Value::Array(aa)) => {
                for i in 0..ea.len().max(aa.len()) {
                    let p = format!("{path}[{i}]");
                    match (ea.get(i), aa.get(i)) {
                        (Some(x), Some(y)) => walk(&p, x, y, out),
                        (Some(x), None) => out.push(format!("{p}: expected {x}, actual <missing>")),
                        (None, Some(y)) => out.push(format!("{p}: expected <missing>, actual {y}")),
                        (None, None) => {}
                    }
                }
            }
            _ if e != a => out.push(format!("{path}: expected {e}, actual {a}")),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk("$", expected, actual, &mut out);
    out
}

/// Compare the fixture's projection with its golden, or rewrite the golden.
async fn check_golden(name: &str) -> Value {
    let actual = run_fixture(name).await;
    let path = contracts_dir().join(format!("lens-{name}.json"));
    if std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &actual).unwrap();
    } else {
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "golden {} unreadable: {e} (UPDATE_GOLDEN=1 writes it)",
                path.display()
            )
        });
        let diffs = diff_fields(
            &serde_json::from_str(&expected).unwrap(),
            &serde_json::from_str(&actual).unwrap(),
        );
        assert!(
            diffs.is_empty(),
            "lens-{name}.json differs from today's projection:\n  {}\n(UPDATE_GOLDEN=1 rewrites the golden)",
            diffs.join("\n  ")
        );
    }
    serde_json::from_str(&actual).unwrap()
}

fn assert_no_error_section(name: &str, projection: &Value) {
    for (section, s) in projection["sections"].as_object().unwrap() {
        assert_ne!(
            s["status"], "error",
            "{name}: section {section} errors, the harness is broken"
        );
    }
}

#[tokio::test]
async fn lens_golden_healthy() {
    let p = check_golden("healthy").await;
    assert_no_error_section("healthy", &p);
}

#[tokio::test]
async fn lens_golden_no_mx() {
    let p = check_golden("no-mx").await;
    assert_no_error_section("no-mx", &p);
    // Today lens marks the mail buckets `skip`, not `not_applicable`; the golden pins that.
    let skipped = p["sections"]["email"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["verdict"] == "skip")
        .count();
    assert!(skipped > 0, "no-mx must skip the mail buckets");
}

/// A null MX domain sends no mail: infrastructure, transport and brand are skipped, the
/// authentication bucket is still scored, and the verdict is complete.
#[tokio::test]
async fn lens_golden_null_mx() {
    let projection: Value = serde_json::from_str(&run_fixture("null-mx").await).unwrap();
    assert_no_error_section("null-mx", &projection);
    let summary = &projection["summary"];
    assert_eq!(
        summary["complete"], true,
        "null-mx: the verdict is complete; summary: {summary}"
    );
    let checks = projection["sections"]["email"]["checks"]
        .as_array()
        .unwrap();
    let verdict = |name: &str| {
        checks
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("null-mx: no email check {name}; checks: {checks:?}"))["verdict"]
            .clone()
    };
    for bucket in [
        "email_infrastructure",
        "email_transport",
        "email_brand_policy",
    ] {
        assert_eq!(verdict(bucket), "skip", "null-mx: {bucket} is skipped");
    }
    assert_eq!(
        verdict("email_authentication"),
        "pass",
        "null-mx: authentication is scored"
    );
    assert_lens_golden_exists("null-mx");
    check_golden("null-mx").await;
}

#[tokio::test]
async fn lens_golden_mx_cname() {
    let p = check_golden("mx-cname").await;
    assert_no_error_section("mx-cname", &p);
}

/// Phase 2 decides these fields; they are asserted before the golden file is compared, so the
/// test fails on them while the verdict is still a letter grade.
#[tokio::test]
async fn lens_golden_no_address_records() {
    let projection: Value = serde_json::from_str(&run_fixture("no-address-records").await).unwrap();
    let summary = &projection["summary"];
    assert_eq!(
        summary["grade"], "incomplete",
        "no-address-records: tlsight and spectra error, so the grade is `incomplete`; summary: {summary}"
    );
    assert_eq!(
        summary["complete"], false,
        "no-address-records: the summary must say `complete: false`; summary: {summary}"
    );
    // The full projection is pinned by lens-no-address-records.json, generated with
    // UPDATE_GOLDEN=1 once Phase 2's code lands.
    let path = contracts_dir().join("lens-no-address-records.json");
    assert!(
        path.exists(),
        "golden {} is missing (UPDATE_GOLDEN=1 writes it after Phase 2)",
        path.display()
    );
    check_golden("no-address-records").await;
}

/// The tlsight goldens of these fixtures come from the tlsight producer test; a missing one
/// fails with the producer command instead of a bare read error.
fn require_tlsight_golden(name: &str) {
    let file = fixture(name).tls.expect("fixture has a tls golden");
    let path = contracts_dir().join(file);
    assert!(
        path.exists(),
        "tlsight golden {} is missing; write it with `UPDATE_GOLDEN=1 cargo test -p tlsight --test contract_golden`",
        path.display()
    );
}

fn assert_lens_golden_exists(name: &str) {
    let path = contracts_dir().join(format!("lens-{name}.json"));
    assert!(
        path.exists(),
        "golden {} is missing (UPDATE_GOLDEN=1 writes it after the code lands)",
        path.display()
    );
}

/// A host that answers HTTP but not TLS is a hard fail: grade F, `tls_reachable` named,
/// and the verdict is complete (tlsight answered).
#[tokio::test]
async fn lens_golden_http_only() {
    require_tlsight_golden("http-only");
    let projection: Value = serde_json::from_str(&run_fixture("http-only").await).unwrap();
    let summary = &projection["summary"];
    assert_eq!(
        summary["grade"], "F",
        "http-only: an unreachable TLS endpoint grades F; summary: {summary}"
    );
    let checks = summary["hard_fail_checks"].as_array().unwrap();
    assert!(
        checks.iter().any(|c| c == "tls_reachable"),
        "http-only: hard_fail_checks must contain tls_reachable; summary: {summary}"
    );
    assert_eq!(
        summary["complete"], true,
        "http-only: the verdict is complete; summary: {summary}"
    );
    assert_lens_golden_exists("http-only");
    check_golden("http-only").await;
}

/// tlsight answered but tested nothing: no weighted TLS check, so the verdict is incomplete.
#[tokio::test]
async fn lens_golden_no_weighted_tls() {
    require_tlsight_golden("no-weighted-tls");
    let projection: Value = serde_json::from_str(&run_fixture("no-weighted-tls").await).unwrap();
    let summary = &projection["summary"];
    assert_eq!(
        summary["grade"], "incomplete",
        "no-weighted-tls: no weighted TLS check ran; summary: {summary}"
    );
    assert_eq!(
        summary["complete"], false,
        "no-weighted-tls: the summary must say `complete: false`; summary: {summary}"
    );
    assert_eq!(
        summary["sections"]["tls"], "error",
        "no-weighted-tls: the tls section must be `error`; summary: {summary}"
    );
    assert_lens_golden_exists("no-weighted-tls");
    check_golden("no-weighted-tls").await;
}

#[tokio::test]
async fn lens_golden_projection_is_byte_stable() {
    let first = run_fixture("healthy").await;
    let second = run_fixture("healthy").await;
    assert_eq!(first, second);
}

#[test]
fn lens_golden_diff_names_the_differing_field() {
    let expected = serde_json::json!({"summary": {"grade": "A", "score": 90.0}, "sections": {"dns": {"checks": [{"name": "ns", "verdict": "pass"}]}}});
    let actual = serde_json::json!({"summary": {"grade": "B", "score": 90.0}, "sections": {"dns": {"checks": [{"name": "ns", "verdict": "pass"}]}}});
    let diffs = diff_fields(&expected, &actual);
    assert_eq!(diffs.len(), 1, "{diffs:?}");
    assert!(diffs[0].contains("$.summary.grade"), "{diffs:?}");
    assert!(
        diffs[0].contains("\"A\"") && diffs[0].contains("\"B\""),
        "{diffs:?}"
    );

    let nested =
        serde_json::json!({"sections": {"dns": {"checks": [{"name": "ns", "verdict": "fail"}]}}});
    let diffs = diff_fields(&expected["sections"], &nested["sections"]);
    assert_eq!(
        diffs,
        vec![r#"$.dns.checks[0].verdict: expected "pass", actual "fail""#]
    );
}
