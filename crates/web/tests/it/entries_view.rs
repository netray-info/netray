//! Contract tests for the entry -> page view (spec v2-entries, requirement 6; C6, C17, C18).
use std::time::{Duration, SystemTime};

use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{Entry, HistoryStore};
use netray_web::components::Tone;
use netray_web::entries::render;
use netray_web::entries::view::{entry_view, format_utc_timestamp};
use netray_web::pages::result::views::line_code;

const CREATED_AT: i64 = 1_778_262_240;

fn id_at(i: usize) -> CheckId {
    catalogue().entries()[i].id.clone()
}

fn rc(id: CheckId, status: Status) -> RunCheck {
    RunCheck {
        id,
        status,
        findings: vec![],
        evidence: vec![],
    }
}

async fn stored(checks: Vec<RunCheck>) -> Entry {
    let run = Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks,
        blocks: vec![],
    };
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record("example.com", &run, &[], CREATED_AT, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    store.entries("example.com").await.unwrap().remove(0)
}

/// Mixed statuses, inserted in an order that is not the catalogue's.
async fn mixed_entry() -> Entry {
    stored(vec![
        rc(id_at(6), Status::Warn),
        rc(id_at(5), Status::Fail),
        rc(id_at(7), Status::Pass),
        rc(id_at(0), Status::Fail),
        rc(id_at(1), Status::Warn),
        rc(id_at(8), Status::NotApplicable),
        rc(id_at(3), Status::Fail),
        rc(id_at(9), Status::NotTested),
        rc(id_at(2), Status::Pass),
        rc(id_at(10), Status::Unmeasured),
        rc(id_at(4), Status::Warn),
    ])
    .await
}

#[test]
fn format_utc_timestamp_known_values() {
    let at = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
    assert_eq!(
        format_utc_timestamp(at(1_778_262_240)),
        "May 8, 2026 · 17:44 UTC"
    );
    assert_eq!(format_utc_timestamp(at(0)), "Jan 1, 1970 · 00:00 UTC");
}

#[tokio::test]
async fn entry_view_maps_run_line_counts_and_issue_rows() {
    let entry = mixed_entry().await;
    let v = entry_view(&entry);

    assert_eq!(v.domain, "example.com");
    assert_eq!(v.id, entry.id);
    assert_eq!(
        v.run_line.parts,
        vec![
            "recorded May 8, 2026 · 17:44 UTC".to_string(),
            "profile v3".to_string(),
            "engine 0.26.0".to_string(),
        ]
    );
    assert_eq!(v.counts.pass, 2);
    assert_eq!(v.counts.warn, 3);
    assert_eq!(v.counts.fail, 3);
    assert_eq!(v.counts.not_applicable, 1);
    assert_eq!(v.counts.not_tested, 1);
    assert_eq!(v.counts.unmeasured, 1);

    // fails (catalogue positions 0, 3, 5) then warns (1, 4, 6).
    let order = [0usize, 3, 5, 1, 4, 6];
    assert_eq!(v.issues.len(), order.len());
    for (k, (row, &i)) in v.issues.iter().zip(order.iter()).enumerate() {
        let spec = &catalogue().entries()[i];
        assert_eq!(row.code, spec.id.to_string());
        assert_eq!(row.title, spec.title);
        assert_eq!(row.body, spec.finding);
        assert_eq!(row.fix, Some(spec.fix_hint.clone()));
        assert_eq!(row.guide_href, Some(spec.guide.clone()));
        assert_eq!(row.evidence_href, None);
        assert_eq!(row.since, None);
        assert_eq!(row.tone, if k < 3 { Tone::Fail } else { Tone::Warn });
    }
}

#[tokio::test]
async fn fail_rows_precede_warn_rows_in_catalogue_order_and_pass_has_none() {
    let entry = mixed_entry().await;
    let v = entry_view(&entry);

    let want: Vec<String> = [0usize, 3, 5, 1, 4, 6]
        .iter()
        .map(|&i| id_at(i).to_string())
        .collect();
    let got: Vec<String> = v.issues.iter().map(|r| r.code.clone()).collect();
    assert_eq!(got, want);
    for pass in [id_at(2), id_at(7)] {
        assert!(!got.contains(&pass.to_string()));
    }

    let html = render::entry(&v).unwrap();
    let mut last = 0;
    for code in &want {
        let line = line_code(&CheckId::parse(code).unwrap()).unwrap();
        let needle = format!("<span class=\"check-row__code\">{line}</span>");
        let at = html
            .find(&needle)
            .unwrap_or_else(|| panic!("no row for {code}"));
        assert!(at > last, "row {code} out of order");
        last = at;
    }
    for pass in [id_at(2), id_at(7)] {
        assert!(!html.contains(&format!(">{pass}</span>")));
    }
    assert_eq!(html.matches("class=\"check-row\"").count(), 6);
}

#[tokio::test]
async fn unknown_check_id_shows_id_as_title_without_body_fix_or_guide() {
    let unknown = "dns.not_in_catalogue_zz";
    let entry = stored(vec![
        rc(CheckId::parse(unknown).unwrap(), Status::Warn),
        rc(id_at(0), Status::Warn),
    ])
    .await;
    let v = entry_view(&entry);

    let row = v.issues.iter().find(|r| r.code == unknown).expect("row");
    assert_eq!(row.title, unknown);
    assert_eq!(row.body, "");
    assert_eq!(row.fix, None);
    assert_eq!(row.guide_href, None);

    let html = render::entry(&v).unwrap();
    let article = html
        .split("<article class=\"check-row\"")
        .find(|a| a.contains(&format!(">{unknown}</span>")))
        .expect("rendered row");
    let article = article.split("</article>").next().unwrap();
    assert!(article.contains(&format!("<h3 class=\"check-row__title\">{unknown}</h3>")));
    assert!(!article.contains("check-row__fix"));
    assert!(!article.contains("Guide"));
}
