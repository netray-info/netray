//! Contract tests for `HistoryStore::head` (spec v2-admission, Phase 3, requirement 7).
use netray_model::diff::{Diff, diff};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{Head, HistoryStore, Recorded};

const DOMAIN: &str = "example.com";

fn run(status: Status, grade: Grade, profile_version: u32) -> Run {
    Run {
        profile_version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
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
async fn head_is_the_chain_head_and_follows_identical_records() {
    let (store, _dir) = fresh().await;
    let a = run(Status::Pass, Grade::A, 3);
    let b = run(Status::Warn, Grade::B, 3);
    let c = run(Status::Fail, Grade::D, 4);
    store.record(DOMAIN, &a, &[], 1000, d).await.unwrap();
    store.record(DOMAIN, &b, &[], 2000, d).await.unwrap();
    let Recorded::Changed { id, .. } = store.record(DOMAIN, &c, &[], 3000, d).await.unwrap()
    else {
        panic!("expected Changed");
    };

    let latest = store.latest(DOMAIN).await.unwrap().unwrap();
    assert_eq!(latest.id, id);
    assert_eq!(
        store.head(DOMAIN).await.unwrap(),
        Some(Head {
            id: id.clone(),
            last_seen_at: 3000,
            grade: Grade::D,
            profile_version: 4,
        })
    );

    let rec = store.record(DOMAIN, &c, &[], 4000, d).await.unwrap();
    assert_eq!(rec, Recorded::Identical { id: id.clone() });
    assert_eq!(
        store.head(DOMAIN).await.unwrap(),
        Some(Head {
            id,
            last_seen_at: 4000,
            grade: Grade::D,
            profile_version: 4,
        })
    );
}

#[tokio::test]
async fn head_of_an_unknown_domain_is_none() {
    let (store, _dir) = fresh().await;
    store
        .record(DOMAIN, &run(Status::Pass, Grade::A, 3), &[], 1000, d)
        .await
        .unwrap();
    assert_eq!(store.head("example.org").await.unwrap(), None);
}
