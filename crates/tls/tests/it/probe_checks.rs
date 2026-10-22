// v2-tls-versions-coop, Phase 3 (requirements 7, 8 and 9, criteria C8-C17): `translate` emits
// `tls.legacy_versions` (SSLv3, TLS 1.0, TLS 1.1 accepted is a fail) and `tls.weak_suites`
// (insecure group fail, CBC or static RSA warn), each judged per address and then over the
// addresses; both reference the `tls.probes` blocks of the addresses at the reported status plus
// `tls.protocols`, an unmeasured one none. The module's checks are the 23 catalogue TLS ids.
//
// Spec: specs/features/v2-tls-versions-coop/spec.md, Phase 3.

use std::collections::BTreeSet;
use std::path::PathBuf;

use netray_engine::profile::Profiles;
use netray_engine::{SectionEvent, SectionOutcome, catalogue, score};
use netray_model::{CheckResult, Protocol, Status};
use netray_tls::inspection::InspectResponse;
use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};
use netray_tls::translate;
use serde_json::{Value as Json, json};

const A: &str = "192.0.2.10";
const B: &str = "192.0.2.11";

fn fixture() -> Json {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn out(verdict: Verdict) -> ProbeOutcome {
    ProbeOutcome {
        verdict,
        version: None,
        cipher_suite: None,
        detail: match verdict {
            Verdict::Accepted => "server_hello".into(),
            Verdict::Refused => "alert protocol_version".into(),
            Verdict::Unanswered => "timeout".into(),
        },
    }
}

fn accepted_with(suite: &str) -> ProbeOutcome {
    ProbeOutcome {
        cipher_suite: Some(suite.into()),
        ..out(Verdict::Accepted)
    }
}

/// Every legacy version and weak suite group refused; TLS 1.2 accepted.
fn clean() -> ProtocolProbes {
    ProtocolProbes {
        ssl3: out(Verdict::Refused),
        tls1_0: out(Verdict::Refused),
        tls1_1: out(Verdict::Refused),
        tls1_2: out(Verdict::Accepted),
        tls1_3: out(Verdict::Refused),
        suites_insecure: out(Verdict::Refused),
        suites_cbc: out(Verdict::Refused),
        suites_static_rsa: out(Verdict::Refused),
    }
}

/// The fixture's first address carries `first`; with a second one, it is a clone carrying
/// `second`.
fn response(first: Option<ProtocolProbes>, second: Option<ProtocolProbes>) -> InspectResponse {
    let mut doc = fixture();
    if second.is_some() {
        let mut clone = doc["ports"][0]["ips"][0].clone();
        clone["ip"] = json!(B);
        doc["ports"][0]["ips"].as_array_mut().unwrap().push(clone);
        let port = doc["ports"][0].as_object_mut().unwrap();
        port.remove("quality");
        port.remove("consistency");
    }
    let mut resp: InspectResponse = serde_json::from_value(doc).unwrap();
    resp.ports[0].ips[0].probes = first;
    if let Some(p) = second {
        resp.ports[0].ips[1].probes = Some(p);
    }
    resp
}

fn outcome_checks(outcome: &SectionOutcome) -> &Vec<CheckResult> {
    match outcome {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn check(outcome: &SectionOutcome, id: &str) -> CheckResult {
    outcome_checks(outcome)
        .iter()
        .find(|c| c.id.to_string() == id)
        .unwrap_or_else(|| panic!("no check {id}"))
        .clone()
}

fn mentions(c: &CheckResult, needle: &str) -> bool {
    c.findings.iter().any(|f| f.contains(needle))
}

fn status_of(resp: &InspectResponse, id: &str) -> Status {
    check(&translate(resp), id).status
}

#[test]
fn legacy_versions_fail_on_an_accepted_tls_1_0_and_reference_the_blocks() {
    let mut p = clean();
    p.tls1_0 = out(Verdict::Accepted);
    let outcome = translate(&response(Some(p), None));
    let c = check(&outcome, "tls.legacy_versions");
    assert_eq!(c.status, Status::Fail, "{c:?}");
    assert!(mentions(&c, "TLS 1.0"), "{:?}", c.findings);
    let got: BTreeSet<String> = c.evidence.iter().map(|b| b.to_string()).collect();
    let want: BTreeSet<String> = [format!("tls.probes@{A}:443"), "tls.protocols".to_string()]
        .into_iter()
        .collect();
    assert_eq!(got, want);
}

#[test]
fn legacy_versions_pass_when_all_three_are_refused() {
    let resp = response(Some(clean()), Some(clean()));
    assert_eq!(status_of(&resp, "tls.legacy_versions"), Status::Pass);
}

#[test]
fn legacy_versions_unanswered_is_unmeasured_and_leaves_the_tls_section_unscored() {
    let mut p = clean();
    p.tls1_1 = out(Verdict::Unanswered);
    let outcome = translate(&response(Some(p), None));
    let c = check(&outcome, "tls.legacy_versions");
    assert_eq!(c.status, Status::Unmeasured, "{c:?}");
    assert!(mentions(&c, "TLS 1.1"), "{:?}", c.findings);
    assert!(c.evidence.is_empty(), "{:?}", c.evidence);

    let s = score(
        Profiles::current(),
        &[SectionEvent {
            protocol: Protocol::Tls,
            outcome,
        }],
    );
    assert!(!s.sections.contains_key(&Protocol::Tls), "{:?}", s.sections);
    assert!(!s.complete);
}

#[test]
fn legacy_versions_a_fail_beats_an_unmeasured_address() {
    let mut a = clean();
    a.tls1_0 = out(Verdict::Accepted);
    let mut b = clean();
    b.tls1_1 = out(Verdict::Unanswered);
    let resp = response(Some(a), Some(b));
    assert_eq!(status_of(&resp, "tls.legacy_versions"), Status::Fail);
}

#[test]
fn weak_suites_fail_names_the_group_and_the_suite() {
    let mut p = clean();
    p.suites_insecure = accepted_with("TLS_RSA_WITH_RC4_128_SHA");
    let outcome = translate(&response(Some(p), None));
    let c = check(&outcome, "tls.weak_suites");
    assert_eq!(c.status, Status::Fail, "{c:?}");
    assert!(mentions(&c, "insecure"), "{:?}", c.findings);
    assert!(mentions(&c, "TLS_RSA_WITH_RC4_128_SHA"), "{:?}", c.findings);
}

#[test]
fn weak_suites_verdict_by_probe_outcome() {
    use Verdict::*;
    // (criterion, insecure, cbc, static_rsa, status)
    let rows = [
        ("C13 cbc", Refused, Accepted, Refused, Status::Warn),
        ("C13 static rsa", Refused, Refused, Accepted, Status::Warn),
        ("C14", Refused, Refused, Refused, Status::Pass),
        ("C15", Unanswered, Accepted, Refused, Status::Unmeasured),
    ];
    for (name, insecure, cbc, rsa, want) in rows {
        let mut p = clean();
        p.suites_insecure = out(insecure);
        p.suites_cbc = out(cbc);
        p.suites_static_rsa = out(rsa);
        assert_eq!(
            status_of(&response(Some(p), None), "tls.weak_suites"),
            want,
            "{name}"
        );
    }
}

#[test]
fn without_probes_both_checks_are_not_applicable_not_reported() {
    let outcome = translate(&response(None, None));
    for id in ["tls.legacy_versions", "tls.weak_suites"] {
        let c = check(&outcome, id);
        assert_eq!(c.status, Status::NotApplicable, "{id}");
        assert_eq!(c.findings, vec!["not reported".to_string()], "{id}");
    }
}

#[test]
fn without_a_handshake_both_checks_are_not_applicable_no_tls_handshake() {
    let mut doc = fixture();
    let port = doc["ports"][0].as_object_mut().unwrap();
    port.remove("quality");
    port.remove("consistency");
    port.insert(
        "ips".into(),
        json!([
            { "ip": A, "ip_version": "v4",
              "error": { "code": "HANDSHAKE_FAILED", "message": "connection refused" } },
            { "ip": B, "ip_version": "v4",
              "error": { "code": "HANDSHAKE_FAILED", "message": "connection refused" } }
        ]),
    );
    let resp: InspectResponse = serde_json::from_value(doc).unwrap();
    let outcome = translate(&resp);
    for id in ["tls.legacy_versions", "tls.weak_suites"] {
        let c = check(&outcome, id);
        assert_eq!(c.status, Status::NotApplicable, "{id}");
        assert_eq!(c.findings, vec!["no TLS handshake".to_string()], "{id}");
    }
}

#[test]
fn the_module_emits_the_23_catalogue_tls_ids() {
    let outcome = translate(&response(None, None));
    let got: BTreeSet<String> = outcome_checks(&outcome)
        .iter()
        .map(|c| c.id.to_string())
        .collect();
    let want: BTreeSet<String> = catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Tls)
        .map(|e| e.id.to_string())
        .collect();
    assert_eq!(want.len(), 23, "{want:?}");
    assert_eq!(got.len(), 23, "{got:?}");
    assert_eq!(got, want);
}
