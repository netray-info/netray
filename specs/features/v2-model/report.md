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

### Behavioural verification

skipped: no entry point; nothing calls the new types or mappings yet (spec non-goal).
