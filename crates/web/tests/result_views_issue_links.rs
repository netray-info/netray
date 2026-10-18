//! Evidence links of issue rows (V2 spec v2-result-page, requirement 8; criteria C10 and C11).
use std::collections::BTreeMap;

use netray_engine::catalogue::catalogue;
use netray_model::evidence::{BlockId, EVIDENCE_SCHEMA_VERSION};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_web::pages::result::views::{Grouping, Links, evidence_href, issues};

fn check(evidence: &[&str]) -> RunCheck {
    RunCheck {
        id: CheckId::parse("tls.caa_compliant").unwrap(),
        status: Status::Warn,
        findings: vec!["issuer is not in the CAA set".into()],
        evidence: evidence
            .iter()
            .map(|b| BlockId::parse(b).unwrap())
            .collect(),
    }
}

fn run(check: RunCheck) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![check],
        blocks: vec![],
    }
}

fn row_href(run: &Run, links: Links<'_>) -> Option<String> {
    let groups = issues(
        run,
        catalogue(),
        links,
        Grouping::Protocol,
        &BTreeMap::new(),
    );
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].rows.len(), 1);
    groups[0].rows[0].view.evidence_href.clone()
}

#[test]
fn entry_links_point_at_the_referenced_block_or_the_protocol_section() {
    let with = check(&["dns.caa"]);
    let without = check(&[]);
    let entry = || Links::Entry("Ab3dE5fG");

    assert_eq!(evidence_href(&with, &entry()), "/r/Ab3dE5fG/dns#ev-dns.caa");
    assert_eq!(evidence_href(&without, &entry()), "/r/Ab3dE5fG/tls");
    assert_eq!(
        row_href(&run(with), entry()).as_deref(),
        Some("/r/Ab3dE5fG/dns#ev-dns.caa")
    );
    assert_eq!(
        row_href(&run(without), entry()).as_deref(),
        Some("/r/Ab3dE5fG/tls")
    );
}

#[test]
fn live_links_are_in_page_anchors() {
    let with = check(&["dns.caa"]);
    let without = check(&[]);

    assert_eq!(evidence_href(&with, &Links::Live), "#ev-dns.caa");
    assert_eq!(evidence_href(&without, &Links::Live), "#evidence-tls");
    assert_eq!(
        row_href(&run(with), Links::Live).as_deref(),
        Some("#ev-dns.caa")
    );
    assert_eq!(
        row_href(&run(without), Links::Live).as_deref(),
        Some("#evidence-tls")
    );
}
