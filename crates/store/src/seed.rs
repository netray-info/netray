//! Seeds a history store from fixture files (`just acceptance-local`, spec v2-inspectors req. 23).

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use netray_model::run::Run;

use crate::{HistoryStore, StoreError};

const KEYS: [&str; 3] = ["domain", "age_seconds", "run"];

/// Why seeding failed; fixture errors name the file.
#[derive(Debug)]
pub struct SeedError(Kind);

#[derive(Debug)]
enum Kind {
    Fixture(String),
    Store(StoreError),
}

impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Kind::Fixture(what) => write!(f, "seed fixture: {what}"),
            Kind::Store(e) => write!(f, "seed: {e}"),
        }
    }
}

impl std::error::Error for SeedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            Kind::Fixture(_) => None,
            Kind::Store(e) => Some(e),
        }
    }
}

impl From<StoreError> for SeedError {
    fn from(e: StoreError) -> Self {
        SeedError(Kind::Store(e))
    }
}

fn fixture_err(what: String) -> SeedError {
    SeedError(Kind::Fixture(what))
}

struct Fixture {
    domain: String,
    age: u64,
    run: Run,
}

/// Records every `*.json` fixture in `fixture_dir`, by file name, at `seed_now - age_seconds`;
/// returns the count. All fixtures are validated before the store is opened.
pub async fn seed(db_path: &Path, fixture_dir: &Path, seed_now: i64) -> Result<usize, SeedError> {
    let fixtures = load(fixture_dir)?;
    let store = HistoryStore::open(db_path).await?;
    store.migrate().await?;
    for f in &fixtures {
        let age = i64::try_from(f.age).unwrap_or(i64::MAX);
        store
            .record(
                &f.domain,
                &f.run,
                &[],
                seed_now.saturating_sub(age),
                |p, c| netray_model::diff::diff(p, c, &[], None),
            )
            .await?;
    }
    Ok(fixtures.len())
}

fn load(dir: &Path) -> Result<Vec<Fixture>, SeedError> {
    let read = |e: std::io::Error| fixture_err(format!("{}: {e}", dir.display()));
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(read)? {
        let path = entry.map_err(read)?.path();
        if path.extension().is_some_and(|x| x == "json") {
            names.push(path);
        }
    }
    names.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    let mut fixtures = Vec::new();
    let mut last: HashMap<String, (u64, String)> = HashMap::new();
    for path in names {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bad = |what: &str| fixture_err(format!("{name}: {what}"));
        let bytes = std::fs::read(&path).map_err(|e| bad(&e.to_string()))?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| bad(&e.to_string()))?;
        let serde_json::Value::Object(mut obj) = value else {
            return Err(bad("not a JSON object"));
        };
        if let Some(key) = obj.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(bad(&format!(
                "unknown top-level key `{key}` (allowed: domain, age_seconds, run)"
            )));
        }
        let domain = match obj.remove("domain") {
            Some(serde_json::Value::String(s)) => s,
            _ => return Err(bad("`domain` missing or not a string")),
        };
        let age = obj
            .remove("age_seconds")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| bad("`age_seconds` missing or not an unsigned integer"))?;
        let run = obj.remove("run").ok_or_else(|| bad("`run` missing"))?;
        let run: Run = serde_json::from_value(run).map_err(|e| bad(&format!("`run`: {e}")))?;
        if !run.complete {
            return Err(bad("run is incomplete; record stores only complete runs"));
        }

        if let Some((prev_age, prev_name)) = last.get(&domain)
            && age >= *prev_age
        {
            return Err(fixture_err(format!(
                "{prev_name} and {name}: age_seconds of {domain} must strictly decrease in file order ({prev_age} then {age})"
            )));
        }
        last.insert(domain.clone(), (age, name));
        fixtures.push(Fixture { domain, age, run });
    }
    Ok(fixtures)
}
