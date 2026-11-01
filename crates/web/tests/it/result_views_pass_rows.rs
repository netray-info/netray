//! Pass rows carry a body, a guide link and an evidence link (spec pustina-findings, C6 and C15).
use netray_engine::catalogue::catalogue;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{Grade, Protocol, Status};
use netray_web::pages::result::views::passed_checks;

fn all_pass_run() -> Run {
    let cat = catalogue();
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: [
            Protocol::Dns,
            Protocol::Tls,
            Protocol::Http,
            Protocol::Email,
            Protocol::Ip,
        ]
        .into_iter()
        .map(|protocol| RunSection {
            protocol,
            outcome: SectionKind::Measured,
            reason: None,
        })
        .collect(),
        checks: cat
            .entries()
            .iter()
            .map(|s| RunCheck {
                id: s.id.clone(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

#[test]
fn every_pass_row_has_impact_body_guide_and_evidence_link() {
    let cat = catalogue();
    let rows = passed_checks(&all_pass_run(), cat);
    assert_eq!(rows.len(), cat.entries().len());
    let raw: toml::Value = toml::from_str(include_str!("../../../engine/catalogue/checks.toml"))
        .expect("checks.toml parses");
    let tables = raw["check"].as_array().expect("check tables");
    for (row, spec) in rows.iter().zip(cat.entries()) {
        let checks = tables
            .iter()
            .find(|t| t.get("id").and_then(|v| v.as_str()) == Some(row.code.as_str()))
            .and_then(|t| t.get("checks"))
            .and_then(|v| v.as_str());
        let Some(checks) = checks.filter(|c| !c.is_empty()) else {
            panic!("{}: catalogue has no checks text", row.code);
        };
        let at_checks = row.body.find(checks);
        let at_impact = row.body.find(&spec.impact);
        assert!(
            at_checks.is_some() && at_checks < at_impact,
            "{}: body {:?} must hold the checks text {:?} before the impact sentence",
            row.code,
            row.body,
            checks
        );
        assert!(
            row.body.contains(&spec.impact),
            "{}: body {:?} lacks the impact sentence {:?}",
            row.code,
            row.body,
            spec.impact
        );
        let guide = row.guide_href.as_deref().unwrap_or("");
        assert!(
            guide.starts_with("/guide/"),
            "{}: guide link {:?}",
            row.code,
            row.guide_href
        );
        let ev = row.evidence_href.as_deref().unwrap_or("");
        assert!(
            ev.starts_with("#evidence-") || ev.starts_with("#ev-"),
            "{}: evidence link {:?}",
            row.code,
            row.evidence_href
        );
    }
}
