# Report: v2-check-fixes

## Phase 1 — Independent check fixes

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: ACAO equal to the probe Origin (`PROBE_ORIGIN`) with ACAC true → reflects, Fail | green | crates/http/tests/cors_probe.rs |
| C2 | R1: ACAO `null` with ACAC true → reflects, Fail | green | crates/http/tests/cors_probe.rs |
| C3 | R1: ACAO `HTTPS://EVIL.EXAMPLE.COM` (case differs) with ACAC true → not reflected, Pass | green | crates/http/tests/cors_probe.rs |
| C4 | R1: ACAO `https://app.example.com` with ACAC true → not reflected, Pass "CORS configured" | green | crates/http/tests/cors_probe.rs |
| C5 | R1: ACAO `*` without credentials → Warn (unchanged) | green | crates/http/tests/cors_probe.rs |
| C6 | R1: `PROBE_ORIGIN` is the Origin the probe sends | green | crates/http/tests/cors_probe.rs |
| C7 | R2: critical record with unknown tag `foo` → Fail | already_implemented | crates/tls/tests/caa_critical.rs |
| C8 | R2: critical `issue` authorising the leaf's CA → Pass | green | crates/tls/tests/caa_critical.rs |
| C9 | R2: critical `issue` not authorising the CA → Fail | already_implemented | crates/tls/tests/caa_critical.rs |
| C10 | R2: critical `IODEF` / `contactemail` next to an authorising `issue` → Pass | green | crates/tls/tests/caa_critical.rs |
| C11 | R3: the six OIDs map to md5WithRSAEncryption, md2WithRSAEncryption, ecdsa-with-SHA1, dsa-with-SHA1, sha1WithRSASignature, Ed448 | green | crates/tls/tests/signature_verdict.rs |
| C12 | R4: SHA-256 leaf + intermediate, SHA-1 self-signed root → Pass | green | crates/tls/tests/signature_verdict.rs |
| C13 | R4: intermediate ecdsa-with-SHA1 → Fail | green | crates/tls/tests/signature_verdict.rs |
| C14 | R4: intermediate dsa-with-SHA1 → Fail | green | crates/tls/tests/signature_verdict.rs |
| C15 | R4: self-signed leaf with SHA-1 → Fail | green | crates/tls/tests/signature_verdict.rs |
| C16 | R4: leaf `Unknown(1.2.840.113549.1.1.10)` → Warn `unrecognised signature algorithm 1.2.840.113549.1.1.10` | green | crates/tls/tests/signature_verdict.rs |
| C17 | R4: leaf `Unknown(...)` + intermediate SHA-1 → Fail | green | crates/tls/tests/signature_verdict.rs |
| C18 | R4: signature algorithm "" → Warn `unreadable certificate signature` | green | crates/tls/tests/signature_verdict.rs |
| C19 | R4: catalogue `tls.strong_signature` finding names MD2 and unrecognised algorithms | green | crates/engine/tests/strong_signature_finding.rs |
| C20 | R4: intermediate md2WithRSAEncryption → Fail | already_implemented | crates/tls/tests/signature_verdict.rs |
| C21 | R1: catalogue `http.cors` finding names `null` | green | crates/engine/tests/strong_signature_finding.rs |
| C22 | R7: wildcard leaf, `issuewild ";"` → Fail | green | crates/tls/tests/caa_wildcard.rs |
| C23 | R7: issuewild names the CA, issue another → Pass; plus apex SAN → Fail | green | crates/tls/tests/caa_wildcard.rs |
| C24 | R7: wildcard leaf, no issuewild → issue decides, Pass | green | crates/tls/tests/caa_wildcard.rs |
| C25 | R7: non-wildcard leaf ignores `issuewild ";"` → Pass | green | crates/tls/tests/caa_wildcard.rs |

C7 and C9 pass today (Fail is already the outcome); their rows stay as protection.

C20 passed on arrival: the G3 code already ranked MD2 weak; the row closes the reader's NIT.
C20–C25 come from the phase-1 amendments (spec d2b3a5b); the orchestrator wrote their tests and code directly, the change being one function and four text lines.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 CORS | 1 | sonnet | 23951 | 47 |
| G2 CAA | 1 | sonnet | 27547 | 59 |
| G3 signatures | 1 | sonnet | 28299 | 41 |

The plan was written by the orchestrator, not a planner agent: three groups over disjoint files, named by the spec's file list.

### Reader

0 blockers. Findings, each verified against the code:

| class | where | finding | outcome |
|---|---|---|---|
| AMENDMENT | checks.toml `http.cors` | finding does not cover the new `null` Fail branch | repaired in phase 1 (R1 amended) |
| AMENDMENT | checks.toml `tls.strong_signature` | R4's finding text omits MD2, a Fail branch | repaired in phase 1 (R4 amended) |
| AMENDMENT | chain_trust.rs `weakest_signature` | the API field `validation.weakest_signature` now excludes root self-signatures | intended; recorded in R4 |
| AMENDMENT | tlsight `ValidationSummary.tsx`, `docs/quality.md`, lens `checkMeta.ts` | texts describe neither the Warn branch nor the root exclusion | repaired in phase 1 |
| DEFERRED | caa_compliance.rs | `issuewild` is never evaluated; the blanket critical Fail used to hide a critical `issuewild ";"` | taken into this feature as R7 (operator, 2026-10-11) |
| DEFERRED | cors.rs:13 | only the first ACAO header is compared; repeated ACAO headers make browsers refuse | listed |
| DEFERRED | cors.rs:21 | ACAC compared case-insensitively; Fetch requires exact `true` | listed (unchanged behaviour) |
| NIT | cors_probe.rs | C6 checks only the constant's value | listed |
| NIT | checks.rs:117 | no end-to-end MD2 row | covered by the R4 amendment's tests |

### Behavioural verification

skipped: the phase changes pure check functions with no entry point of its own; the end-to-end paths (`/api/inspect`, lens SSE) need live targets, and the unit and integration tests above drive the same functions the routes call (`routes.rs:820`, :868; `inspect/mod.rs:205`).

### Verify

`adlc verify`: contract passed, 101 tests collected (tree d05eb99).

## Phase 2 — Trust and chain completeness

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C26 | R5: chain building to an anchor, hostname the leaf does not cover → `chain_trusted` true, `leaf_covers_hostname` false | green | crates/tls/tests/trust_anchor.rs |
| C27 | R5: same chain, no hostname → `chain_trusted` true | green | crates/tls/tests/trust_anchor.rs |
| C28 | R5: self-signed leaf not in the store → `chain_trusted` false | green | crates/tls/tests/trust_anchor.rs |
| C29 | R6: lone leaf, issuer not an anchor → chain_complete Fail; aia Fail without, Warn with caIssuers URL | green | crates/tls/tests/chain_completeness.rs |
| C30 | R6: same lone leaf, expired → chain_complete Fail | green | crates/tls/tests/chain_completeness.rs |
| C31 | R6: lone leaf issued by an anchor → chain_complete Pass, aia Pass (module maps it to NotApplicable) | green | crates/tls/tests/chain_completeness.rs |
| C32 | R6: leaf + intermediate, intermediate issued by an anchor, leaf expired → chain_complete Pass | green | crates/tls/tests/chain_completeness.rs |
| C33 | R6: ordered chain ending in a self-signed root not in the store → chain_complete Pass | green | crates/tls/tests/chain_completeness.rs |
| C34 | R6: wrongly ordered trusted chain → chain_complete Warn | green | crates/tls/tests/chain_completeness.rs |
| C35 | R6: the stored fixtures' asserted statuses hold (`module_v3`, `port_results_table`, `contract_golden`) | green | existing tests |
| C36 | R6 (amended): cross-certificates served past the anchor do not fail completeness | green | crates/tls/tests/chain_completeness.rs |
| C37 | R6 (amended): a chain ending self-signed outside the store passes with "chain ends at a self-signed certificate" | green | crates/tls/tests/chain_completeness.rs |

C36 and C37 come from the phase-2 reading and were written with their fix, not committed red first.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G4 trust and completeness | 2 | sonnet | 45138 | 76 |

The coder's edit of the struct literals and the golden was blocked by the sandbox (`sed -i`); the orchestrator applied it and regenerated `tlsight-inspect.json` with `UPDATE_GOLDEN=1` (one added key, `ends_at_anchor`).

### Reader

| class | where | finding | outcome |
|---|---|---|---|
| BLOCKER | checks.rs `check_chain_complete` | Pass detail "chain reaches a trust anchor" for a chain ending self-signed outside the store | fixed: detail distinguishes the two; C37 |
| AMENDMENT | chain_trust.rs `chain_ends_at_anchor` | only the last served certificate was judged, so cross-certificates past the anchor failed a trusted chain | repaired in phase 2 (R6 amended); C36 |
| DEFERRED | checks.rs `check_aia_reachability` | the incomplete branch cites the first caIssuers URL, usually the leaf's, which points at the intermediate already served | listed; AIA rules are out of scope |
| DEFERRED | tlsight `IpCard.tsx:29` | IP targets: the card is red with "hostname mismatch" because `leaf_covers_hostname` is false without a hostname | listed; predates the change, R5 exposes it |
| NIT | tlsight `ValidationSummary.tsx` | `chain_complete` tooltip described ordering only | fixed |
| NIT | chain_trust.rs `der_contents` | long-form DER header untested | fixed: unit test `der_contents_strips_short_and_long_form_headers` |

### Behavioural verification

skipped: no live target in the offline gate; `validate_chain` is driven end to end on real rcgen chains by `trust_anchor.rs` and `chain_completeness.rs`, the same call `routes.rs` makes.
| C38 | R8: v3 hard fails are chain_trusted, hostname_match, not_expired, tls_reachable | green | crates/engine/tests/profile.rs |
| C39 | R8: every hard-fail check is `critical` in the catalogue | already_implemented (guard; held for the three before R8) | crates/engine/tests/profile.rs |

C38 and C39 come from the branch reading (MAJOR: a name mismatch no longer reached a hard fail once R5 took the name out of `chain_trusted`). Profile v3 is unreleased; its lock hash moves with the one-line change. Lens goldens are unchanged: every one passes `hostname_match`.
