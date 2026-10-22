//! The history fixtures are what lens computes from their own evidence
//! (specs/features/v2-fixture-consistency/spec.md, Phase 3, requirements 7 and 8; report.md C1,
//! C2, C5, C6, C7).
//!
//! Each fixture under `tests/acceptance/fixtures/history/` is recomputed: lens gets one module per
//! section of the stored run, answering `netray_<p>::testing::from_evidence` over the run's blocks
//! (or the section's recorded not-applicable or incomplete reason), runs its check with
//! `tests/fixtures/lens.production.toml`'s section plan and builds the run with `build_run`. The
//! result must equal the stored run in everything but `engine_version`. `UPDATE_FIXTURES=1`
//! rewrites each fixture's run with the recomputed one, keeping `domain`, `age_seconds` and
//! `engine_version`.
//!
//! Copied from crates/lens/tests/it/history_fixtures_consistent.rs (spec v2-cutover, Phase 6);
//! lens's `run_check_with_input` and `build_run` are `netray_engine::domain_run`'s
//! `run_check_streaming` and `build_run` over the production section plan, the resolve budget
//! and the current profile; nothing dropped.

use std::path::{Path, PathBuf};

use netray_engine::domain_run::{CheckInput, HARD_DEADLINE, build_run, run_check_streaming};
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, SectionOutcome,
};
use netray_model::evidence::{Block, Shape, Value};
use netray_model::run::{Run, SectionKind};
use netray_model::{CheckId, Protocol, Status};
use serde::{Deserialize, Serialize};

use crate::common;

/// One fixture file: `{"domain", "age_seconds", "run"}`, in the files' field order.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureFile {
    domain: String,
    age_seconds: u64,
    run: Run,
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/acceptance/fixtures/history")
}

/// Every `*.json` fixture, sorted by path.
fn fixture_paths() -> Vec<PathBuf> {
    let d = dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&d)
        .unwrap_or_else(|e| panic!("fixture directory {} is unreadable: {e}", d.display()))
        .map(|e| e.expect("directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} holds no *.json fixture", d.display());
    files
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().to_string()
}

/// The fixture as JSON and as its typed form.
fn load(path: &Path) -> (serde_json::Value, FixtureFile) {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));
    let value: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("fixture {} is not JSON: {e}", path.display()));
    let typed: FixtureFile = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("fixture {} does not parse: {e}", path.display()));
    (value, typed)
}

/// A module answering a precomputed outcome.
struct FixedModule {
    protocol: Protocol,
    outcome: SectionOutcome,
}

impl Module for FixedModule {
    fn protocol(&self) -> Protocol {
        self.protocol
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn needs_addresses(&self) -> bool {
        false
    }
    fn wants_facts(&self) -> bool {
        false
    }
    fn needs_address_records(&self) -> bool {
        false
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move { self.outcome.clone() })
    }
}

/// The resolve stage of the run: the A and AAAA addresses of its `dns.a` and `dns.aaaa` blocks
/// (lens's IP section is `NoDnsResults` without them).
struct ResolvedFacts(Facts);

impl FactsProvider for ResolvedFacts {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

fn resolved_facts(blocks: &[Block]) -> Facts {
    let mut facts = Facts::default();
    for block in blocks {
        let Shape::Records(records) = &block.shape else {
            continue;
        };
        for r in records {
            let (Value::Text(rtype), Value::Text(data)) = (&r.rtype, &r.data) else {
                continue;
            };
            match (block.id.to_string().as_str(), rtype.as_str()) {
                ("dns.a", "A") => facts
                    .a
                    .push(data.parse().expect("dns.a data is an address")),
                ("dns.aaaa", "AAAA") => facts
                    .aaaa
                    .push(data.parse().expect("dns.aaaa data is an address")),
                _ => {}
            }
        }
    }
    facts
}

async fn from_evidence(protocol: Protocol, domain: &str, blocks: &[Block]) -> SectionOutcome {
    match protocol {
        Protocol::Dns => netray_dns::testing::from_evidence(domain, blocks).await,
        Protocol::Tls => netray_tls::testing::from_evidence(domain, blocks).await,
        Protocol::Http => netray_http::testing::from_evidence(domain, blocks).await,
        Protocol::Email => netray_email::testing::from_evidence(domain, blocks).await,
        Protocol::Ip => netray_ip::testing::from_evidence(domain, blocks).await,
    }
}

/// lens's run over modules answering `from_evidence` of `run`'s blocks.
async fn recompute(domain: &str, run: &Run) -> Run {
    let mut registry = Registry::new();
    for section in &run.sections {
        let reason = section.reason.clone().unwrap_or_default();
        let outcome = match section.outcome {
            SectionKind::Measured => from_evidence(section.protocol, domain, &run.blocks).await,
            SectionKind::NotApplicable => SectionOutcome::NotApplicable { reason },
            SectionKind::Incomplete => SectionOutcome::Incomplete { reason },
            SectionKind::TimedOut => SectionOutcome::TimedOut,
        };
        registry = registry.with(Box::new(FixedModule {
            protocol: section.protocol,
            outcome,
        }));
    }
    registry = registry.with_facts(Box::new(ResolvedFacts(resolved_facts(&run.blocks))));
    let plan = common::production_plan(&registry);
    let profile = common::profile();
    let (output, _facts) = run_check_streaming(
        &registry,
        &plan,
        common::RESOLVE_BUDGET,
        &profile,
        CheckInput {
            domain: domain.to_string(),
            dkim_selectors: None,
        },
        HARD_DEADLINE,
        |_| {},
    )
    .await;
    build_run(&profile, &output, &[])
}

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap()
}

/// `Ok` when `stored` equals `recomputed` except for `engine_version`; otherwise the fixture `name`
/// and the first difference: a check id, a block id or a top-level field.
fn compare(name: &str, stored: &Run, recomputed: &Run) -> Result<(), String> {
    for s in &stored.checks {
        let Some(r) = recomputed.checks.iter().find(|r| r.id == s.id) else {
            return Err(format!(
                "{name}: check {} is missing from the recomputed run",
                s.id
            ));
        };
        if s.status != r.status {
            return Err(format!(
                "{name}: check {} status stored {} recomputed {}",
                s.id,
                json(&s.status),
                json(&r.status)
            ));
        }
        if s.findings != r.findings {
            return Err(format!(
                "{name}: check {} findings stored {:?} recomputed {:?}",
                s.id, s.findings, r.findings
            ));
        }
        if s.evidence != r.evidence {
            return Err(format!(
                "{name}: check {} evidence stored {} recomputed {}",
                s.id,
                json(&s.evidence),
                json(&r.evidence)
            ));
        }
    }
    if let Some(r) = recomputed
        .checks
        .iter()
        .find(|r| !stored.checks.iter().any(|s| s.id == r.id))
    {
        return Err(format!(
            "{name}: check {} is extra in the recomputed run",
            r.id
        ));
    }
    if stored.checks != recomputed.checks {
        return Err(format!("{name}: checks are in a different order"));
    }

    for s in &stored.blocks {
        let Some(r) = recomputed.blocks.iter().find(|r| r.id == s.id) else {
            return Err(format!(
                "{name}: block {} is missing from the recomputed run",
                s.id
            ));
        };
        if s != r {
            return Err(format!(
                "{name}: block {} stored {} recomputed {}",
                s.id,
                json(s),
                json(r)
            ));
        }
    }
    if let Some(r) = recomputed
        .blocks
        .iter()
        .find(|r| !stored.blocks.iter().any(|s| s.id == r.id))
    {
        return Err(format!(
            "{name}: block {} is extra in the recomputed run",
            r.id
        ));
    }
    if stored.blocks != recomputed.blocks {
        return Err(format!("{name}: blocks are in a different order"));
    }

    if stored.grade != recomputed.grade {
        return Err(format!(
            "{name}: grade stored {} recomputed {}",
            json(&stored.grade),
            json(&recomputed.grade)
        ));
    }
    if stored.complete != recomputed.complete {
        return Err(format!(
            "{name}: complete stored {} recomputed {}",
            stored.complete, recomputed.complete
        ));
    }
    if stored.profile_version != recomputed.profile_version {
        return Err(format!(
            "{name}: profile_version stored {} recomputed {}",
            stored.profile_version, recomputed.profile_version
        ));
    }
    if stored.evidence_schema_version != recomputed.evidence_schema_version {
        return Err(format!(
            "{name}: evidence_schema_version stored {} recomputed {}",
            stored.evidence_schema_version, recomputed.evidence_schema_version
        ));
    }
    if stored.sections != recomputed.sections {
        return Err(format!(
            "{name}: sections stored {} recomputed {}",
            json(&stored.sections),
            json(&recomputed.sections)
        ));
    }
    Ok(())
}

/// The fixture with its run replaced by `recomputed`, keeping `domain`, `age_seconds` and the
/// stored run's `engine_version`.
fn regenerated(fixture: &serde_json::Value, recomputed: &Run) -> FixtureFile {
    let mut run = recomputed.clone();
    run.engine_version = fixture["run"]["engine_version"]
        .as_str()
        .expect("fixture run has an engine_version")
        .to_string();
    FixtureFile {
        domain: fixture["domain"]
            .as_str()
            .expect("fixture has a domain")
            .to_string(),
        age_seconds: fixture["age_seconds"]
            .as_u64()
            .expect("fixture has age_seconds"),
        run,
    }
}

/// The file text of `fixture`: two-space pretty JSON with a trailing newline.
fn render(fixture: &FixtureFile) -> String {
    let mut text = serde_json::to_string_pretty(fixture).unwrap();
    text.push('\n');
    text
}

#[tokio::test]
async fn every_history_fixture_is_its_recomputed_run() {
    let update = std::env::var("UPDATE_FIXTURES").is_ok_and(|v| v == "1");
    let mut failures = Vec::new();
    for path in fixture_paths() {
        let name = file_name(&path);
        let (value, fixture) = load(&path);
        let recomputed = recompute(&fixture.domain, &fixture.run).await;
        if update {
            std::fs::write(&path, render(&regenerated(&value, &recomputed)))
                .unwrap_or_else(|e| panic!("cannot write fixture {}: {e}", path.display()));
            continue;
        }
        if let Err(e) = compare(&name, &fixture.run, &recomputed) {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "{} fixture(s) differ from their recomputed run:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[tokio::test]
async fn guard_names_the_fixture_and_check_on_a_contradiction() {
    let path = dir().join("new.example.com.json");
    let (_, fixture) = load(&path);
    let recomputed = recompute(&fixture.domain, &fixture.run).await;

    // The stored run is the recomputed one with email.dmarc flipped, so the contradiction is the
    // only difference whatever else the committed fixture holds.
    let mut mutated = recomputed.clone();
    let dmarc = mutated
        .checks
        .iter_mut()
        .find(|c| c.id.to_string() == "email.dmarc")
        .expect("recomputed run holds email.dmarc");
    dmarc.status = if dmarc.status == Status::Fail {
        Status::Pass
    } else {
        Status::Fail
    };

    let err = compare(&file_name(&path), &mutated, &recomputed)
        .expect_err("a flipped email.dmarc status differs from the recomputed run");
    assert!(
        err.contains("new.example.com"),
        "message names the fixture: {err}"
    );
    assert!(
        err.contains("email.dmarc"),
        "message names the check: {err}"
    );
}

#[tokio::test]
async fn regenerated_fixture_keeps_its_identity() {
    let path = dir().join("shop.example.com-1.json");
    let (value, fixture) = load(&path);
    let recomputed = recompute(&fixture.domain, &fixture.run).await;

    let text = render(&regenerated(&value, &recomputed));
    let reparsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(reparsed["domain"], "shop.example.com");
    assert_eq!(reparsed["age_seconds"], 1_209_600);
    assert_eq!(
        reparsed["run"]["engine_version"],
        value["run"]["engine_version"]
    );

    let again: FixtureFile = serde_json::from_str(&text).unwrap();
    let recomputed_again = recompute(&again.domain, &again.run).await;
    compare(&file_name(&path), &again.run, &recomputed_again)
        .unwrap_or_else(|e| panic!("a regenerated fixture passes the guard: {e}"));
}
