//! Contract tests for the result page's diff view (spec v2-result-page, requirements 11 and 19).
use netray_engine::catalogue::catalogue;
use netray_model::diff::{
    BlockChange, BlockChangeKind, ChangeClass, CheckChange, Diff, GradeChange,
};
use netray_model::evidence::BlockId;
use netray_model::{CheckId, Grade, Status};
use netray_web::pages::result::views::{FIRST_STATE, GradeStep, diff_view};

fn id(s: &str) -> CheckId {
    CheckId::parse(s).unwrap()
}

fn make_diff(checks: Vec<CheckChange>, grade: GradeChange) -> Diff {
    Diff {
        checks,
        evidence: vec![],
        grade,
        engine_changed: false,
    }
}

fn grade(from: Grade, rescored: Option<Grade>, to: Grade) -> GradeChange {
    GradeChange { from, rescored, to }
}

fn step(grade: &str, note: Option<&str>) -> GradeStep {
    GradeStep {
        grade: grade.into(),
        note: note.map(str::to_string),
    }
}

#[test]
fn every_change_class_yields_one_row_with_its_label() {
    use ChangeClass::*;
    use Status::*;
    let rows: Vec<(ChangeClass, Option<Status>, Option<Status>, &str)> = vec![
        (Fixed, Some(Fail), Some(Pass), "fixed"),
        (Improved, Some(Fail), Some(Warn), "improved"),
        (New, Some(Pass), Some(Fail), "new"),
        (Worse, Some(Warn), Some(Fail), "worse"),
        (
            NowApplicable,
            Some(NotApplicable),
            Some(Pass),
            "now applicable",
        ),
        (
            NoLongerApplicable,
            Some(Pass),
            Some(NotApplicable),
            "no longer applicable",
        ),
        (
            IntoNotTested,
            Some(Pass),
            Some(NotTested),
            "into not tested",
        ),
        (
            OutOfNotTested,
            Some(NotTested),
            Some(Warn),
            "out of not tested",
        ),
        (
            IntoNotMeasured,
            Some(Pass),
            Some(Unmeasured),
            "into not measured",
        ),
        (
            OutOfNotMeasured,
            Some(Unmeasured),
            Some(Fail),
            "out of not measured",
        ),
        (Added, None, Some(Pass), "added"),
        (Removed, Some(Warn), None, "removed"),
        (AddedByRelease, None, Some(Warn), "added by release"),
        (RemovedByRelease, Some(Fail), None, "removed by release"),
    ];
    for (class, from, to, label) in rows {
        let d = make_diff(
            vec![CheckChange {
                id: id("tls.not_expired"),
                class: class.clone(),
                from,
                to,
            }],
            grade(Grade::B, None, Grade::B),
        );
        let v = diff_view(Some(&d), catalogue());
        assert_eq!(v.block.entries.len(), 1, "{class:?}");
        assert_eq!(v.block.entries[0].kind, label, "{class:?}");
        assert_eq!(v.block.entries[0].text, "Certificate valid", "{class:?}");
    }
}

#[test]
fn diff_view_rows_checks_then_blocks_grade_and_engine_note() {
    let mut d = make_diff(
        vec![
            CheckChange {
                id: id("tls.not_expired"),
                class: ChangeClass::Fixed,
                from: Some(Status::Fail),
                to: Some(Status::Pass),
            },
            CheckChange {
                id: id("dns.dnssec"),
                class: ChangeClass::Worse,
                from: Some(Status::Pass),
                to: Some(Status::Fail),
            },
        ],
        grade(Grade::B, None, Grade::A),
    );
    d.evidence = vec![
        BlockChange {
            block: BlockId::parse("tls.chain").unwrap(),
            kind: BlockChangeKind::Added,
        },
        BlockChange {
            block: BlockId::parse("http.headers").unwrap(),
            kind: BlockChangeKind::Removed,
        },
        BlockChange {
            block: BlockId::parse("dns.records@443").unwrap(),
            kind: BlockChangeKind::Changed {
                items: vec!["~ a".into(), "+ b".into(), "- c".into()],
            },
        },
    ];
    let v = diff_view(Some(&d), catalogue());
    let e = &v.block.entries;
    assert_eq!(e.len(), 5);
    assert_eq!(
        (e[0].kind.as_str(), e[0].text.as_str()),
        ("fixed", "Certificate valid")
    );
    assert_eq!(
        (e[1].kind.as_str(), e[1].text.as_str()),
        ("worse", "DNSSEC signed")
    );
    assert_eq!(e[2].kind, "added");
    assert!(e[2].text.contains("tls.chain"), "{}", e[2].text);
    assert_eq!(e[3].kind, "removed");
    assert!(e[3].text.contains("http.headers"), "{}", e[3].text);
    assert_eq!(e[4].kind, "changed");
    assert!(e[4].text.contains("dns.records@443"), "{}", e[4].text);
    assert!(e[4].text.contains("3 items"), "{}", e[4].text);
    assert_eq!(v.grade, vec![step("B", None), step("A", None)]);
    assert_eq!(v.block.summary, "B \u{2192} A");
    assert_eq!(v.block.note, None);

    d.engine_changed = true;
    let note = diff_view(Some(&d), catalogue()).block.note;
    assert!(note.expect("engine note").contains("engine"));
}

#[test]
fn rescored_grade_has_two_steps_and_none_has_one() {
    let with = diff_view(
        Some(&make_diff(
            vec![],
            grade(Grade::C, Some(Grade::D), Grade::B),
        )),
        catalogue(),
    );
    assert_eq!(
        with.grade,
        vec![
            step("C", None),
            step("D", Some("profile effect")),
            step("B", Some("domain effect")),
        ]
    );
    let without = diff_view(
        Some(&make_diff(vec![], grade(Grade::C, None, Grade::B))),
        catalogue(),
    );
    assert_eq!(without.grade, vec![step("C", None), step("B", None)]);
}

#[test]
fn entry_without_a_diff_notes_the_first_state() {
    assert_eq!(
        FIRST_STATE,
        "first state recorded \u{2014} re-check after your change"
    );
    let v = diff_view(None, catalogue());
    assert!(v.block.entries.is_empty());
    assert!(v.grade.is_empty());
    assert_eq!(
        v.block.note.as_deref(),
        Some("first state recorded \u{2014} re-check after your change")
    );
}

#[test]
fn changed_block_counts_one_item_in_the_singular_and_omits_an_empty_count() {
    let block = |items: Vec<String>| BlockChange {
        block: BlockId::parse("email.mta_sts_policy").unwrap(),
        kind: BlockChangeKind::Changed { items },
    };
    let mut diff = make_diff(
        vec![],
        GradeChange {
            from: Grade::A,
            rescored: None,
            to: Grade::A,
        },
    );
    diff.evidence = vec![block(vec!["~ a".into()]), block(vec![])];
    let view = diff_view(Some(&diff), catalogue());
    let e = &view.block.entries;
    assert_eq!(e[0].text, "email.mta_sts_policy (1 item)");
    assert_eq!(e[1].text, "email.mta_sts_policy");
}
