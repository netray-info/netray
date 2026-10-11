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
| C19 | R4: catalogue `tls.strong_signature` finding names unrecognised algorithms | green | crates/engine/tests/strong_signature_finding.rs |

C7 and C9 pass today (Fail is already the outcome); their rows stay as protection.

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
