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

## Phase 2 — DNS

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R3a: the resolve stage projects `dns.a`, `aaaa`, `mx`, `caa`, `ns`, `https` (records, with TTL) into Facts; the DNS section publishes them even when the DNS module timed out; DNS module does not project those six; an errored lookup gives no block | green | `crates/dns/tests/facts_blocks.rs` |
| C2 | R4 (dns): DNS emits `dns.<type>` records blocks for the other base-name types; measured checks reference their input blocks (CHECK_INPUTS + facts blocks; `dns.ttl` every records block); the blocks validate against the DNS register | green | `crates/dns/tests/evidence.rs` |
| C3 | R5 (dns): DNS volatile paths (`dns.<type>/ttl` per records block) name registered keys | green | `crates/dns/tests/evidence.rs` |
| C4 | DNS fixtures with CAA (and NS/CAA for no-address) batches added: blocks validate; `dns.caa` references `dns.caa` | green | `crates/dns/tests/evidence.rs` |
| C5 | Facts CAA answered + DNS section timed out → `dns.caa` still published; `tls.caa_compliant` reference resolves | green | `crates/engine/tests/facts_blocks.rs` |
| C6 | CAA batch errored → `dns.caa` unmeasured, no `dns.caa` block claims records | green | `crates/dns/tests/evidence.rs` |
| C7 | DNS volatile list: every `dns.<type>/ttl` path registered | green | `crates/dns/tests/evidence.rs` |
| C8 | reader: records keep the highest TTL across resolvers | green | `crates/dns/tests/evidence_records.rs` |
| C9 | reader: records in canonical order (owner, type, data) | green | `crates/dns/tests/evidence_records.rs` |
| C10 | reader: `dns.dnssec` also references RRSIG/NSEC/NSEC3 blocks | green | `crates/dns/tests/evidence_records.rs` |
| C11 | operator: a section's block wins over the facts block of the same id | green | `crates/engine/tests/facts_blocks_section_wins.rs` |

### Amendments (operator, 2026-10-13)

- **A1, replacing requirement 3a's "the DNS module does not project those six types".** The DNS module projects every one of its 15 answered types itself and stays independent of the facts, so it no longer waits on the resolve stage. Requirement 3a's earlier version cost up to the resolve budget in latency and turned a DNS check unmeasured when only the facts lookup failed. The facts still carry `dns.a`, `aaaa`, `mx`, `caa`, `ns` and `https`.
- **A2.** When the same id exists in both the facts and a section, the section's block wins: a DNS check points at what it judged. The facts fill in only ids that no section carries, as after a DNS timeout. A first version let the facts win; the reader showed that a passing DNS check could then point at another resolver's empty answer.
- **Fixtures.** `prism.sse` gains a CAA batch, and `prism-no-address.sse` gains NS and CAA NODATA batches, as the Phase 2 scenario asks. Nine lens-full goldens change only the `dns.caa` lint message; verdicts and grades are unchanged.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 DNS + facts + engine | 1 + 1 + 1 + 3 + 1 (five rounds: plan, test conflicts, the A1 rework, fixtures, the reading) | opus | 179232 (cumulative, one agent) | 1211 |

### Reader (phase 2)

| Class | Finding | Action |
|---|---|---|
| BLOCKER | The first TTL across resolvers was kept, against `dns.ttl`'s highest-copy rule | fixed (C8) |
| AMENDMENT | `dns.dnssec` evidence lacked RRSIG, NSEC and NSEC3 | repaired through `EVIDENCE_ONLY_INPUTS` (C10); CHECK_INPUTS is unchanged |
| AMENDMENT | With facts winning, a DNS check could point at records it never judged | operator: the section wins (A2, C11) |
| DEFERRED | Record order was not canonical | repaired (C9) |
| NIT | TXT quoting does not escape `"` and `\` | listed |
| NIT | SVCB `no-default-alpn` renders with a trailing `=` | listed |
| NIT | NSEC3 hash algorithm shows a name, not its number | listed |
| NIT | A "not reported" N/A `dns.ttl` still references every block | listed |

### Behavioural verification

skipped: lens drops evidence until Phase 6, so no entry point shows the blocks yet. The lens goldens run the DNS module through the gate and are byte-identical apart from the CAA lint message caused by the fixture change.


## Phase 3 — TLS

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R4 (tls): TLS emits `tls.addresses`, `tls.chain@a:p`, `tls.handshake@a:p`, `tls.tlsa@p`, `tls.ct`; judged checks reference per the TLS row; blocks validate against the TLS register | green | `crates/tls/tests/evidence.rs` |
| C2 | R5 (tls): TLS volatile paths (`tls.chain@…/days_remaining`, `tls.handshake@…/handshake_ms`) name registered keys | green | `crates/tls/tests/evidence.rs` |
| C3 | tlsight-inspect.json: chain and handshake blocks validate; `not_expired` references the worst address's chain | green | `crates/tls/tests/evidence.rs` |
| C4 | `caa_compliant` references `dns.caa` (facts) — or, when the facts lack it, TLS's own `tls.caa` block | green | `crates/tls/tests/evidence.rs` |
| C5 | timed-out address: no chain/handshake block; `tls.addresses` row with outcome HANDSHAKE_TIMEOUT | green | `crates/tls/tests/evidence.rs` |
| C6 | tlsight-unreachable.json: `tls_reachable` fail references `tls.addresses` | green | `crates/tls/tests/evidence.rs` |
| C7 | reader: `caa_compliant` references `dns.caa` only when TLS judged from the facts' CAA; otherwise `tls.caa` or the chains | green | `crates/tls/tests/evidence_refs.rs` |
| C8 | reader: no `ech` key in handshake blocks after a failed HTTPS lookup | green | `crates/tls/tests/evidence_refs.rs` |
| C9 | reader: `consistency` also references the chains | green | `crates/tls/tests/evidence_refs.rs` |
| C10 | reader: chain items carry `ca_issuers_url` | green | `crates/tls/tests/evidence_refs.rs` |
| C11 | reader: `tls.tlsa/ttl` and `tls.caa/ttl` volatile | green | `crates/tls/tests/evidence_refs.rs` |
| C12 | reference kind for every judged per-address check | already_implemented | `crates/tls/tests/evidence_refs.rs` |
| C13 | reader: no placeholder `days_remaining`, `key` without size 0 | green | `crates/tls/tests/evidence_refs.rs` |

### Amendments

- **A3.** `caa_compliant` references `dns.caa` when TLS judged from the facts' CAA. Otherwise it references TLS's own `tls.caa` (projected from its climb, with the real owner, TTL and flags), else the chain blocks. `dane_valid` references `tls.tlsa@<port>`, else the chains. Blocks cover the judged port only.
- Records from JSON carry no TTL or owner, so a JSON-replayed response emits no `tls.tlsa` or `tls.caa` with records; nothing is invented.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 TLS | 2 + 1 + 2 + 2 (four rounds: build, real TTLs, CAA literals, the reading) | opus | 181185 (one agent) | 1627 |

### Reader (phase 3)

| Class | Finding | Action |
|---|---|---|
| BLOCKER | `caa_compliant` pointed at an empty `dns.caa` while parent-level records decided | fixed (C7, `CaaSource`) |
| BLOCKER | A failed HTTPS lookup was published as `ech: false` | fixed (C8) |
| AMENDMENT | `consistency` lacked the chains | repaired (C9) |
| AMENDMENT | Chain items lacked the AIA URL | repaired (C10) |
| AMENDMENT | The TLSA and CAA TTLs were not volatile | repaired (C11) |
| DEFERRED | `days_remaining` carried a placeholder; `key` showed "0" | repaired (C13) |
| DEFERRED | The record projection was untested | in-src unit tests in `crates/tls/src/dns/mod.rs` |
| DEFERRED | Only 3 checks' references were asserted | repaired (C12) |

### Behavioural verification

skipped: lens drops evidence until Phase 6. The tlsight V1 wire and every golden are byte-identical (the fixture-diff check is green).

