# Spec: v2 model

Status: Ready for Implementation
Created: 2026-10-09

## Goal

Two new crates exist without changing any result: `netray-model` holds V2's check vocabulary as types, and every V1 status word of every crate maps onto it once, by a function a test drives over every variant; `netray-engine` defines the `Module` and `FactsProvider` traits, and a repository check fails when the engine depends on a module crate.

## Non-goals

- Evidence shapes, the catalogue (`checks.toml`), profiles, the orchestrator (V2 Phase 1b and V1.4–V1.7).
- Renaming the module crates (V1.3, feature `v2-modules`).
- Any V1 crate using the model for its own results: the mappings exist and are tested; nothing calls them yet.

## Context and constraints

- V2 SDD (planning repo `specs/sdd/v2.md`): §3.1 crates and dependency direction (modules → `engine` → `model`; the engine never names a module crate, P40); §3.5 check model; §3.6 the traits; Phase 1a criteria V1.1 and V1.2.
- V1 status words today:
  - tlsight `CheckStatus { Pass, Warn, Fail, Skip }` (`crates/tlsight/src/validate/mod.rs:12`) and the connect-error code `NOT_TESTED_FROM_HERE` (`crates/tlsight/src/tls/mod.rs:62`);
  - spectra `CheckStatus { Pass, Skip, Warn, Fail }` (`crates/spectra/src/quality/types.rs:6`);
  - beacon `Verdict { Skip, Info, Pass, Warn, Fail }` (`crates/beacon/src/quality/types.rs:21`);
  - mhost 0.12.0 lint `CheckResult { NotFound, Ok, Warning, Failed }` as prism reads it (`mhost-0.12.0/src/lints/mod.rs:53`);
  - lens `CheckVerdict { Pass, Warn, Fail, NotFound, Skip }` (`crates/lens/src/scoring/engine.rs:6`) and its grade strings `A+ A B C D F` (`crates/lens/profiles/default.toml`, `[thresholds]`) plus `incomplete` (`engine.rs:162`).
- SDD §3.5 mapping: `NotFound` → `fail`, `Skip` → `not_applicable`, beacon `Info` → a finding on a `pass`; `NOT_TESTED_FROM_HERE` → `not_tested`.
- The workspace takes every `crates/*` as a member (`Cargo.toml`); repository checks are `tests/repo/test_*.sh`.

## Requirements

1. `crates/model` (package `netray-model`, no I/O, no dependency on another workspace crate) holds `CheckId` (`<protocol>.<name>`, parsed and refused otherwise), `Protocol` (`dns tls http email ip`), `Status` (`pass warn fail not_applicable not_tested unmeasured`), `Severity` (`critical high medium low`), `FixOwner` (`dns_provider registrar certificate_provider web_server mail_provider hosting`), `CheckResult` (`id`, `status`, `findings`, `evidence` as a list of block IDs) and `Grade` (`A+ A B C D F` or `Incomplete`). Each serialises in snake_case as listed, `Grade` as the letter or `incomplete`.
2. Each V1 crate with a status word maps it once onto `netray_model::Status`: a `From` impl where the type is the crate's own (tlsight, spectra, beacon, lens), a function where the orphan rule forbids it (prism over mhost's `CheckResult`; tlsight's `NOT_TESTED_FROM_HERE` code). Beacon's `Info` maps to `pass`; the caller keeps the message as a finding. lens maps its grade strings onto `Grade` and refuses any other string.
3. A test in each mapping crate covers every variant of the V1 type, with an exhaustive `match` so a new variant fails to compile.
4. `crates/engine` (package `netray-engine`) depends on `netray-model` only, among workspace crates, and defines `Module` and `FactsProvider` with the signatures of SDD §3.6, plus the types they name (`RunContext`, `Facts`, `SectionOutcome`, `EvidencePath`, `Domain`, `ResolveError`) as far as the signatures need them. No orchestrator.
5. `tests/repo/test_engine_names_no_module.sh` fails when `crates/engine/Cargo.toml` lists a module crate (`beacon`, `tlsight`, `spectra`, `prism`, `ifconfig-rs`, `lens`, or any later `netray-dns|tls|http|email|ip`) as a dependency of any kind.
6. Every result stays as today: the lens result goldens (`crates/lens/tests/lens_golden.rs`) and every results table pass unchanged.

## Phase 1 — Model and mappings

**Depends on:** none
**Requirements:** 1, 2, 3, 6

### Test Scenarios

- GIVEN `"tls.chain_trusted"` WHEN parsed as `CheckId` THEN protocol `tls`, name `chain_trusted`.
- GIVEN `"chain_trusted"`, `"smtp.x"` or `"tls."` WHEN parsed as `CheckId` THEN refused.
- GIVEN each `Status` variant WHEN serialised THEN `pass`, `warn`, `fail`, `not_applicable`, `not_tested`, `unmeasured`.
- GIVEN `Grade::Incomplete` and `Grade` A+ WHEN serialised THEN `"incomplete"` and `"A+"`.
- GIVEN tlsight `Skip`, spectra `Skip`, beacon `Skip` WHEN mapped THEN `not_applicable`.
- GIVEN beacon `Info` WHEN mapped THEN `pass`.
- GIVEN lens `NotFound` and mhost `NotFound()` WHEN mapped THEN `fail`; mhost `Ok`, `Warning`, `Failed` → `pass`, `warn`, `fail`.
- GIVEN tlsight error code `NOT_TESTED_FROM_HERE` WHEN mapped THEN `not_tested`; GIVEN `HANDSHAKE_FAILED` THEN not `not_tested` (`unmeasured`).
- GIVEN lens grade `"incomplete"` and `"B"` WHEN mapped THEN `Grade::Incomplete` and `Grade::B`; GIVEN `"E"` THEN refused.
- GIVEN the lens goldens and results tables WHEN run THEN unchanged.

## Phase 2 — Engine traits

**Depends on:** 1
**Requirements:** 4, 5

### Test Scenarios

- GIVEN a stub type implementing `Module` WHEN `run` is called with a `RunContext` and `Facts` THEN it returns its `SectionOutcome` (the trait is object-safe: used as `Box<dyn Module>`).
- GIVEN a stub `FactsProvider` WHEN `resolve` is called THEN it returns `Facts` or `ResolveError`.
- GIVEN `crates/engine/Cargo.toml` as committed WHEN the repo check runs THEN it passes.
- GIVEN a fixture `Cargo.toml` listing `beacon` under `[dependencies]`, `[dev-dependencies]` or `[build-dependencies]` WHEN the repo check runs on it THEN it fails, naming the crate.
- GIVEN a fixture listing only `netray-model` and external crates WHEN the check runs THEN it passes.

## Decision log

- Four features for Phase 1a, this one first (V1.1, V1.2), then `v2-modules`, `v2-engine-in-process`, `v2-metrics` (operator, 2026-10-09).
- The V1 mapping lives in the V1 crates as `From` impls, over mirror enums in `netray-model`: modules depend on the model (§3.1), no second copy drifts (P26), and V1.3 carries the impls along with the renames (operator, 2026-10-09).
- A function instead of `From` where the orphan rule forbids it (prism over mhost; a string code in tlsight): forced by Rust, not a choice.
- `HANDSHAKE_FAILED` and other connect errors map to `unmeasured`, over `fail`: they say the check could not be measured; security-correctness R5.3 keeps EHOSTUNREACH target-side as HANDSHAKE_FAILED, and V2's model separates "broken" from "could not check" (S13), and `tls.tls_reachable` carries unreachability (S28) (operator, 2026-10-09).

## Open decisions

None.

## Out of scope

- `Section`, `Run`, `Evidence`, the diff and run equality in `netray-model` (V2 Phases 1b and 2).
