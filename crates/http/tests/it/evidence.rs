// V2 evidence, Phase 4 HTTP row (requirements 4-5, criteria C1-C4): `translate` emits blocks
// that fit `netray_http::module::REGISTER`, checks reference the blocks they judged, a probe
// that got no answer emits no block, and the volatile paths name registered keys.
//
// Spec: specs/features/v2-evidence/spec.md. Seam: `translate` over the spectra fixture, with
// probe errors injected as in probe_outcomes.rs. Assumed interface (absent today):
// `netray_http::module::REGISTER`, `http.server` carries `error_kind` when the HTTPS probe
// was refused, reset or failed TLS.

use std::collections::BTreeSet;

use netray_engine::{EvidencePath, Module, SectionOutcome};
use netray_http::inspect::assembler::InspectResponse;
use netray_http::module::{REGISTER, translate};
use netray_http::{HttpModule, ModuleConfig};
use netray_model::evidence::{Block, Shape, Value as Ev, check_evidence_rule, validate};
use netray_model::{CheckResult, Status};
use serde_json::{Value, json};

const SPECTRA_JSON: &str =
    include_str!("../../../../tests/fixtures/contracts/spectra-inspect.json");

const TEN: [&str; 10] = [
    "hsts",
    "csp",
    "x_frame_options",
    "x_content_type_options",
    "referrer_policy",
    "permissions_policy",
    "cookie_secure",
    "caching",
    "deprecated_headers",
    "info_leakage",
];

fn run(mutate: impl FnOnce(&mut Value)) -> (Vec<CheckResult>, Vec<Block>) {
    let mut v: Value = serde_json::from_str(SPECTRA_JSON).unwrap();
    mutate(&mut v);
    let resp: InspectResponse = serde_json::from_value(v).unwrap();
    match translate(&resp) {
        SectionOutcome::Measured {
            checks, evidence, ..
        } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn check<'a>(checks: &'a [CheckResult], name: &str) -> &'a CheckResult {
    let id = format!("http.{name}");
    checks
        .iter()
        .find(|c| c.id.to_string() == id)
        .unwrap_or_else(|| panic!("no check {id}"))
}

fn refs(checks: &[CheckResult], name: &str) -> Vec<String> {
    check(checks, name)
        .evidence
        .iter()
        .map(|b| b.to_string())
        .collect()
}

fn block<'a>(blocks: &'a [Block], id: &str) -> Option<&'a Block> {
    blocks.iter().find(|b| b.id.to_string() == id)
}

fn no_https_response(v: &mut Value, kind: &str, message: &str) {
    v["status"] = json!(0);
    v["headers"] = json!({});
    v["error"] = json!({"kind": kind, "message": message});
}

#[test]
fn fixture_blocks_fit_the_register_and_checks_reference_them() {
    let (checks, blocks) = run(|_| {});
    validate("http", &REGISTER, &blocks).expect("blocks fit the register");
    check_evidence_rule(&checks, &blocks).expect("evidence rule holds");

    assert_eq!(refs(&checks, "hsts"), vec!["http.headers"]);
    assert_eq!(
        refs(&checks, "https_redirect"),
        vec!["http.port80", "http.port80_probe"]
    );

    let headers = block(&blocks, "http.headers").expect("http.headers block");
    let Shape::Table { key, columns, rows } = &headers.shape else {
        panic!("http.headers must be a table, got {:?}", headers.shape);
    };
    assert_eq!(key, "name");
    let at = columns
        .iter()
        .position(|c| c == "name")
        .expect("name column");
    let mut seen = BTreeSet::new();
    for row in rows {
        let Ev::Text(name) = &row[at] else {
            panic!("name must be Text: {row:?}")
        };
        assert_eq!(name, &name.to_lowercase(), "header names are lowercased");
        assert!(seen.insert(name.clone()), "duplicate header row {name}");
    }
}

#[test]
fn https_timeout_emits_no_headers_block_and_references_nothing() {
    let (checks, blocks) = run(|v| no_https_response(v, "timeout", "https probe timed out"));
    assert!(block(&blocks, "http.headers").is_none(), "{blocks:?}");
    for name in TEN {
        let c = check(&checks, name);
        assert_eq!(c.status, Status::Unmeasured, "{name}");
        assert!(c.evidence.is_empty(), "{name}: {:?}", c.evidence);
    }
    validate("http", &REGISTER, &blocks).expect("blocks fit the register");
    check_evidence_rule(&checks, &blocks).expect("evidence rule holds");
}

#[test]
fn https_refused_fails_the_ten_against_http_server_with_error_kind() {
    let (checks, blocks) = run(|v| no_https_response(v, "refused", "connection refused"));
    assert!(block(&blocks, "http.headers").is_none(), "{blocks:?}");
    for name in TEN {
        assert_eq!(check(&checks, name).status, Status::Fail, "{name}");
        assert_eq!(refs(&checks, name), vec!["http.server"], "{name}");
    }
    let server = block(&blocks, "http.server").expect("http.server block");
    let Shape::Fields(fields) = &server.shape else {
        panic!("http.server must be fields, got {:?}", server.shape);
    };
    assert!(
        fields
            .iter()
            .any(|(k, v)| k == "error_kind" && *v == Ev::Text("refused".into())),
        "{fields:?}"
    );
    validate("http", &REGISTER, &blocks).expect("blocks fit the register");
    check_evidence_rule(&checks, &blocks).expect("evidence rule holds");
}

#[test]
fn volatile_paths_name_registered_keys() {
    let module = HttpModule::new(toml::from_str::<ModuleConfig>("").expect("empty config"))
        .expect("module builds");
    let volatile: &[EvidencePath] = module.volatile();
    let got: BTreeSet<&str> = volatile.iter().map(|p| p.0).collect();
    // response_ms per run; address, org and network_type follow the resolver's answer order
    // (specs/features/v2-history R17).
    let mut want: BTreeSet<String> = [
        "http.server/response_ms",
        "http.server/address",
        "http.server/org",
        "http.server/network_type",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    for h in [
        "date",
        "age",
        "set-cookie",
        "server-timing",
        "etag",
        "last-modified",
        "x-request-id",
        "cf-ray",
        "x-amz-cf-id",
        "x-correlation-id",
        "x-trace-id",
        "traceparent",
        "x-amzn-requestid",
        "expires",
        "x-timer",
        "x-served-by",
        "x-cache",
        "x-cache-hits",
        "cf-cache-status",
        "report-to",
        "via",
        "x-fastly-request-id",
        "x-vercel-id",
        "x-vercel-cache",
        "x-github-request-id",
        "x-amz-cf-pop",
    ] {
        want.insert(format!("http.headers/value@{h}"));
    }
    assert_eq!(
        got,
        want.iter().map(String::as_str).collect::<BTreeSet<_>>()
    );

    for path in volatile {
        let (id, key, _row) = EvidencePath::parse(path.0).expect(path.0);
        let name = format!("{}.{}", id.module(), id.name());
        assert!(
            REGISTER
                .keys
                .iter()
                .any(|(b, k, _)| *b == name && *k == key)
                || REGISTER.blocks.iter().any(|(b, _)| *b == name) && key == "value",
            "{}: key {key} not registered for {name}",
            path.0
        );
    }
}
