//! Contract tests for the retention sweep (spec v2-history, Phase 3, requirement 11; C4, C13-C16).
use netray_model::diff::diff;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, RETENTION_SECS};

const DAY: i64 = 86_400;
const T: i64 = 1_800_000_000;

/// A complete run whose single check carries `status`; distinct statuses give distinct runs.
fn run(status: Status) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: netray_model::evidence::EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

const STATUSES: [Status; 4] = [Status::Pass, Status::Warn, Status::Fail, Status::NotTested];

async fn open() -> (HistoryStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (store, dir)
}

/// Records one distinct run per element of `ages_days` (days before T, oldest first).
async fn chain(store: &HistoryStore, domain: &str, ages_days: &[i64]) {
    for (i, age) in ages_days.iter().enumerate() {
        store
            .record(domain, &run(STATUSES[i]), &[], T - age * DAY, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn sweep_deletes_a_domain_whose_latest_entry_is_older_than_ninety_days() {
    assert_eq!(RETENTION_SECS, 90 * DAY);
    let (store, _dir) = open().await;
    chain(&store, "example.com", &[95, 91]).await;
    chain(&store, "example.org", &[1]).await;

    let deleted = store.sweep(T).await.unwrap();

    assert_eq!(deleted, 2);
    assert!(store.entries("example.com").await.unwrap().is_empty());
    assert_eq!(store.entries("example.org").await.unwrap().len(), 1);
}

#[tokio::test]
async fn sweep_deletes_old_entries_below_a_fresh_latest_entry() {
    let (store, _dir) = open().await;
    chain(&store, "example.com", &[95, 92, 1]).await;
    let before = store.entries("example.com").await.unwrap();
    let (e3, e2) = (before[0].clone(), before[1].clone());

    let deleted = store.sweep(T).await.unwrap();

    assert_eq!(deleted, 1);
    let after = store.entries("example.com").await.unwrap();
    let ids: Vec<&str> = after.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, vec![e3.id.as_str(), e2.id.as_str()]);
}

#[tokio::test]
async fn sweep_keeps_the_predecessor_of_the_latest_entry_and_its_diff() {
    let (store, _dir) = open().await;
    chain(&store, "example.com", &[95, 93, 92, 1]).await;
    let before = store.entries("example.com").await.unwrap();
    let (e4, e3) = (before[0].clone(), before[1].clone());
    assert!(e3.diff.is_some());

    let deleted = store.sweep(T).await.unwrap();

    assert_eq!(deleted, 2);
    let after = store.entries("example.com").await.unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].id, e4.id);
    assert_eq!(after[0].predecessor_id.as_deref(), Some(e3.id.as_str()));
    assert_eq!(after[1].id, e3.id);
    assert_eq!(after[1].predecessor_id, None);
    assert_eq!(after[1].diff, e3.diff);
    assert!(after[1].diff.is_some());
}

#[tokio::test]
async fn sweep_keeps_an_entry_last_seen_eighty_nine_days_ago() {
    let (store, _dir) = open().await;
    chain(&store, "example.com", &[89, 60, 1]).await;

    let deleted = store.sweep(T).await.unwrap();

    assert_eq!(deleted, 0);
    assert_eq!(store.entries("example.com").await.unwrap().len(), 3);
}
