//! Contract tests for the stored check duration (spec v2-result-fidelity, Phase 1,
//! requirements 9 and 21).
use netray_model::diff::{Diff, diff};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

const DOMAIN: &str = "example.com";

fn run(status: Status) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
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

async fn fresh() -> (HistoryStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (store, dir)
}

fn d(p: &Run, c: &Run) -> Diff {
    diff(p, c, &[], None)
}

#[tokio::test]
async fn record_timed_stores_the_duration_on_new_entries_only() {
    let (store, _dir) = fresh().await;
    let first = store
        .record_timed(DOMAIN, &run(Status::Pass), &[], 1000, Some(4210), d)
        .await
        .unwrap();
    assert!(matches!(first, Recorded::First { .. }), "{first:?}");
    let entries = store.entries(DOMAIN).await.unwrap();
    assert_eq!(entries[0].duration_ms, Some(4210));

    let changed = store
        .record_timed(DOMAIN, &run(Status::Fail), &[], 2000, Some(900), d)
        .await
        .unwrap();
    assert!(matches!(changed, Recorded::Changed { .. }), "{changed:?}");
    let entries = store.entries(DOMAIN).await.unwrap();
    assert_eq!(entries.len(), 2);
    let head = entries.iter().find(|e| e.predecessor_id.is_some()).unwrap();
    let pred = entries.iter().find(|e| e.predecessor_id.is_none()).unwrap();
    assert_eq!(head.duration_ms, Some(900));
    assert_eq!(pred.duration_ms, Some(4210));
}

#[tokio::test]
async fn record_without_a_duration_stores_none() {
    let (store, _dir) = fresh().await;
    store
        .record(DOMAIN, &run(Status::Pass), &[], 1000, d)
        .await
        .unwrap();
    assert_eq!(store.entries(DOMAIN).await.unwrap()[0].duration_ms, None);
}

#[tokio::test]
async fn the_duration_does_not_change_the_fingerprint_or_the_run() {
    let (store, _dir) = fresh().await;
    let r = run(Status::Pass);
    store
        .record_timed("timed.example.com", &r, &[], 1000, Some(4210), d)
        .await
        .unwrap();
    store
        .record("plain.example.com", &r, &[], 1000, d)
        .await
        .unwrap();
    let timed = &store.entries("timed.example.com").await.unwrap()[0];
    let plain = &store.entries("plain.example.com").await.unwrap()[0];
    assert_eq!(timed.fingerprint, plain.fingerprint);
    assert_eq!(timed.run, plain.run);
}

#[tokio::test]
async fn identical_run_leaves_the_existing_duration_unchanged() {
    let (store, _dir) = fresh().await;
    store
        .record_timed(DOMAIN, &run(Status::Pass), &[], 1000, Some(4210), d)
        .await
        .unwrap();
    let again = store
        .record_timed(DOMAIN, &run(Status::Pass), &[], 2000, Some(100), d)
        .await
        .unwrap();
    assert!(matches!(again, Recorded::Identical { .. }), "{again:?}");
    let entries = store.entries(DOMAIN).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].duration_ms, Some(4210));
}

#[tokio::test]
async fn entries_recorded_before_the_migration_read_back_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let r = run(Status::Pass);
    let first_migration = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/0001_create_entries.sql"),
    )
    .unwrap();

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&path).create_if_missing(true))
        .await
        .unwrap();
    sqlx::raw_sql(&first_migration).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO entries (id, domain, created_at, last_seen_at, predecessor_id, \
         profile_version, grade, fingerprint, run, diff, engine_version) \
         VALUES ('legacy01', ?, 500, 500, NULL, 3, 'A', 'fp-legacy', ?, NULL, '0.26.0')",
    )
    .bind(DOMAIN)
    .bind(serde_json::to_string(&r).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let store = HistoryStore::open(&path).await.unwrap();
    store.migrate().await.unwrap();
    let entries = store.entries(DOMAIN).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "legacy01");
    assert_eq!(entries[0].duration_ms, None);
}
