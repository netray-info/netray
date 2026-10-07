/// Integration tests for SnapshotStore (insert, get, sweep).
use chrono::Utc;
use lens::snapshot::{
    SNAPSHOT_TTL_SECS, Snapshot, SnapshotFinding, SnapshotSection, SnapshotStore,
};
use tempfile::NamedTempFile;

fn make_snapshot(domain: &str) -> Snapshot {
    Snapshot {
        shortid: String::new(), // assigned by store.insert
        domain: domain.to_string(),
        grade: "A".to_string(),
        score: 95.0,
        sections: vec![SnapshotSection {
            name: "DNS".to_string(),
            grade: "A".to_string(),
            passes: 5,
            warns: 0,
            fails: 0,
            skips: 0,
            findings: vec![SnapshotFinding {
                check_name: "dnssec".to_string(),
                verdict: "pass".to_string(),
                message: String::new(),
                earned: 5,
                possible: 5,
                fix_hint: None,
                fix_owner: None,
                guide_url: None,
            }],
        }],
        server_addresses: vec![],
        created_at: Utc::now(),
        lens_version: "0.9.1".to_string(),
    }
}

async fn open_store() -> (SnapshotStore, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let store = SnapshotStore::new(tmp.path()).await.unwrap();
    store.migrate().await.unwrap();
    (store, tmp)
}

#[tokio::test]
async fn insert_and_get_round_trip() {
    let (store, _tmp) = open_store().await;
    let snap = make_snapshot("example.com");
    let id = store.insert(snap).await.unwrap();
    assert_eq!(id.len(), 8, "shortid should be 8 chars");

    let retrieved = store.get(&id).await.unwrap();
    assert!(retrieved.is_some(), "should find snapshot by id");
    let r = retrieved.unwrap();
    assert_eq!(r.domain, "example.com");
    assert_eq!(r.grade, "A");
    assert_eq!(r.shortid, id, "embedded shortid must match");
}

#[tokio::test]
async fn get_unknown_id_returns_none() {
    let (store, _tmp) = open_store().await;
    let result = store.get("AAAAAAAA").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn sweep_expired_removes_old_rows() {
    let (store, _tmp) = open_store().await;
    let snap = make_snapshot("old.example.com");
    let id = store.insert(snap).await.unwrap();

    // Verify it exists
    assert!(store.get(&id).await.unwrap().is_some());

    // Sweep with TTL=-1 (threshold is 1 second in the future, so all rows are "expired")
    let deleted = store.sweep_expired(-1).await.unwrap();
    assert_eq!(deleted, 1, "one row should be deleted");

    // Should no longer be found via get (which also applies TTL filter)
    let result = store.get(&id).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn sweep_keeps_fresh_rows() {
    let (store, _tmp) = open_store().await;
    let snap = make_snapshot("fresh.example.com");
    store.insert(snap).await.unwrap();

    let deleted = store.sweep_expired(SNAPSHOT_TTL_SECS).await.unwrap();
    assert_eq!(deleted, 0, "fresh rows should not be deleted");
}

#[tokio::test]
async fn shortid_is_alphanumeric() {
    let (store, _tmp) = open_store().await;
    for _ in 0..10 {
        let id = store.insert(make_snapshot("example.com")).await.unwrap();
        assert!(
            id.chars().all(|c| c.is_ascii_alphanumeric()),
            "shortid must be alphanumeric, got: {id}"
        );
    }
}

#[tokio::test]
async fn multiple_inserts_get_unique_ids() {
    let (store, _tmp) = open_store().await;
    let mut ids = std::collections::HashSet::new();
    for _ in 0..20 {
        let id = store.insert(make_snapshot("example.com")).await.unwrap();
        ids.insert(id);
    }
    assert_eq!(ids.len(), 20, "all shortids should be unique");
}
