//! V2 spec v2-entries, Phase 1, requirements 3 and 6 (C3, C8, C9, C10): the entry page renders
//! its parts in order, maps the grade to a tone, omits an absent diff block and escapes values.
use netray_model::Grade;
use netray_model::diff::Diff;
use netray_model::run::Run;
use netray_store::Entry;
use netray_web::components::{
    CheckRowView, DiffBlockView, DiffEntry, GradeView, RunLineView, Tone,
};
use netray_web::entries::render;
use netray_web::entries::view::{EntryPageView, StatusCounts, entry_view};

const COUNTS: &str = "1 pass · 0 warn · 2 fail · 0 not applicable · 0 not tested · 0 not measured";

fn row(code: &str, title: &str, tone: Tone) -> CheckRowView {
    CheckRowView {
        code: code.into(),
        tone,
        title: title.into(),
        since: None,
        body: "body".into(),
        fix: None,
        evidence_href: None,
        guide_href: None,
    }
}

fn diff_block() -> DiffBlockView {
    DiffBlockView {
        title: "Changes since the previous entry".into(),
        summary: "C → B".into(),
        entries: vec![DiffEntry {
            kind: "fixed".into(),
            tone: Tone::Fix,
            text: "Certificate not expired".into(),
            protocol: "tls".into(),
        }],
        note: None,
    }
}

fn view(domain: &str, title: &str, diff: Option<DiffBlockView>) -> EntryPageView {
    EntryPageView {
        id: "Ab3dE6gH".into(),
        domain: domain.into(),
        grade: GradeView {
            letter: "B".into(),
            score: None,
            tone: Tone::Pass,
        },
        run_line: RunLineView {
            parts: vec![
                "recorded May 8, 2026 · 17:44 UTC".into(),
                "profile v3".into(),
                "engine 0.26.0".into(),
            ],
        },
        diff,
        issues: vec![
            row("tls.not_expired", title, Tone::Fail),
            row("dns.dnssec_valid", "DNSSEC valid", Tone::Warn),
        ],
        counts: StatusCounts {
            pass: 1,
            warn: 0,
            fail: 2,
            not_applicable: 0,
            not_tested: 0,
            unmeasured: 0,
        },
    }
}

/// Positions of the needles in `html`, asserting each is present and they ascend.
fn assert_in_order(html: &str, needles: &[&str]) {
    let mut at = 0;
    for n in needles {
        let i = html[at..]
            .find(n)
            .unwrap_or_else(|| panic!("`{n}` missing or out of order after byte {at}"));
        at += i + n.len();
    }
}

#[test]
fn entry_page_holds_its_parts_in_order() {
    let html = render::entry(&view(
        "a.example.com",
        "Certificate not expired",
        Some(diff_block()),
    ))
    .expect("render");
    assert_in_order(
        &html,
        &[
            "<h1",
            "a.example.com</h1>",
            r#"<div class="grade" data-tone="pass">"#,
            r#"<p class="run-line">"#,
            "recorded May 8, 2026 · 17:44 UTC",
            "profile v3",
            "engine 0.26.0",
            r#"<section class="diff-block">"#,
            r#"<article class="check-row" data-tone="fail">"#,
            r#"<article class="check-row" data-tone="warn">"#,
            COUNTS,
        ],
    );
    assert!(!html.contains(r#"class="grade__score""#), "no score");
    assert_eq!(html.matches(r#"<article class="check-row""#).count(), 2);
    assert!(html.contains("<title>a.example.com · B</title>"));
    assert!(html.contains(r#"property="og:title" content="a.example.com · B""#));
    assert!(html.contains(r#"property="og:image" content="/r/Ab3dE6gH/og.png""#));
    assert!(html.contains(r#"property="og:url" content="/r/Ab3dE6gH""#));
}

#[test]
fn entry_page_without_a_diff_has_no_diff_block() {
    let html =
        render::entry(&view("a.example.com", "Certificate not expired", None)).expect("render");
    assert!(!html.contains(r#"class="diff-block"#), "{html}");
}

#[test]
fn domain_and_check_title_are_escaped() {
    let html = render::entry(&view(
        "<script>alert(1)</script>.example.com",
        "<script>alert(2)</script>",
        None,
    ))
    .expect("render");
    assert!(!html.contains("<script"), "page holds a script element");
    assert!(
        html.contains("&lt;script&gt;alert(1)") || html.contains("&#60;script&#62;alert(1)"),
        "domain not escaped"
    );
    assert!(
        html.contains("&lt;script&gt;alert(2)") || html.contains("&#60;script&#62;alert(2)"),
        "title not escaped"
    );
}

fn entry(grade: Grade) -> Entry {
    Entry {
        id: "Ab3dE6gH".into(),
        domain: "a.example.com".into(),
        created_at: 1_778_262_240,
        last_seen_at: 1_778_262_240,
        predecessor_id: None,
        profile_version: 3,
        grade,
        fingerprint: "f".into(),
        run: Run {
            profile_version: 3,
            engine_version: "0.26.0".into(),
            evidence_schema_version: 1,
            grade,
            complete: true,
            sections: vec![],
            checks: vec![],
            blocks: vec![],
        },
        diff: None::<Diff>,
        engine_version: "0.26.0".into(),
    }
}

#[test]
fn grade_tone_per_letter() {
    let table = [
        (Grade::APlus, "A+", "pass"),
        (Grade::A, "A", "pass"),
        (Grade::B, "B", "pass"),
        (Grade::C, "C", "warn"),
        (Grade::D, "D", "warn"),
        (Grade::F, "F", "fail"),
    ];
    for (grade, letter, tone) in table {
        let v = entry_view(&entry(grade));
        assert_eq!(v.grade.letter, letter);
        assert_eq!(v.grade.score, None);
        assert_eq!(v.grade.tone.as_str(), tone, "view tone for {letter}");
        let html = render::entry(&v).expect("render");
        assert!(
            html.contains(&format!(r#"<div class="grade" data-tone="{tone}">"#)),
            "rendered tone for {letter}"
        );
        assert!(html.contains(&format!("<title>a.example.com · {letter}</title>")));
    }
}
