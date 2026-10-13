//! Evidence on the lens wire (v2-evidence Phase 6: C2 wire shape, C3, C4).
//!
//! Every section carries `evidence: [Block]`, every check item `evidence: [block-id]`, the summary
//! `evidence_schema: 1`; the dns section also carries the facts blocks no section carries.
//! Unresolved references are dropped (A5). Removing every `evidence` key (and
//! `summary.evidence_schema`) leaves the existing `lens-full-*` goldens identical (C4).
//! The harness mirrors `lens_full_output.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::PathBuf;

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden, email_golden, facts_golden, http_module, ip_golden, registry_with, tls_module,
};
use lens::config::Config;
use lens::routes::api_router;
use lens::state::AppState;
use serde_json::Value;
use tower::ServiceExt;

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

async fn production_config() -> Config {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.snapshots.enabled = false;
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

async fn run_full(f: &Fixture) -> Value {
    let state = AppState::with_registry(
        production_config().await,
        registry_with(
            dns_golden(f.dns),
            facts_golden(f.dns),
            http_module(f.http),
            email_golden(f.email),
            ip_golden(f.ip),
            tls_module(f.tls),
        ),
    )
    .unwrap();
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

/// JSON paths where `expected` and `actual` differ.
fn diff_paths(expected: &Value, actual: &Value) -> Vec<String> {
    fn walk(path: &str, e: &Value, a: &Value, out: &mut Vec<String>) {
        match (e, a) {
            (Value::Object(eo), Value::Object(ao)) => {
                let keys: BTreeSet<_> = eo.keys().chain(ao.keys()).collect();
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
    walk("", expected, actual, &mut out);
    out
}

const SECTIONS: [&str; 5] = ["dns", "tls", "http", "email", "ip"];

/// Remove every `evidence` key at any depth, and `evidence_schema` (the summary's).
fn strip_evidence(v: &mut Value) {
    match v {
        Value::Object(o) => {
            o.remove("evidence");
            o.remove("evidence_schema");
            o.values_mut().for_each(strip_evidence);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_evidence),
        _ => {}
    }
}

/// C3: every check item's evidence ids resolve to a block of the run; every judged check has one.
#[tokio::test]
async fn evidence_references_resolve_and_judged_checks_have_evidence() {
    let mut failures = Vec::new();
    for f in all_fixtures() {
        let out = run_full(&f).await;
        if out["summary"]["evidence_schema"] != 1 {
            failures.push(format!("{}: summary.evidence_schema is not 1", f.name));
        }
        let ids: BTreeSet<String> = SECTIONS
            .iter()
            .filter_map(|s| out.get(*s))
            .filter_map(|s| s.get("evidence")?.as_array())
            .flatten()
            .filter_map(|b| b["id"].as_str().map(str::to_string))
            .collect();
        for s in SECTIONS {
            let Some(section) = out.get(s) else { continue };
            if !section["evidence"].is_array() {
                failures.push(format!("{}: {s} has no evidence array", f.name));
            }
            for c in section["checks"].as_array().into_iter().flatten() {
                let name = c["name"].as_str().unwrap_or("?");
                let Some(refs) = c["evidence"].as_array() else {
                    failures.push(format!("{}: {s}.{name} has no evidence array", f.name));
                    continue;
                };
                for r in refs {
                    if !r.as_str().is_some_and(|r| ids.contains(r)) {
                        failures.push(format!(
                            "{}: {s}.{name} references unknown block {r}",
                            f.name
                        ));
                    }
                }
                let judged = matches!(c["verdict"].as_str(), Some("pass" | "warn" | "fail"));
                if judged && refs.is_empty() {
                    failures.push(format!(
                        "{}: judged check {s}.{name} has no evidence",
                        f.name
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "evidence wire:\n  {}",
        failures.join("\n  ")
    );
}

/// C4: without evidence the output equals the existing golden.
#[tokio::test]
async fn output_without_evidence_equals_goldens() {
    let mut failures = Vec::new();
    for f in all_fixtures() {
        let mut actual = run_full(&f).await;
        strip_evidence(&mut actual);
        let path = contracts_dir().join(format!("lens-full-{}.json", f.name));
        let expected: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("golden {} is not JSON: {e}", path.display()));
        for d in diff_paths(&expected, &actual) {
            failures.push(format!("{}: {d}", f.name));
        }
    }
    assert!(
        failures.is_empty(),
        "pre-existing fields changed:\n  {}",
        failures.join("\n  ")
    );
}

/// C2 wire shape: the healthy dns section carries a `dns.a` block whose values are tagged `{t, v}`.
#[tokio::test]
async fn healthy_dns_section_carries_tagged_dns_a_block() {
    fn count_tagged(v: &Value) -> usize {
        match v {
            Value::Object(o) => {
                usize::from(o.len() == 2 && o.contains_key("t") && o.contains_key("v"))
                    + o.values().map(count_tagged).sum::<usize>()
            }
            Value::Array(a) => a.iter().map(count_tagged).sum(),
            _ => 0,
        }
    }
    let out = run_full(&HEALTHY).await;
    let blocks = out["dns"]["evidence"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let block = blocks
        .iter()
        .find(|b| b["id"] == "dns.a")
        .unwrap_or_else(|| panic!("no dns.a block in dns.evidence: {blocks:?}"));
    assert!(block["title"].is_string(), "{block}");
    assert!(block["shape"]["kind"].is_string(), "{block}");
    assert!(
        count_tagged(&block["shape"]["data"]) > 0,
        "dns.a block has no tagged {{t, v}} values: {block}"
    );
}
