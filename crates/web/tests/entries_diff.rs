//! Contract tests for the diff block of an entry page (spec v2-entries, requirement 7).
use netray_model::diff::{
    BlockChange, BlockChangeKind, ChangeClass, CheckChange, Diff, GradeChange, diff,
};
use netray_model::evidence::{BlockId, EVIDENCE_SCHEMA_VERSION};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{Entry, HistoryStore, Recorded};
use netray_web::components::Tone;
use netray_web::entries::render;
use netray_web::entries::view::{diff_view, entry_view};

const DOMAIN: &str = "example.com";
const NOTE: &str = "The previous entry was recorded by another engine version.";

fn id(s: &str) -> CheckId {
    CheckId::parse(s).unwrap()
}

fn grade_change(from: Grade, rescored: Option<Grade>, to: Grade) -> GradeChange {
    GradeChange { from, rescored, to }
}

fn make_diff(checks: Vec<CheckChange>, grade: GradeChange) -> Diff {
    Diff {
        checks,
        evidence: vec![],
        grade,
        engine_changed: false,
    }
}

#[test]
fn diff_view_names_every_class_with_its_tone() {
    use ChangeClass::*;
    use Status::*;
    // (class, from, to, kind word, tone)
    let rows: Vec<(ChangeClass, Option<Status>, Option<Status>, &str, Tone)> = vec![
        (Fixed, Some(Fail), Some(Pass), "fixed", Tone::Fix),
        (Improved, Some(Fail), Some(Warn), "improved", Tone::Warn),
        (New, Some(Pass), Some(Fail), "new", Tone::Fail),
        (Worse, Some(Warn), Some(Fail), "worse", Tone::Fail),
        (
            NowApplicable,
            Some(NotApplicable),
            Some(Pass),
            "now applicable",
            Tone::Pass,
        ),
        (
            NoLongerApplicable,
            Some(Pass),
            Some(NotApplicable),
            "no longer applicable",
            Tone::Untested,
        ),
        (
            IntoNotTested,
            Some(Pass),
            Some(NotTested),
            "into not tested",
            Tone::Untested,
        ),
        (
            OutOfNotTested,
            Some(NotTested),
            Some(Warn),
            "out of not tested",
            Tone::Warn,
        ),
        (
            IntoNotMeasured,
            Some(Pass),
            Some(Unmeasured),
            "into not measured",
            Tone::Untested,
        ),
        (
            OutOfNotMeasured,
            Some(Unmeasured),
            Some(Fail),
            "out of not measured",
            Tone::Fail,
        ),
        (Added, None, Some(Pass), "added", Tone::Pass),
        (Removed, Some(Warn), None, "removed", Tone::Untested),
        (
            AddedByRelease,
            None,
            Some(Warn),
            "added by release",
            Tone::Warn,
        ),
        (
            RemovedByRelease,
            Some(Fail),
            None,
            "removed by release",
            Tone::Untested,
        ),
    ];
    for (class, from, to, word, tone) in rows {
        let d = make_diff(
            vec![CheckChange {
                id: id("tls.not_expired"),
                class: class.clone(),
                from,
                to,
            }],
            grade_change(Grade::B, None, Grade::B),
        );
        let v = diff_view(&d);
        assert_eq!(v.entries.len(), 1, "{class:?}");
        let e = &v.entries[0];
        assert_eq!(e.kind, word, "{class:?}");
        assert_eq!(e.tone, tone, "{class:?}");
        assert_eq!(e.text, "Certificate valid", "{class:?}");
        assert_eq!(e.protocol, "tls", "{class:?}");
    }
}

#[test]
fn diff_view_title_text_fallback_and_order() {
    let d = make_diff(
        vec![
            CheckChange {
                id: id("dns.zz_not_in_catalogue"),
                class: ChangeClass::Worse,
                from: Some(Status::Pass),
                to: Some(Status::Fail),
            },
            CheckChange {
                id: id("tls.not_expired"),
                class: ChangeClass::Fixed,
                from: Some(Status::Fail),
                to: Some(Status::Pass),
            },
        ],
        grade_change(Grade::B, None, Grade::B),
    );
    let v = diff_view(&d);
    assert_eq!(v.title, "Changes since the previous entry");
    assert_eq!(v.entries[0].text, "dns.zz_not_in_catalogue");
    assert_eq!(v.entries[0].protocol, "dns");
    assert_eq!(v.entries[1].text, "Certificate valid");
    assert_eq!(v.note, None);
}

#[test]
fn diff_view_evidence_items_follow_the_checks() {
    let mut d = make_diff(
        vec![CheckChange {
            id: id("tls.not_expired"),
            class: ChangeClass::Fixed,
            from: Some(Status::Fail),
            to: Some(Status::Pass),
        }],
        grade_change(Grade::C, None, Grade::B),
    );
    d.evidence = vec![
        BlockChange {
            block: BlockId::parse("tls.chain").unwrap(),
            kind: BlockChangeKind::Added,
        },
        BlockChange {
            block: BlockId::parse("dns.records@443").unwrap(),
            kind: BlockChangeKind::Removed,
        },
        BlockChange {
            block: BlockId::parse("http.headers").unwrap(),
            kind: BlockChangeKind::Changed {
                items: vec!["~ a".into(), "+ b".into(), "- c".into()],
            },
        },
        BlockChange {
            block: BlockId::parse("email.mta_sts_policy").unwrap(),
            kind: BlockChangeKind::Changed { items: vec![] },
        },
    ];
    let v = diff_view(&d);
    assert_eq!(v.entries.len(), 5);
    let got: Vec<(&str, &str, Tone)> = v.entries[1..]
        .iter()
        .map(|e| (e.kind.as_str(), e.text.as_str(), e.tone))
        .collect();
    assert_eq!(
        got,
        vec![
            ("evidence", "tls.chain added", Tone::Untested),
            ("evidence", "dns.records@443 removed", Tone::Untested),
            ("evidence", "http.headers: 3 changed", Tone::Untested),
            ("evidence", "email.mta_sts_policy changed", Tone::Untested),
        ]
    );
}

#[test]
fn diff_view_summary_and_engine_note() {
    use Grade::*;
    let letters = [
        (APlus, "A+"),
        (A, "A"),
        (B, "B"),
        (C, "C"),
        (D, "D"),
        (F, "F"),
    ];
    for (from, f) in letters {
        for (to, t) in letters {
            let v = diff_view(&make_diff(vec![], grade_change(from, None, to)));
            let want = if f == t {
                format!("Grade unchanged ({t})")
            } else {
                format!("{f} \u{2192} {t}")
            };
            assert_eq!(v.summary, want);
            let v = diff_view(&make_diff(vec![], grade_change(from, Some(F), to)));
            assert_eq!(v.summary, format!("{f} \u{2192} F (profile) \u{2192} {t}"));
        }
    }
    let mut d = make_diff(vec![], grade_change(B, None, B));
    assert_eq!(diff_view(&d).note, None);
    d.engine_changed = true;
    assert_eq!(diff_view(&d).note.as_deref(), Some(NOTE));
}

fn run(status: Status, grade: Grade) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: id("tls.not_expired"),
            status,
        }],
        blocks: vec![],
    }
}

fn entry_with(diff: Option<Diff>, predecessor_id: Option<String>) -> Entry {
    let r = run(Status::Pass, Grade::B);
    Entry {
        id: "abcdef123456".into(),
        domain: DOMAIN.into(),
        created_at: 1_000,
        last_seen_at: 1_000,
        predecessor_id,
        profile_version: r.profile_version,
        grade: r.grade,
        fingerprint: "fp".into(),
        engine_version: r.engine_version.clone(),
        run: r,
        diff,
    }
}

fn diff_block(html: &str) -> &str {
    let start = html
        .find(r#"class="diff-block""#)
        .expect("diff block present");
    &html[start..]
}

#[tokio::test]
async fn stored_diff_renders_fixed_item_and_grade_summary() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let d = |p: &Run, c: &Run| diff(p, c, &[], None);
    store
        .record(DOMAIN, &run(Status::Fail, Grade::C), &[], 1000, d)
        .await
        .unwrap();
    let rec = store
        .record(DOMAIN, &run(Status::Pass, Grade::B), &[], 2000, d)
        .await
        .unwrap();
    assert!(matches!(rec, Recorded::Changed { .. }), "{rec:?}");
    let entry = store.latest(DOMAIN).await.unwrap().unwrap();

    let view = entry_view(&entry);
    let dv = view.diff.as_ref().expect("diff block view");
    assert_eq!(dv.summary, "C \u{2192} B");
    let fixed: Vec<_> = dv.entries.iter().filter(|e| e.kind == "fixed").collect();
    assert_eq!(fixed.len(), 1);
    assert_eq!(fixed[0].tone, Tone::Fix);
    assert_eq!(fixed[0].text, "Certificate valid");

    let html = render::entry(&view).unwrap();
    let block = diff_block(&html);
    assert!(block.contains("C \u{2192} B"));
    assert!(block.contains("data-tone=\"fix\""));
    assert!(block.contains(">fixed<"));
}

#[test]
fn rescored_and_engine_change_render_only_when_set() {
    let render_with = |rescored: Option<Grade>, engine_changed: bool| {
        let mut d = make_diff(vec![], grade_change(Grade::C, rescored, Grade::B));
        d.engine_changed = engine_changed;
        render::entry(&entry_view(&entry_with(Some(d), Some("p".into())))).unwrap()
    };
    let both = render_with(Some(Grade::D), true);
    assert!(both.contains("C \u{2192} D (profile) \u{2192} B"));
    assert!(both.contains(NOTE));
    let neither = render_with(None, false);
    assert!(neither.contains("C \u{2192} B"));
    assert!(!neither.contains("(profile)"));
    assert!(!neither.contains(NOTE));
}

#[test]
fn diff_block_is_present_exactly_when_the_entry_has_a_diff() {
    let first = render::entry(&entry_view(&entry_with(None, None))).unwrap();
    assert!(!first.contains(r#"class="diff-block""#));
    assert!(entry_view(&entry_with(None, None)).diff.is_none());

    let swept = entry_with(
        Some(make_diff(vec![], grade_change(Grade::B, None, Grade::B))),
        None,
    );
    let html = render::entry(&entry_view(&swept)).unwrap();
    assert!(html.contains(r#"class="diff-block""#));
    assert!(html.contains("Grade unchanged (B)"));
    assert!(html.contains("Changes since the previous entry"));
}
