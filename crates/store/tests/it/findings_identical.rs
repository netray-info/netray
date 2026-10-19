//! Findings and evidence references are no part of a run's identity (specs/features/v2-api/spec.md,
//! requirement 4, criterion C35).
use netray_model::diff::diff;
use netray_model::evidence::{BlockId, EVIDENCE_SCHEMA_VERSION};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind, fingerprint};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};

const DOMAIN: &str = "example.com";

fn run(status: Status, findings: &[&str], evidence: &[&str]) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.caa_compliant").unwrap(),
            status,
            findings: findings.iter().map(|s| s.to_string()).collect(),
            evidence: evidence
                .iter()
                .map(|s| BlockId::parse(s).unwrap())
                .collect(),
        }],
        blocks: vec![],
    }
}

#[tokio::test]
async fn runs_differing_only_in_findings_and_evidence_are_identical() {
    let bare = run(Status::Warn, &[], &[]);
    let rich = run(Status::Warn, &["CAA does not match"], &["dns.caa"]);

    assert_eq!(fingerprint(&bare, &[]), fingerprint(&rich, &[]));
    let delta = diff(&bare, &rich, &[], None);
    assert!(delta.checks.is_empty(), "{:?}", delta.checks);
    assert!(delta.evidence.is_empty(), "{:?}", delta.evidence);

    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let d = |p: &Run, c: &Run| diff(p, c, &[], None);
    let Recorded::First { id } = store.record(DOMAIN, &bare, &[], 1000, d).await.unwrap() else {
        panic!("expected First");
    };
    let rec = store.record(DOMAIN, &rich, &[], 2000, d).await.unwrap();
    let Recorded::Identical { id: same } = rec else {
        panic!("expected Identical, got {rec:?}");
    };
    assert_eq!(same, id);
}

#[test]
fn runs_differing_in_a_check_status_have_different_fingerprints() {
    let warn = run(Status::Warn, &["m"], &["dns.caa"]);
    let fail = run(Status::Fail, &["m"], &["dns.caa"]);
    assert_ne!(fingerprint(&warn, &[]), fingerprint(&fail, &[]));
}
