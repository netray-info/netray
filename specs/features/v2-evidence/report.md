# Report: v2-evidence

## Phase 1 — Model and engine

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: evidence model (Block, Shape ×6, Value ×6 tagged `{t,v}`, BlockId grammar, schema version 1, `CheckResult.evidence: Vec<BlockId>`, Serialize + Deserialize) | green | `crates/model/tests/evidence.rs` |
| C2 | R2: per-module `Register` and `validate` (unregistered key, wrong type, foreign prefix, unregistered block name) | green | `crates/model/tests/evidence.rs` |
| C3 | R3: `SectionOutcome::Measured.evidence`, `volatile()` path forms, cross-module reference check helper | green | `crates/model/tests/evidence.rs` |
| C4 | Value round trip, each of the six types tagged | green | `crates/model/tests/evidence.rs` |
| C5 | BlockId parse: `tls.chain@192.0.2.1:443` ok; `tls..chain`, `chain` refused | green | `crates/model/tests/evidence.rs` |
| C6 | validate: ok / unregistered key / type error | green | `crates/model/tests/evidence.rs` |
| C7 | validate: foreign module prefix refused | green | `crates/model/tests/evidence.rs` |
| C8 | reference check: cross-module ref resolves; dangling ref is an error naming it | green | `crates/model/tests/evidence.rs` |
| C9 | reader: BlockId canonical qualifier, strict ports | green | `crates/model/tests/evidence_rules.rs` |
| C10 | reader: register declares shape kind; duplicate ids, duplicate row keys and non-RFC-3339 times refused; tagged records; repeated keys allowed | green | `crates/model/tests/evidence_rules.rs` |
| C11 | reader: `check_evidence_rule` (judged checks need a resolving reference) | green | `crates/model/tests/evidence_rules.rs` |
| C12 | reader: `EvidencePath::parse` with row keys | green | `crates/engine/tests/evidence_path.rs` |

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 model + engine | 1 (GROUP_CMD lacked the gate's ip subset; the rest green) | opus | 64596 | 346 |
| reader round | 1 | opus (same agent) | — | — |

### Reader (phase 1)

| Class | Finding | Action |
|---|---|---|
| BLOCKER | A port qualifier accepted `+443`, `0443` and `0` | fixed (C9) |
| AMENDMENT | A qualifier had no canonical form | repaired (C9) |
| AMENDMENT | `Value::Time` was unchecked | repaired: strict RFC 3339 (C10) |
| AMENDMENT | Record fields were untagged | repaired (C10) |
| AMENDMENT | There was no list value for multi-valued keys | repaired: Fields and Chain may repeat a key, and a Table's key column is unique (C10). This keeps the six value types closed. |
| AMENDMENT | Duplicate block ids were not refused | repaired (C10) |
| AMENDMENT | The register declared no shape | repaired (C10) |
| AMENDMENT | References were not checked against status | repaired (C11) |
| AMENDMENT | No test covered the non-Fields `validate` branches | repaired (C10) |
| DEFERRED | `EvidencePath` had no parser | repaired (C12); module paths come in Phases 2–5 |
| DEFERRED | Lens drops evidence | Phase 6 |

### Behavioural verification

skipped: Phase 1 adds model and engine types only; no entry point changes behaviour, and every module emits `evidence: vec![]`.
