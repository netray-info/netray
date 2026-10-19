//! Contract tests for the per-protocol evidence view's scope (spec v2-inspectors, requirement 3;
//! C9, C11).
use netray_engine::catalogue::catalogue;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_web::components::{RunLineView, Tone};
use netray_web::evidence::{render_section, section};

fn run_line() -> RunLineView {
    RunLineView {
        parts: vec!["incomplete, not recorded".into()],
    }
}

fn run(sections: Vec<RunSection>, checks: Vec<RunCheck>) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections,
        checks,
        blocks: vec![],
    }
}

fn ids_of(protocol: Protocol) -> Vec<CheckId> {
    catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == protocol)
        .map(|e| e.id.clone())
        .collect()
}

#[test]
fn not_applicable_section_shows_its_reason_before_untested_rows() {
    let reason = "no A or AAAA record";
    let checks = ids_of(Protocol::Tls)
        .into_iter()
        .map(|id| RunCheck {
            id,
            status: Status::NotApplicable,
            findings: vec![],
            evidence: vec![],
        })
        .collect();
    let run = run(
        vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::NotApplicable,
            reason: Some(reason.into()),
        }],
        checks,
    );

    let view = section(&run, Protocol::Tls, run_line());
    assert_eq!(view.reason.as_deref(), Some(reason));
    assert!(!view.rows.is_empty());
    assert!(view.rows.iter().all(|r| r.check.tone == Tone::Untested));

    let html = render_section(&view).unwrap();
    let at_reason = html.find(reason).expect("reason is rendered");
    let at_first_row = html
        .find(view.rows[0].check.title.as_str())
        .expect("first row is rendered");
    assert!(at_reason < at_first_row, "reason precedes the first row");
}

#[test]
fn rows_follow_catalogue_order_and_cover_every_check_of_all_five_protocols() {
    let protocols = [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ];
    for protocol in protocols {
        let expected = ids_of(protocol);
        assert!(!expected.is_empty(), "{protocol:?} has catalogue checks");

        // Reversed run order; every third check (by catalogue position) is omitted.
        let present: Vec<bool> = (0..expected.len()).map(|i| i % 3 != 1).collect();
        let checks: Vec<RunCheck> = expected
            .iter()
            .zip(&present)
            .rev()
            .filter(|(_, p)| **p)
            .map(|(id, _)| RunCheck {
                id: id.clone(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            })
            .collect();
        let run = run(
            vec![RunSection {
                protocol,
                outcome: SectionKind::Measured,
                reason: None,
            }],
            checks,
        );

        let view = section(&run, protocol, run_line());
        assert_eq!(view.rows.len(), expected.len(), "{protocol:?} row count");
        for (i, (row, id)) in view.rows.iter().zip(&expected).enumerate() {
            let spec = catalogue().get(id).unwrap();
            assert_eq!(row.check.code, spec.short, "{protocol:?} row {i} code");
            assert_eq!(row.check.title, spec.title, "{protocol:?} row {i} title");
            let tone = if present[i] {
                Tone::Pass
            } else {
                Tone::Untested
            };
            assert_eq!(row.check.tone, tone, "{protocol:?} row {i} tone");
        }
    }
}
