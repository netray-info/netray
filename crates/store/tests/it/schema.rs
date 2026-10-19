//! Contract tests for the history store schema (spec v2-history, Phase 3, requirement 8).
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use std::path::PathBuf;

fn complete_run() -> Run {
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
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

#[tokio::test]
async fn open_migrate_twice_and_record_yields_eight_char_alphanumeric_id() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.sqlite"))
        .await
        .expect("open");
    store.migrate().await.expect("first migrate");
    store.migrate().await.expect("second migrate is idempotent");

    let recorded = store
        .record("example.com", &complete_run(), &[], 1_000, |_, _| {
            panic!("no diff expected for a first entry")
        })
        .await
        .expect("record");
    let Recorded::First { id } = recorded else {
        panic!("expected First, got {recorded:?}");
    };
    assert_eq!(id.len(), 8, "id: {id}");
    assert!(id.chars().all(|c| c.is_ascii_alphanumeric()), "id: {id}");
}

#[tokio::test]
async fn open_in_nonexistent_directory_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("missing").join("history.sqlite");
    assert!(HistoryStore::open(&path).await.is_err());
}

fn migrations() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("migrations directory exists") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("sql") {
            let sql = std::fs::read_to_string(&path).unwrap();
            out.push((path.display().to_string(), sql));
        }
    }
    out
}

fn normalized(sql: &str) -> String {
    sql.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("( ", "(")
        .replace(" )", ")")
        .replace(" ,", ",")
}

#[test]
fn migrations_define_the_portable_entries_schema() {
    let files = migrations();
    assert!(!files.is_empty(), "at least one .sql migration");
    let all: String = files
        .iter()
        .map(|(_, s)| normalized(s))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(all.contains("create table"), "creates a table");
    assert!(all.contains("entries"), "table entries");
    for column in [
        "id text primary key",
        "domain text not null",
        "created_at bigint not null",
        "last_seen_at bigint not null",
        "predecessor_id text",
        "profile_version bigint not null",
        "grade text not null",
        "fingerprint text not null",
        "run text not null",
        "diff text",
        "engine_version text not null",
    ] {
        assert!(all.contains(column), "missing column `{column}` in:\n{all}");
    }
    assert!(
        all.contains("references entries(id) on delete set null"),
        "predecessor_id ON DELETE SET NULL in:\n{all}"
    );
    assert!(
        all.contains("(domain, created_at desc)"),
        "index on (domain, created_at DESC) in:\n{all}"
    );
    assert!(all.contains("create index"), "an index is created");

    for (path, sql) in &files {
        let lower = sql.to_lowercase();
        for forbidden in [
            "autoincrement",
            "without rowid",
            "datetime(",
            "strftime(",
            "julianday(",
            "json_",
            "pragma",
            "integer primary key",
        ] {
            assert!(
                !lower.contains(forbidden),
                "{path} contains forbidden `{forbidden}`"
            );
        }
    }
}
