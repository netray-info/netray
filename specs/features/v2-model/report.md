# Report: v2 model

## Phase 1 — Model and mappings

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: `netray-model` holds CheckId, Protocol, Status, Severity, FixOwner, CheckResult, Grade, snake_case serialisation, no I/O, no workspace dependency | green | crates/model/tests/model.rs |
| C2 | R2: each V1 crate maps its status word once onto `Status` (From impl or function), lens grade strings onto `Grade` | green | crates/*/tests/model_status.rs |
| C3 | R3: a test per mapping crate covers every variant with an exhaustive match | green | crates/*/tests/model_status.rs |
| C4 | R6: lens goldens and results tables unchanged | already_implemented | existing (crates/lens/tests/lens_golden.rs, results tables) |
| C5 | `"tls.chain_trusted"` parses: protocol tls, name chain_trusted | green | crates/model/tests/model.rs |
| C6 | `"chain_trusted"`, `"smtp.x"`, `"tls."` refused | green | crates/model/tests/model.rs |
| C7 | Status serialises pass, warn, fail, not_applicable, not_tested, unmeasured | green | crates/model/tests/model.rs |
| C8 | Grade::Incomplete → "incomplete", A+ → "A+" | green | crates/model/tests/model.rs |
| C9 | tlsight, spectra, beacon Skip → not_applicable | green | crates/{tlsight,spectra,beacon}/tests/model_status.rs |
| C10 | beacon Info → pass | green | crates/beacon/tests/model_status.rs |
| C11 | lens NotFound, mhost NotFound → fail; mhost Ok/Warning/Failed → pass/warn/fail | green | crates/{lens,mhost-prism}/tests/model_status.rs |
| C12 | NOT_TESTED_FROM_HERE → not_tested; HANDSHAKE_FAILED → unmeasured | green | crates/tlsight/tests/model_status.rs |
| C13 | lens "incomplete" → Incomplete, "B" → B, "E" refused | green | crates/lens/tests/model_status.rs |
| C14 | lens goldens and results tables run unchanged | already_implemented | existing (crates/lens/tests/lens_golden.rs, results tables) |

RED: every new test fails because `crates/model` has no manifest yet (`failed to read crates/model/Cargo.toml`), the missing behaviour itself. C4/C14 are the existing goldens, green before and after.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet (fmt diff on the writer's test file, formatted by the orchestrator) | 35995 | 90 |
| G2 | 1 | sonnet (GROUP_CMD's `--all-targets` clippy hit existing test-code lints the gate does not lint; gate clippy clean) | 40481 | 153 |
| G3 | 0 | already_implemented | 0 | 0 |

### Reader

| class | at | finding |
|---|---|---|
| AMENDMENT | crates/tlsight/src/validate/mod.rs:26 (and spectra types.rs:18, beacon types.rs:34, lens engine.rs:23) | V1 `Skip` is overloaded: by design (not applicable) and for errors/timeouts (tlsight `quality/http.rs:133`, beacon's pipeline timeout `checks/mod.rs:111`). Spec R2 and SDD §3.5 map every `Skip` to `not_applicable`; the error cases are `unmeasured` by §3.5's own definition. affected_phase: 1, repaired_in_phase: no |
| AMENDMENT | crates/model/src/lib.rs:59 | R1 does not define a check name's characters: `tls.a.b`, `tls.chain trusted`, `tls.Chain` parse. affected_phase: 1, repaired_in_phase: no |

Resolved (operator, 2026-10-09): the `Skip` mapping stays for 1a; error and timeout `Skip`s become `unmeasured` in V2 Phase 1b with S13 (an SDD amendment for the planning session). Check names are `[a-z][a-z0-9_]*`; R1 amended and fixed in this phase.

### Behavioural verification

skipped: no entry point; nothing calls the new types or mappings yet (spec non-goal).

## Phase 2 — Engine traits

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R4: `netray-engine` depends on `netray-model` only among workspace crates, defines `Module` and `FactsProvider` and the types they name; no orchestrator | green | crates/engine/tests/traits.rs |
| C2 | R5: `tests/repo/test_engine_names_no_module.sh` fails when `crates/engine/Cargo.toml` lists a module crate as any kind of dependency | green | tests/repo/test_engine_names_no_module.sh |
| C3 | A stub `Module` returns its `SectionOutcome`; the trait is used as `Box<dyn Module>` | green | crates/engine/tests/traits.rs |
| C4 | A stub `FactsProvider` returns `Facts` or `ResolveError` | green | crates/engine/tests/traits.rs |
| C5 | The committed `crates/engine/Cargo.toml` passes the repo check | green | tests/repo/test_engine_names_no_module.sh |
| C6 | A fixture listing `beacon` under `[dependencies]`, `[dev-dependencies]` or `[build-dependencies]` fails the check, naming the crate | green | tests/repo/test_engine_names_no_module.sh |
| C7 | A fixture listing only `netray-model` and external crates passes | green | tests/repo/test_engine_names_no_module.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 29027 | 40 |

### Reader

| class | at | finding | outcome |
|---|---|---|---|
| BLOCKER | tests/repo/test_engine_names_no_module.sh:32 | the TOML text scan missed renamed dependencies (`package = "beacon"`) | fixed: the check reads `cargo metadata` |
| BLOCKER | tests/repo/test_engine_names_no_module.sh:23 | dotted keys before any header, inline tables under `[target.*]`, `[dependencies . beacon]` escaped the scan | fixed: same; self-test cases rename, dotted, target-inline, dev, build |
| AMENDMENT | crates/engine/src/lib.rs:74 | `checks()` returned `Vec<CheckId>` against SDD §3.6 without need | repaired in phase: `&'static [CheckId]`, a module holds its list in a `LazyLock` |

### Behavioural verification

skipped: no entry point (traits only). The dependency check was sabotage-tested: adding `[dev-dependencies.b] package = "beacon"` to `crates/engine/Cargo.toml` prints `FAIL: netray-engine depends on module crate beacon`; restored, it passes.
