//! Golden test: the verdict lens computes from the committed backend goldens.
//!
//! The DNS, TLS, HTTP, email and IP sections come from `netray_dns`'s, `netray_tls`'s,
//! `netray_http`'s, `netray_email`'s and `netray_ip`'s `golden_module` through the engine registry
//! on the goldens in `tests/fixtures/contracts/`; the IP module samples the addresses the
//! resolve stage answers. The apex runs with the admission and scoring settings of lens's
//! `tests/fixtures/lens.production.toml` and the run is admitted through
//! `netray_web::admission::admit`. A projection of the published output (grades, score,
//! completeness, hard fails, not-applicable sections, check verdicts; no prose, durations or
//! ids) is compared with the same fields of `tests/fixtures/contracts/lens-<fixture>.json`.
//!
//! Copied from crates/lens/tests/lens_golden.rs (spec v2-cutover, Phase 6); dropped as V1: the
//! `POST /api/check` SSE wire and the fields of the projection only it computed (the
//! `summary.sections` and `sections.*.status` status words, `summary.overall`, and
//! `sections.email.grade`, beacon's own grade), and `UPDATE_GOLDEN=1`: the lens goldens are
//! read here, never rewritten.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use crate::common;

use common::{
    dns_golden, email_golden, facts_golden, http_module, ip_golden, registry_with, tls_module,
};
use netray_engine::domain_run::{CheckOutput, CheckVerdict, grade_label};
use netray_engine::profile::Profile;
use netray_web::admission::{Admit, Trigger, admit, until_done};
use serde::Serialize;
use serde_json::Value;

/// One fixture: a name and the golden file each module serves. `None` for tls or http means
/// that module is incomplete.
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

#[derive(Serialize)]
struct Summary {
    grade: String,
    score: f64,
    complete: Option<bool>,
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
    checks: Vec<Check>,
}

#[derive(Serialize)]
struct Projection {
    summary: Summary,
    sections: BTreeMap<String, Section>,
}

/// The verdict word lens's wire carried for `verdict`.
fn verdict_word(verdict: &CheckVerdict) -> &'static str {
    match verdict {
        CheckVerdict::Pass => "pass",
        CheckVerdict::Warn => "warn",
        CheckVerdict::Fail | CheckVerdict::NotFound => "fail",
        CheckVerdict::Skip => "skip",
        CheckVerdict::Unmeasured => "unmeasured",
    }
}

/// The letter of `percentage` under `profile`'s thresholds, as lens's `grade_for`.
fn grade_for(profile: &Profile, percentage: f64) -> String {
    profile
        .thresholds
        .iter()
        .find(|(_, t)| percentage >= *t as f64)
        .map_or("F", |(g, _)| grade_label(*g))
        .to_string()
}

fn project(output: &CheckOutput, profile: &Profile) -> Projection {
    let score = &output.score;
    let mut not_applicable: Vec<String> = score.not_applicable.keys().cloned().collect();
    not_applicable.sort();
    let summary = Summary {
        grade: score.grade.clone(),
        score: (score.overall_percentage * 10.0).round() / 10.0,
        complete: Some(score.complete),
        section_grades: score
            .sections
            .iter()
            .map(|(name, s)| (name.clone(), grade_for(profile, s.percentage)))
            .collect(),
        hard_fail: score.hard_fail_triggered,
        hard_fail_checks: score.hard_fail_checks.clone(),
        not_applicable,
    };
    let sections = output
        .sections
        .iter()
        .map(|(name, result)| {
            let checks = result
                .as_ref()
                .map(|r| {
                    r.checks
                        .iter()
                        .map(|c| Check {
                            name: c.name.clone(),
                            verdict: verdict_word(&c.verdict).to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            (name.clone(), Section { checks })
        })
        .collect();
    Projection { summary, sections }
}

/// Run one fixture through admission and return the serialised projection.
async fn run_fixture(name: &str) -> String {
    let f = fixture(name);
    let mut config = common::production_config();
    // Every run computes the verdict afresh instead of answering from the cache.
    config.cache_enabled = false;
    let registry = registry_with(
        dns_golden(f.dns),
        facts_golden(f.dns),
        http_module(f.http),
        email_golden(f.email),
        ip_golden(f.ip),
        tls_module(f.tls),
    );
    let plan = common::production_plan(&registry);
    let state = common::state_with(config, registry, plan);
    let client = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let Admit::Admitted(mut stream) = admit(&state, "example.com", client, Trigger::Api, false)
    else {
        panic!("{name}: the run is not admitted");
    };
    let published = until_done(&mut stream).await.expect("the run finishes");

    let mut out =
        serde_json::to_string_pretty(&project(&published.output, &state.scoring_profile)).unwrap();
    out.push('\n');
    out
}

/// The fields of a lens golden this projection compares: everything but the V1 status words,
/// `summary.overall` and a section's own grade.
fn expected_subset(golden: &Value) -> Value {
    let s = &golden["summary"];
    let summary = serde_json::json!({
        "grade": s["grade"],
        "score": s["score"],
        "complete": s["complete"],
        "section_grades": s["section_grades"],
        "hard_fail": s["hard_fail"],
        "hard_fail_checks": s["hard_fail_checks"],
        "not_applicable": s["not_applicable"],
    });
    let sections: serde_json::Map<String, Value> = golden["sections"]
        .as_object()
        .unwrap_or_else(|| panic!("golden has no sections: {golden}"))
        .iter()
        .map(|(name, sec)| (name.clone(), serde_json::json!({ "checks": sec["checks"] })))
        .collect();
    serde_json::json!({ "summary": summary, "sections": sections })
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

/// Compare the fixture's projection with the same fields of its lens golden.
async fn check_golden(name: &str) -> Value {
    let actual = run_fixture(name).await;
    let path = contracts_dir().join(format!("lens-{name}.json"));
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()));
    let expected = expected_subset(&serde_json::from_str(&expected).unwrap());
    let actual: Value = serde_json::from_str(&actual).unwrap();
    let diffs = diff_fields(&expected, &actual);
    assert!(
        diffs.is_empty(),
        "lens-{name}.json differs from today's projection:\n  {}",
        diffs.join("\n  ")
    );
    actual
}

/// No section of the fixture's run errored where the golden says it answered: an answering
/// section carries checks.
fn assert_no_error_section(name: &str, projection: &Value) {
    for (section, s) in projection["sections"].as_object().unwrap() {
        assert!(
            projection["summary"]["not_applicable"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n == section)
                || !s["checks"].as_array().unwrap().is_empty(),
            "{name}: section {section} errors, the harness is broken"
        );
    }
}

#[tokio::test]
async fn run_golden_healthy() {
    let p = check_golden("healthy").await;
    assert_no_error_section("healthy", &p);
}

#[tokio::test]
async fn run_golden_no_mx() {
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

/// A null MX domain sends no mail: the seven MX-dependent checks are skipped, spf and dmarc
/// are still scored, and the verdict is complete.
#[tokio::test]
async fn run_golden_null_mx() {
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
    for name in [
        "mx", "mta_sts", "tls_rpt", "bimi", "dane", "dnsbl", "fcrdns",
    ] {
        assert_eq!(verdict(name), "skip", "null-mx: {name} is skipped");
    }
    for name in ["spf", "dmarc"] {
        assert_eq!(verdict(name), "pass", "null-mx: {name} is scored");
    }
    assert_golden_exists("null-mx");
    check_golden("null-mx").await;
}

#[tokio::test]
async fn run_golden_mx_cname() {
    let p = check_golden("mx-cname").await;
    assert_no_error_section("mx-cname", &p);
}

#[tokio::test]
async fn run_golden_no_address_records() {
    let projection: Value = serde_json::from_str(&run_fixture("no-address-records").await).unwrap();
    let summary = &projection["summary"];
    assert_ne!(
        summary["grade"], "incomplete",
        "no-address-records: a definite negative keeps a letter grade; summary: {summary}"
    );
    assert_eq!(
        summary["complete"], true,
        "no-address-records: the summary must say `complete: true`; summary: {summary}"
    );
    let not_applicable = summary["not_applicable"]
        .as_array()
        .expect("not_applicable");
    for section in ["ip", "http", "tls"] {
        assert!(
            not_applicable.iter().any(|s| s == section),
            "no-address-records: {section} must be not applicable; summary: {summary}"
        );
    }
    // The full projection is pinned by lens-no-address-records.json.
    let path = contracts_dir().join("lens-no-address-records.json");
    assert!(path.exists(), "golden {} is missing", path.display());
    check_golden("no-address-records").await;
}

/// The tlsight goldens of these fixtures come from the tlsight producer test; a missing one
/// fails with the producer command instead of a bare read error.
fn require_tlsight_golden(name: &str) {
    let file = fixture(name).tls.expect("fixture has a tls golden");
    let path = contracts_dir().join(file);
    assert!(
        path.exists(),
        "tlsight golden {} is missing; write it with `UPDATE_GOLDEN=1 cargo test -p netray-tls --test contract_golden`",
        path.display()
    );
}

fn assert_golden_exists(name: &str) {
    let path = contracts_dir().join(format!("lens-{name}.json"));
    assert!(path.exists(), "golden {} is missing", path.display());
}

/// A host that answers HTTP but not TLS is a hard fail: grade F, `tls_reachable` named,
/// and the verdict is complete (tlsight answered).
#[tokio::test]
async fn run_golden_http_only() {
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
    assert_golden_exists("http-only");
    check_golden("http-only").await;
}

/// tlsight answered but tested nothing: no weighted TLS check, so the verdict is incomplete and
/// the TLS section has no section grade (lens's `sections.tls` was `error`; that status word is
/// V1 and dropped).
#[tokio::test]
async fn run_golden_no_weighted_tls() {
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
    assert!(
        summary["section_grades"].get("tls").is_none(),
        "no-weighted-tls: the tls section must not be scored; summary: {summary}"
    );
    assert_golden_exists("no-weighted-tls");
    check_golden("no-weighted-tls").await;
}

#[tokio::test]
async fn run_golden_projection_is_byte_stable() {
    let first = run_fixture("healthy").await;
    let second = run_fixture("healthy").await;
    assert_eq!(first, second);
}

#[test]
fn run_golden_diff_names_the_differing_field() {
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
