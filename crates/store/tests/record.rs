//! Contract tests for recording a run (spec v2-history, Phase 3, requirements 9 and 10).
use netray_model::diff::{Diff, diff};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind, fingerprint};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};

const DOMAIN: &str = "example.com";

fn run(status: Status, complete: bool, grade: Grade) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![
            RunCheck {
                id: CheckId::parse("dns.dnssec_valid").unwrap(),
                status,
            },
            RunCheck {
                id: CheckId::parse("tls.caa_compliant").unwrap(),
                status: Status::Pass,
            },
        ],
        blocks: vec![],
    }
}

fn complete(status: Status) -> Run {
    run(status, true, Grade::A)
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
async fn first_complete_run_is_stored_without_predecessor_or_diff() {
    let (store, _dir) = fresh().await;
    let r = complete(Status::Pass);
    let rec = store.record(DOMAIN, &r, &[], 1000, d).await.unwrap();
    let Recorded::First { id } = rec else {
        panic!("expected First, got {rec:?}");
    };
    let entries = store.entries(DOMAIN).await.unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.id, id);
    assert_eq!(e.predecessor_id, None);
    assert_eq!(e.diff, None);
    assert_eq!(e.created_at, 1000);
    assert_eq!(e.last_seen_at, 1000);
    assert_eq!(e.fingerprint, fingerprint(&r, &[]));
    assert_eq!(e.run, r);
    assert_eq!(e.grade, r.grade);
}

#[tokio::test]
async fn identical_run_only_moves_last_seen_at() {
    let (store, _dir) = fresh().await;
    let r = complete(Status::Pass);
    let Recorded::First { id } = store.record(DOMAIN, &r, &[], 1000, d).await.unwrap() else {
        panic!("expected First");
    };
    let rec = store.record(DOMAIN, &r, &[], 2000, d).await.unwrap();
    assert_eq!(rec, Recorded::Identical { id });
    let entries = store.entries(DOMAIN).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].last_seen_at, 2000);
    assert_eq!(entries[0].created_at, 1000);
}

#[tokio::test]
async fn changed_run_adds_entry_with_predecessor_and_diff() {
    let (store, _dir) = fresh().await;
    let first = complete(Status::Pass);
    let second = complete(Status::Fail);
    let Recorded::First { id: first_id } =
        store.record(DOMAIN, &first, &[], 1000, d).await.unwrap()
    else {
        panic!("expected First");
    };
    let rec = store.record(DOMAIN, &second, &[], 3000, d).await.unwrap();
    let Recorded::Changed { id, diff: returned } = rec else {
        panic!("expected Changed, got {rec:?}");
    };
    assert_eq!(store.entries(DOMAIN).await.unwrap().len(), 2);
    let latest = store.latest(DOMAIN).await.unwrap().unwrap();
    assert_eq!(latest.id, id);
    assert_eq!(latest.predecessor_id, Some(first_id));
    assert_eq!(latest.diff, Some(returned.clone()));
    let change = returned
        .checks
        .iter()
        .find(|c| c.id.to_string() == "dns.dnssec_valid")
        .expect("status change in diff");
    assert_eq!(change.from, Some(Status::Pass));
    assert_eq!(change.to, Some(Status::Fail));
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_different_runs_chain_into_two_entries() {
    let (store, _dir) = fresh().await;
    let a = complete(Status::Pass);
    let b = complete(Status::Fail);
    let (ra, rb) = tokio::join!(
        store.record(DOMAIN, &a, &[], 1000, d),
        store.record(DOMAIN, &b, &[], 1000, d),
    );
    ra.unwrap();
    rb.unwrap();
    let entries = store.entries(DOMAIN).await.unwrap();
    assert_eq!(entries.len(), 2);
    let preds: Vec<_> = entries
        .iter()
        .filter_map(|e| e.predecessor_id.clone())
        .collect();
    assert_eq!(preds.len(), 1);
    let root = entries.iter().find(|e| e.predecessor_id.is_none()).unwrap();
    assert_eq!(preds[0], root.id);
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_identical_runs_yield_one_entry() {
    let (store, _dir) = fresh().await;
    let a = complete(Status::Pass);
    let b = complete(Status::Pass);
    let (ra, rb) = tokio::join!(
        store.record(DOMAIN, &a, &[], 1000, d),
        store.record(DOMAIN, &b, &[], 1000, d),
    );
    ra.unwrap();
    rb.unwrap();
    assert_eq!(store.entries(DOMAIN).await.unwrap().len(), 1);
}

#[tokio::test]
async fn incomplete_run_is_not_stored_but_diffed_against_latest() {
    let (store, _dir) = fresh().await;
    let base = complete(Status::Pass);
    store.record(DOMAIN, &base, &[], 1000, d).await.unwrap();
    let partial = run(Status::Fail, false, Grade::F);
    let rec = store.record(DOMAIN, &partial, &[], 2000, d).await.unwrap();
    assert!(
        matches!(rec, Recorded::NotStored { diff: Some(_) }),
        "got {rec:?}"
    );
    assert_eq!(store.entries(DOMAIN).await.unwrap().len(), 1);
    assert_eq!(
        store.latest(DOMAIN).await.unwrap().unwrap().last_seen_at,
        1000
    );
}

#[tokio::test]
async fn incomplete_run_without_any_entry_is_not_stored_and_has_no_diff() {
    let (store, _dir) = fresh().await;
    let partial = run(Status::Fail, false, Grade::F);
    let rec = store.record(DOMAIN, &partial, &[], 2000, d).await.unwrap();
    assert_eq!(rec, Recorded::NotStored { diff: None });
    assert!(store.entries(DOMAIN).await.unwrap().is_empty());
}

#[tokio::test]
async fn incomplete_run_with_equal_fingerprint_does_not_move_last_seen_at() {
    let (store, _dir) = fresh().await;
    let base = complete(Status::Pass);
    store.record(DOMAIN, &base, &[], 1000, d).await.unwrap();
    let partial = run(Status::Pass, false, Grade::Incomplete);
    store.record(DOMAIN, &partial, &[], 5000, d).await.unwrap();
    let latest = store.latest(DOMAIN).await.unwrap().unwrap();
    assert_eq!(latest.last_seen_at, 1000);
}

#[tokio::test]
async fn latest_is_the_chain_head_at_equal_created_at() {
    let (store, _dir) = fresh().await;
    store
        .record(DOMAIN, &complete(Status::Pass), &[], 1000, d)
        .await
        .unwrap();
    let rec = store
        .record(DOMAIN, &complete(Status::Fail), &[], 1000, d)
        .await
        .unwrap();
    let Recorded::Changed { id, .. } = rec else {
        panic!("expected Changed, got {rec:?}");
    };
    assert_eq!(store.latest(DOMAIN).await.unwrap().unwrap().id, id);
}

#[tokio::test]
async fn a_clock_running_backwards_never_forks_the_chain() {
    let (store, _dir) = fresh().await;
    let a = complete(Status::Pass);
    let b = complete(Status::Fail);
    store.record(DOMAIN, &a, &[], 2000, d).await.unwrap();
    let rec = store.record(DOMAIN, &b, &[], 1999, d).await.unwrap();
    let Recorded::Changed { id: second, .. } = rec else {
        panic!("expected Changed, got {rec:?}");
    };
    assert_eq!(store.latest(DOMAIN).await.unwrap().unwrap().id, second);
    // the same run again is identical to the head, not a change against the first entry
    let rec = store.record(DOMAIN, &b, &[], 2001, d).await.unwrap();
    assert_eq!(rec, Recorded::Identical { id: second.clone() });
    // last_seen_at only moves forward
    store.record(DOMAIN, &b, &[], 1500, d).await.unwrap();
    let head = store.latest(DOMAIN).await.unwrap().unwrap();
    assert_eq!(head.last_seen_at, 2001);
    assert_eq!(store.entries(DOMAIN).await.unwrap().len(), 2);
}

#[tokio::test]
async fn entries_are_newest_first_at_equal_created_at() {
    let (store, _dir) = fresh().await;
    for status in [Status::Pass, Status::Fail, Status::Warn] {
        store
            .record(DOMAIN, &complete(status), &[], 1000, d)
            .await
            .unwrap();
    }
    let entries = store.entries(DOMAIN).await.unwrap();
    assert_eq!(entries.len(), 3);
    // each entry's predecessor is the next one in the list
    assert_eq!(
        entries[0].predecessor_id.as_deref(),
        Some(entries[1].id.as_str())
    );
    assert_eq!(
        entries[1].predecessor_id.as_deref(),
        Some(entries[2].id.as_str())
    );
    assert_eq!(entries[2].predecessor_id, None);
}
