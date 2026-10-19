//! Contract tests for seeding the store from fixtures (spec v2-inspectors, Phase 8, requirement 23).
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_store::seed::seed;
use std::path::Path;

const DOMAIN: &str = "example.com";
const SEED_NOW: i64 = 1_760_000_000;

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

fn write(dir: &Path, name: &str, age: u64, status: Status) {
    let v = serde_json::json!({"domain": DOMAIN, "age_seconds": age, "run": run(status)});
    std::fs::write(dir.join(name), serde_json::to_vec(&v).unwrap()).unwrap();
}

async fn entries(db: &Path) -> Vec<netray_store::Entry> {
    let store = HistoryStore::open(db).await.unwrap();
    store.migrate().await.unwrap();
    store.entries(DOMAIN).await.unwrap()
}

#[tokio::test]
async fn single_fixture_is_recorded_at_seed_now() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = tmp.path().join("fx");
    std::fs::create_dir(&fx).unwrap();
    let db = tmp.path().join("history.db");
    write(&fx, "example.com.json", 0, Status::Pass);

    let n = seed(&db, &fx, SEED_NOW).await.unwrap();

    assert_eq!(n, 1);
    let e = entries(&db).await;
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].domain, DOMAIN);
    assert_eq!(e[0].created_at, SEED_NOW);
    assert_eq!(e[0].last_seen_at, SEED_NOW);
}

#[tokio::test]
async fn fixtures_of_one_domain_are_recorded_in_file_name_order() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = tmp.path().join("fx");
    std::fs::create_dir(&fx).unwrap();
    let db = tmp.path().join("history.db");
    write(&fx, "a.json", 600, Status::Pass);
    write(&fx, "b.json", 0, Status::Fail);

    let n = seed(&db, &fx, SEED_NOW).await.unwrap();

    assert_eq!(n, 2);
    let mut e = entries(&db).await;
    e.sort_by_key(|x| x.created_at);
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].created_at, SEED_NOW - 600);
    assert_eq!(e[1].created_at, SEED_NOW);
    assert_eq!(e[0].predecessor_id, None);
    assert_eq!(e[1].predecessor_id.as_ref(), Some(&e[0].id));
}

#[tokio::test]
async fn out_of_order_ages_fail_naming_both_files_and_store_nothing() {
    for later_age in [600u64, 0] {
        let tmp = tempfile::tempdir().unwrap();
        let fx = tmp.path().join("fx");
        std::fs::create_dir(&fx).unwrap();
        let db = tmp.path().join("history.db");
        // a.json is processed first and must be strictly older than b.json.
        write(&fx, "a.json", 0, Status::Pass);
        write(&fx, "b.json", later_age, Status::Fail);

        let err = seed(&db, &fx, SEED_NOW).await.unwrap_err().to_string();

        assert!(err.contains("a.json"), "age {later_age}: {err}");
        assert!(err.contains("b.json"), "age {later_age}: {err}");
        assert!(entries(&db).await.is_empty(), "age {later_age}");
    }
}

#[tokio::test]
async fn fixture_with_extra_top_level_key_fails_naming_the_file_and_stores_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = tmp.path().join("fx");
    std::fs::create_dir(&fx).unwrap();
    let db = tmp.path().join("history.db");
    let v = serde_json::json!({
        "domain": DOMAIN, "age_seconds": 0, "created_at": 1_760_000_000u64,
        "run": run(Status::Pass),
    });
    std::fs::write(fx.join("stamped.json"), serde_json::to_vec(&v).unwrap()).unwrap();

    let err = seed(&db, &fx, SEED_NOW).await.unwrap_err().to_string();

    assert!(err.contains("stamped.json"), "{err}");
    assert!(entries(&db).await.is_empty());
}

#[tokio::test]
async fn incomplete_run_fails_naming_the_file_and_stores_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let fx = tmp.path().join("fx");
    std::fs::create_dir(&fx).unwrap();
    let db = tmp.path().join("history.db");
    let mut incomplete = run(Status::Pass);
    incomplete.complete = false;
    let v = serde_json::json!({"domain": DOMAIN, "age_seconds": 0, "run": incomplete});
    std::fs::write(fx.join("partial.json"), serde_json::to_vec(&v).unwrap()).unwrap();

    let err = seed(&db, &fx, SEED_NOW).await.unwrap_err().to_string();

    assert!(err.contains("partial.json"), "{err}");
    assert!(entries(&db).await.is_empty());
}
