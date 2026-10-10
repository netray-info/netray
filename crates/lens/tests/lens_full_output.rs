//! Full-output golden test: lens's whole sync result (`POST /api/check` with `stream:false`)
//! for every `lens_golden` fixture plus three beacon scenarios, compared with
//! `tests/fixtures/contracts/lens-full-<fixture>.json`. Unlike `lens_golden`, which projects
//! verdicts only, this keeps sections, checks with messages, headlines, extras, score and grade.
//! `UPDATE_GOLDEN=1 cargo test -p lens --test lens_full_output` rewrites the goldens.
//!
//! Volatile keys removed before comparison, at any depth: `duration_ms` (`done`),
//! `response_duration_ms` (http section), `request_id`, `snapshot_id` (`done`, absent while
//! snapshots are disabled), `cached_at`, `timestamp`, and any key ending in `_at`. The
//! `detail_url` fields embed only the domain, never a stub port or an id, and stay.

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
use serde_json::Value;
use tower::ServiceExt;

/// One fixture: a name and the golden file each backend serves. `None` for tls or http means
/// that backend answers HTTP 500.
#[derive(Clone)]
struct Fixture {
    name: &'static str,
    dns: &'static str,
    tls: Option<&'static str>,
    http: Option<&'static str>,
    email: &'static str,
    ip: &'static str,
}

const HEALTHY: Fixture = Fixture {
    name: "healthy",
    dns: "prism.sse",
    tls: Some("tlsight-inspect.json"),
    http: Some("spectra-inspect.json"),
    email: "beacon.sse",
    ip: "ifconfig-json.json",
};

/// The fixtures of `lens_golden`, plus three beacon scenarios built from `healthy`.
fn all_fixtures() -> Vec<Fixture> {
    let mut v = vec![
        HEALTHY,
        Fixture {
            name: "no-mx",
            email: "beacon-no-mx.sse",
            ..HEALTHY
        },
        Fixture {
            name: "mx-cname",
            email: "beacon-mx-cname.sse",
            ..HEALTHY
        },
        Fixture {
            name: "no-address-records",
            dns: "prism-no-address.sse",
            tls: None,
            http: None,
            ..HEALTHY
        },
        Fixture {
            name: "http-only",
            tls: Some("tlsight-unreachable.json"),
            ..HEALTHY
        },
        Fixture {
            name: "no-weighted-tls",
            tls: Some("tlsight-not-tested.json"),
            ..HEALTHY
        },
        Fixture {
            name: "null-mx",
            email: "beacon-null-mx.sse",
            ..HEALTHY
        },
    ];
    for (name, email) in [
        ("beacon-timeout", "beacon-timeout.sse"),
        ("beacon-partial", "beacon-partial.sse"),
        ("beacon-sending-no-dkim", "beacon-sending-no-dkim.sse"),
    ] {
        v.push(Fixture {
            name,
            email,
            ..HEALTHY
        });
    }
    v
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
/// address policy does not enrich. The harness serves it with a public stand-in so the IP
/// section stays scored (same as `lens_golden`).
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

fn is_volatile(key: &str) -> bool {
    matches!(
        key,
        "duration_ms"
            | "response_duration_ms"
            | "request_id"
            | "snapshot_id"
            | "cached_at"
            | "timestamp"
    ) || key.ends_with("_at")
}

/// Remove every volatile key at any depth.
fn strip_volatile(v: &mut Value) {
    match v {
        Value::Object(o) => {
            o.retain(|k, _| !is_volatile(k));
            o.values_mut().for_each(strip_volatile);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_volatile),
        _ => {}
    }
}

/// Rebuild with sorted keys, whatever order serde_json keeps.
fn sorted(v: Value) -> Value {
    match v {
        Value::Object(o) => {
            let m: BTreeMap<String, Value> = o.into_iter().map(|(k, v)| (k, sorted(v))).collect();
            Value::Object(m.into_iter().collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        other => other,
    }
}

/// Run one fixture through the sync `POST /api/check`; return the stripped, key-sorted output.
async fn run_full(f: &Fixture) -> Value {
    let config = production_config(f).await;
    let state = AppState::new(config).unwrap();
    let (routes, _) = api_router().split_for_parts();
    let app = Router::new()
        .merge(routes.with_state(state))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));

    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"domain":"example.com","stream":false}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "{}: sync check", f.name);
    let bytes = to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    let mut value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("{}: sync body is not JSON: {e}", f.name));
    strip_volatile(&mut value);
    sorted(value)
}

/// JSON paths where `expected` and `actual` differ, each as `sections.http.headline: expected
/// X, actual Y` (object keys joined by `.`, array indices as `[i]`).
fn diff_paths(expected: &Value, actual: &Value) -> Vec<String> {
    fn join(path: &str, k: &str) -> String {
        if path.is_empty() {
            k.to_string()
        } else {
            format!("{path}.{k}")
        }
    }
    fn side(p: &str, x: Option<&Value>, y: Option<&Value>, out: &mut Vec<String>) {
        match (x, y) {
            (Some(x), Some(y)) => walk(p, x, y, out),
            (Some(x), None) => out.push(format!("{p}: expected {x}, actual <missing>")),
            (None, Some(y)) => out.push(format!("{p}: expected <missing>, actual {y}")),
            (None, None) => {}
        }
    }
    fn walk(path: &str, e: &Value, a: &Value, out: &mut Vec<String>) {
        match (e, a) {
            (Value::Object(eo), Value::Object(ao)) => {
                let keys: std::collections::BTreeSet<_> = eo.keys().chain(ao.keys()).collect();
                for k in keys {
                    side(&join(path, k), eo.get(k), ao.get(k), out);
                }
            }
            (Value::Array(ea), Value::Array(aa)) => {
                for i in 0..ea.len().max(aa.len()) {
                    side(&format!("{path}[{i}]"), ea.get(i), aa.get(i), out);
                }
            }
            _ if e != a => out.push(format!("{path}: expected {e}, actual {a}")),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk("", expected, actual, &mut out);
    out
}

/// Every fixture's full output equals its golden; failures name the fixture and the path.
#[tokio::test]
async fn lens_full_output_matches_goldens() {
    let update = std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1");
    let mut failures = Vec::new();
    for f in all_fixtures() {
        let actual = run_full(&f).await;
        let path = contracts_dir().join(format!("lens-full-{}.json", f.name));
        if update {
            let mut out = serde_json::to_string_pretty(&actual).unwrap();
            out.push('\n');
            std::fs::write(&path, out).unwrap();
            continue;
        }
        let expected = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str::<Value>(&s)
                .unwrap_or_else(|e| panic!("golden {} is not JSON: {e}", path.display())),
            Err(e) => {
                failures.push(format!(
                    "{}: golden {} unreadable: {e} (UPDATE_GOLDEN=1 writes it)",
                    f.name,
                    path.display()
                ));
                continue;
            }
        };
        for d in diff_paths(&expected, &actual) {
            failures.push(format!("{}: {d}", f.name));
        }
    }
    assert!(
        failures.is_empty(),
        "lens full output differs from its goldens:\n  {}\n(UPDATE_GOLDEN=1 rewrites them)",
        failures.join("\n  ")
    );
}

/// C3: the comparator names the path of a single nested difference.
#[test]
fn lens_full_output_diff_names_the_nested_path() {
    let expected = serde_json::json!({"sections": {"http": {"headline": "all good", "score": 1}}});
    let actual = serde_json::json!({"sections": {"http": {"headline": "one issue", "score": 1}}});
    let diffs = diff_paths(&expected, &actual);
    assert_eq!(diffs.len(), 1, "{diffs:?}");
    assert!(diffs[0].starts_with("sections.http.headline:"), "{diffs:?}");
    assert!(diff_paths(&expected, &expected).is_empty());
}

#[test]
fn lens_full_output_strips_volatile_keys() {
    let mut v = serde_json::json!({
        "done": {"duration_ms": 5, "domain": "example.com"},
        "a": [{"request_id": "x", "keep": 1, "cached_at": 3}]
    });
    strip_volatile(&mut v);
    assert_eq!(
        v,
        serde_json::json!({"done": {"domain": "example.com"}, "a": [{"keep": 1}]})
    );
}
