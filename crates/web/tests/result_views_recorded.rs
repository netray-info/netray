//! Contract tests for the diff block after a fresh run (spec v2-result-page, requirements 12, 16).
use netray_engine::catalogue::catalogue;
use netray_model::diff::{ChangeClass, CheckChange, Diff, GradeChange};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{Entry, Recorded};
use netray_web::pages::result::views::recorded_diff_view;

// 2025-10-09T08:53:20Z
const CREATED_AT: i64 = 1_760_000_000;

fn diff_with(class: ChangeClass) -> Diff {
    Diff {
        checks: vec![CheckChange {
            id: CheckId::parse("tls.not_expired").unwrap(),
            class,
            from: Some(Status::Fail),
            to: Some(Status::Pass),
        }],
        evidence: vec![],
        grade: GradeChange {
            from: Grade::C,
            rescored: None,
            to: Grade::B,
        },
        engine_changed: false,
    }
}

fn entry(diff: Option<Diff>) -> Entry {
    Entry {
        id: "AbCd1234".into(),
        domain: "example.com".into(),
        created_at: CREATED_AT,
        last_seen_at: CREATED_AT + 86_400 * 3,
        predecessor_id: None,
        profile_version: 3,
        grade: Grade::B,
        fingerprint: "fp".into(),
        run: Run {
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
            checks: vec![RunCheck {
                id: CheckId::parse("tls.not_expired").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            }],
            blocks: vec![],
        },
        diff,
        engine_version: "0.26.0".into(),
    }
}

fn kinds(recorded: &Recorded, latest: Option<&Entry>) -> (Vec<String>, Option<String>) {
    let view = recorded_diff_view(recorded, latest, catalogue());
    (
        view.block.entries.iter().map(|e| e.kind.clone()).collect(),
        view.block.note.clone(),
    )
}

#[test]
fn recorded_diff_view_by_outcome() {
    let latest = entry(Some(diff_with(ChangeClass::Worse)));
    let rows: Vec<(&str, Recorded, Option<&Entry>, Vec<&str>, Option<&str>)> = vec![
        (
            "changed",
            Recorded::Changed {
                id: "X".into(),
                diff: diff_with(ChangeClass::Fixed),
            },
            Some(&latest),
            vec!["fixed"],
            None,
        ),
        (
            "identical",
            Recorded::Identical { id: "X".into() },
            Some(&latest),
            vec!["worse"],
            Some("unchanged since 2025-10-09"),
        ),
        (
            "first",
            Recorded::First { id: "X".into() },
            None,
            vec![],
            Some("first state recorded \u{2014} re-check after your change"),
        ),
        (
            "not stored with diff",
            Recorded::NotStored {
                diff: Some(diff_with(ChangeClass::New)),
            },
            Some(&latest),
            vec!["new"],
            Some("incomplete, not recorded"),
        ),
        (
            "not stored without diff",
            Recorded::NotStored { diff: None },
            None,
            vec![],
            Some("not recorded: this run is incomplete"),
        ),
    ];
    for (name, recorded, latest, want_kinds, want_note) in rows {
        let (got_kinds, got_note) = kinds(&recorded, latest);
        assert_eq!(got_kinds, want_kinds, "{name}: rows");
        assert_eq!(got_note.as_deref(), want_note, "{name}: note");
    }
}

#[test]
fn identical_keeps_the_latest_entrys_own_notes() {
    let cat = catalogue();
    let mut engine = diff_with(ChangeClass::Worse);
    engine.engine_changed = true;
    let latest = entry(Some(engine));
    let view = recorded_diff_view(&Recorded::Identical { id: "X".into() }, Some(&latest), cat);
    assert_eq!(
        view.block.note.as_deref(),
        Some("unchanged since 2025-10-09; engine changed between these runs")
    );
    let first = entry(None);
    let view = recorded_diff_view(&Recorded::Identical { id: "X".into() }, Some(&first), cat);
    assert_eq!(
        view.block.note.as_deref(),
        Some(
            "unchanged since 2025-10-09; first state recorded \u{2014} re-check after your change"
        )
    );
}
