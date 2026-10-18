//! Contract tests for the evidence section's check rows (spec v2-inspectors, Phase 2,
//! requirement 3; C7, C8).
use netray_engine::catalogue::catalogue;
use netray_model::run::Run;
use netray_model::{CheckId, Protocol};
use netray_web::components::{RunLineView, Tone};
use netray_web::evidence::{EvidenceRow, EvidenceView, render_section, section};

const FINDING: &str =
    "The served certificate chain does not build to a trust anchor in the public trust stores.";
const FIX_HINT: &str =
    "Install a certificate from a public CA together with its full intermediate chain.";

fn run_json(check: &str) -> String {
    format!(
        r#"{{"profile_version":3,"engine_version":"0.26.0","evidence_schema_version":1,
        "grade":"B","complete":true,
        "sections":[{{"protocol":"tls","outcome":"measured","reason":null}}],
        "checks":[{check}],"blocks":[]}}"#
    )
}

fn run_line() -> RunLineView {
    RunLineView {
        parts: vec![
            "run of 2026-10-08 14:32 UTC".into(),
            "entry Ab3dE5fG".into(),
        ],
    }
}

fn view(check_json: &str) -> EvidenceView {
    let run: Run = serde_json::from_str(&run_json(check_json)).unwrap();
    section(&run, Protocol::Tls, run_line())
}

fn row<'a>(view: &'a EvidenceView, id: &str) -> &'a EvidenceRow {
    let short = &catalogue().get(&CheckId::parse(id).unwrap()).unwrap().short;
    view.rows.iter().find(|r| &r.check.code == short).unwrap()
}

#[test]
fn warn_without_findings_uses_catalogue_finding_and_fix_hint() {
    let v = view(r#"{"id":"tls.chain_trusted","status":"warn"}"#);
    let r = row(&v, "tls.chain_trusted");
    assert_eq!(r.check.tone, Tone::Warn);
    assert_eq!(r.check.body, FINDING);
    assert_eq!(r.check.fix.as_deref(), Some(FIX_HINT));
}

#[test]
fn fail_with_findings_uses_findings_as_body() {
    let v = view(
        r#"{"id":"tls.chain_trusted","status":"fail",
            "findings":["issuer unknown for example.com","chain ends at 192.0.2.7"]}"#,
    );
    let r = row(&v, "tls.chain_trusted");
    assert_eq!(r.check.tone, Tone::Fail);
    assert_eq!(
        r.check.body,
        "issuer unknown for example.com; chain ends at 192.0.2.7"
    );
    assert_eq!(r.check.fix.as_deref(), Some(FIX_HINT));
}

#[test]
fn pass_has_no_fix() {
    let v = view(r#"{"id":"tls.chain_trusted","status":"pass"}"#);
    let r = row(&v, "tls.chain_trusted");
    assert_eq!(r.check.tone, Tone::Pass);
    assert_eq!(r.check.fix, None);
}

#[test]
fn entry_without_findings_and_evidence_renders_catalogue_texts_without_block_link() {
    let v = view(r#"{"id":"tls.chain_trusted","status":"pass"}"#);
    let r = row(&v, "tls.chain_trusted");
    let spec = catalogue()
        .get(&CheckId::parse("tls.chain_trusted").unwrap())
        .unwrap();
    assert_eq!(r.check.title, "Chain trusted");
    assert_eq!(r.check.guide_href.as_deref(), Some(spec.guide.as_str()));
    assert!(r.links.is_empty());
    let html = render_section(&v).unwrap();
    assert!(html.contains("Chain trusted"));
    assert!(!html.contains("#ev-"), "no block link expected: {html}");
}
