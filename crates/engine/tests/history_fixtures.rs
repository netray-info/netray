//! History fixtures (V2 spec v2-inspectors, requirement 23): every run in
//! `tests/acceptance/fixtures/history/` holds each catalogue check exactly once, and this
//! feature's fixture is one `example.com` entry with every block shape and evidence references.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use netray_engine::catalogue;
use netray_model::evidence::ShapeKind;
use netray_model::run::Run;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    domain: String,
    age_seconds: u64,
    run: Run,
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/acceptance/fixtures/history")
}

fn load(path: &std::path::Path) -> Fixture {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("fixture {} does not parse: {e}", path.display()))
}

#[test]
fn every_history_fixture_run_holds_each_catalogue_id_exactly_once() {
    let d = dir();
    let entries = std::fs::read_dir(&d)
        .unwrap_or_else(|e| panic!("fixture directory {} is unreadable: {e}", d.display()));
    let mut files: Vec<PathBuf> = entries
        .map(|e| e.expect("directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} holds no *.json fixture", d.display());

    let catalogue_ids: BTreeSet<String> = catalogue()
        .entries()
        .iter()
        .map(|e| e.id.to_string())
        .collect();
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let fixture = load(&file);
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for check in &fixture.run.checks {
            *counts.entry(check.id.to_string()).or_default() += 1;
        }
        for id in &catalogue_ids {
            match counts.get(id) {
                None => panic!("fixture {name}: catalogue check {id} is missing"),
                Some(1) => {}
                Some(n) => panic!("fixture {name}: check {id} appears {n} times"),
            }
        }
        for id in counts.keys() {
            assert!(
                catalogue_ids.contains(id),
                "fixture {name}: check {id} is not in the catalogue"
            );
        }
    }
}

#[test]
fn example_com_fixture_holds_every_block_shape_and_evidence_references() {
    let path = dir().join("example.com.json");
    let fixture = load(&path);
    assert_eq!(fixture.domain, "example.com");
    assert_eq!(fixture.age_seconds, 0);

    let kinds: Vec<ShapeKind> = fixture.run.blocks.iter().map(|b| b.shape.kind()).collect();
    for want in [
        ShapeKind::Records,
        ShapeKind::Table,
        ShapeKind::Chain,
        ShapeKind::Fields,
        ShapeKind::Text,
    ] {
        assert!(kinds.contains(&want), "example.com run holds no {want:?} block");
    }

    assert!(
        fixture.run.checks.iter().any(|c| !c.findings.is_empty()),
        "no check carries findings"
    );
    assert!(
        fixture.run.checks.iter().any(|c| !c.evidence.is_empty()),
        "no check carries evidence references"
    );
    let ids: BTreeSet<String> = fixture.run.blocks.iter().map(|b| b.id.to_string()).collect();
    for check in &fixture.run.checks {
        for r in &check.evidence {
            assert!(
                ids.contains(&r.to_string()),
                "check {} references block {r} which the run does not hold",
                check.id
            );
        }
    }
}
