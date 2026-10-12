# Report: v2-unmeasured

## Phase 1 — Engine, lens, frontend

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: a weighted `unmeasured` check makes its section incomplete and the run letterless; N/A and not_tested unchanged | green | `crates/engine/tests/unmeasured_scoring.rs` |
| C2 | R2: lens carries `Unmeasured` to scoring and wire verdict `"unmeasured"` (SSE, snapshots, MCP); stored `"skip"` reads as before; `site/api/lens.html` lists the value | green | `crates/lens/tests/unmeasured_verdict.rs` |
| C3 | R3: frontend shows "could not check" for `unmeasured`; incomplete note appears for an unmeasured-incomplete section | green | `crates/lens/frontend/src/components/Unmeasured.test.tsx` |
| C4 | spf pass + dmarc unmeasured → section incomplete, grade incomplete | green | `crates/engine/tests/unmeasured_scoring.rs` |
| C5 | spf pass + dmarc not_applicable → complete, possible = spf weight, letter | already_implemented | `crates/engine/tests/unmeasured_scoring.rs` |
| C6 | spf pass + dmarc not_tested → complete | already_implemented | `crates/engine/tests/unmeasured_scoring.rs` |
| C7 | engine run with email.dnsbl unmeasured → SSE verdict "unmeasured", grade incomplete | green | `crates/lens/tests/unmeasured_verdict.rs` |
| C8 | same with email.dnsbl not_applicable → verdict "skip" | already_implemented | `crates/lens/tests/unmeasured_verdict.rs` |
| C9 | stored snapshot with "skip" loads unchanged | already_implemented | `crates/lens/tests/unmeasured_verdict.rs` |
| C10 | frontend: verdict unmeasured → "could not check" + incomplete note | green | `crates/lens/frontend/src/components/Unmeasured.test.tsx` |
| C11 | frontend: verdict skip → "–", no incomplete note on its account | already_implemented | `crates/lens/frontend/src/components/Unmeasured.test.tsx` |
### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 engine scoring | 1 | sonnet | 23093 | 26 |
| G2 lens wire | 1 (the tls step of GROUP_CMD lacked `--features testing`; rerun by me green) | sonnet | 37566 | 67 |
| G3 frontend | 1 | sonnet | 25776 | 28 |

Test-side fixes by the orchestrator: the C2/C7 stub emitted an unweighted `email.email_authentication`, so the C8 row could never get a letter; it now emits `email.spf` (b61bed4).

### Reader (phase 1)

| Class | Where | Finding | Action |
|---|---|---|---|
| BLOCKER | `crates/lens/frontend/src/components/Summary.tsx:196` | Every incomplete grade gets the headline "Grade unavailable — all backends failed", even when every backend answered | fixed (C14) |
| BLOCKER | `crates/lens/src/routes.rs:840` | A section made incomplete by an unmeasured check is "pass" in its event and "error" in `summary.sections` | fixed: new section status "incomplete" (operator, C13) |
| BLOCKER | `crates/lens/frontend/src/components/SectionHeadline.tsx:27` | The card header draws an unmeasured check as "–" | fixed (C15) |
| AMENDMENT | `crates/lens/frontend/src/components/ValidationChips.tsx:14` | The chip row does not count unmeasured checks | repaired in phase 1 (C16) |
| AMENDMENT | `crates/engine/src/scoring.rs:130` | Incomplete outranks a measured hard fail | operator 2026-10-12: a measured hard fail gives F even on an incomplete run; repaired in phase 1 (C12) |
| AMENDMENT | `crates/lens/README.md:235` | The SCORING SYNC RULE (`crates/lens/CLAUDE.md:5`) wants the README Scoring section changed in the same commit as the engine rule; requirement 17 deferred it | repaired in phase 1 |
| AMENDMENT | `site/api/lens.html:182` | The page documents neither grade `incomplete`, `summary.complete`, nor an incomplete section status | repaired in phase 1 |
| NIT | `crates/lens/src/routes.rs:189`, `crates/lens/src/score.rs:62` | The `complete` docs list only errored and possible-0 sections | fixed |
| NIT | `site/api/lens.html:181` | `skip` also carries `not_tested` | fixed |
| NIT | `crates/lens/src/routes.rs:1433` | The snapshot "incomplete" branch is unreachable under v3, because incomplete runs are not snapshotted | kept: it labels an unprofiled section correctly |

Further criteria from the reading (operator decisions of 2026-10-12):

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C12 | A measured hard fail gives F even when the run is incomplete; without one, incomplete stays letterless | green | `crates/engine/tests/hard_fail_precedence.rs` |
| C13 | A section made incomplete by a weighted unmeasured check is "incomplete" in its event and in `summary.sections` | green | `crates/lens/tests/unmeasured_section_status.rs` |
| C14 | The summary for grade incomplete with answering backends does not say "all backends failed" | green | `crates/lens/frontend/src/components/UnmeasuredSummary.test.tsx` |
| C15 | The section headline draws an unmeasured check with "?" | green | `crates/lens/frontend/src/components/UnmeasuredSummary.test.tsx` |
| C16 | The chip row counts unmeasured checks ("could not check") | green | `crates/lens/frontend/src/components/UnmeasuredSummary.test.tsx` |
| C17 | A section value "incomplete" is not rendered as "err" | green | `crates/lens/frontend/src/components/UnmeasuredSummary.test.tsx` |
### Runs (reader round)

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| C12 engine precedence | 0 (orchestrator, `scoring.rs` grade order) | orchestrator | 0 | 0 |
| C13 lens section status + docs | 2 | sonnet | 46624 | 133 |
| C14–C17 frontend | 2 | sonnet | 31197 | 38 |

### Behavioural verification

skipped: no module emits `unmeasured` before Phase 2. The SSE route is covered in process by `crates/lens/tests/unmeasured_verdict.rs` and `unmeasured_section_status.rs`, which POST to `/api/check` and read the `email` and `summary` events.

### Deferred

- Badge and OG responses for an F on an incomplete run keep the long `Cache-Control` (3600 s): `is_unscored_grade` matches only `error` and `incomplete`. The server-side cache and snapshots refuse incomplete results.
- A grade `incomplete` with an empty `sections` map still shows "all backends failed". `sdd_lens-badges_p2_c4_summary-badge-btn.test.tsx` pins this, and the backend does not send that combination.
- `crates/lens/README.md` changed in the phase commit, not in the prose commit: the SCORING SYNC RULE (`crates/lens/CLAUDE.md:5`) requires it.

## Phase 2 — Facts, S31, DNS

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R4: workspace on mhost 0.12.1; `Facts` records for A and AAAA whether the negative answer is definite (every resolver NXDOMAIN or non-referral NODATA) | green | `crates/dns/tests/facts_definite.rs` |
| C2 | R5: A and AAAA both definite negative → IP, HTTP, TLS `not_applicable` "no A or AAAA record", run complete with a letter; any non-definite answer keeps them incomplete | green | `crates/engine/tests/no_address_sections.rs` |
| C3 | R6: DNS lookup error or stream timeout → `unmeasured`, never fail via NotFound or "not reported"; ns_lame/ns_delegation judge answering nameservers, a timed-out NS is excluded with a finding, none answering or NS addresses unresolvable by resolver error → unmeasured, NS host NXDOMAIN stays a defect; ns_lame catalogue finding drops "or reached" | green | `crates/dns/tests/lookup_errors_unmeasured.rs, ns_timeouts.rs` |
| C4 | R7: DNS contract table, worst measured + unmeasured row per check, guard on the 10 IDs | green | `crates/dns/tests/contract_status.rs` |
| C5 | A and AAAA both NODATA from every resolver → IP/HTTP/TLS N/A "no A or AAAA record", letter from DNS + email | green | `crates/engine/tests/no_address_sections.rs` |
| C6 | A NODATA + AAAA SERVFAIL → incomplete | green | `crates/engine/tests/no_address_sections.rs` |
| C7 | A NXDOMAIN + AAAA timeout → incomplete | green | `crates/engine/tests/no_address_sections.rs` |
| C8 | A NODATA from one resolver, referral from another → incomplete | green | `crates/engine/tests/no_address_sections.rs` |
| C9 | DNSKEY lookup SERVFAIL → `dns.dnskey_algorithm` unmeasured with a finding naming the lookup | green | `crates/dns/tests/lookup_errors_unmeasured.rs` |
| C10 | definite empty DNSKEY answer → `dns.dnskey_algorithm` not_applicable | already_implemented | `crates/dns/tests/lookup_errors_unmeasured.rs` |
| C11 | two NS, one answers SOA with authority, one times out → `dns.ns_lame` pass with a finding naming the silent NS | green | `crates/dns/tests/ns_timeouts.rs` |
| C12 | two NS, both time out → `dns.ns_lame` unmeasured | green | `crates/dns/tests/ns_timeouts.rs` |
| C13 | DNS contract guard: table IDs equal the 10 DNS catalogue IDs | green | `crates/dns/tests/contract_status.rs` |
| C14 | reader: `dns.https_svcb` reads HTTPS and SVCB; SVCB SERVFAIL → unmeasured | green | `crates/dns/tests/unmeasured_inputs.rs` |
| C15 | reader: `dns.cname_apex` reads CNAME and SOA, not A | green | `crates/dns/tests/unmeasured_inputs.rs` |
| C16 | reader: every NS RawError counts as "did not answer" | green | `crates/dns/tests/unmeasured_inputs.rs` |
| C17 | reader: a policy-refused NS keeps its measured Warn next to a silent one | green | `crates/dns/tests/unmeasured_inputs.rs` |
| C18 | reader: lens rejects `resolve_timeout_ms` ≥ `[backends.http] timeout_ms` | green | `crates/lens/tests/lens_config_http_window.rs` |

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 a–c (facts, S31, goldens) | 1 | sonnet | 56736 | 130 |
| G1 d–e (translate, NS) | 2 | opus | 114938 | 346 |
| reader round (C14–C18) | 1 | opus (same agent) | 139864 | 148 |
| G2 contract table | 0 (green with G1) | — | 0 | 0 |

### Reader (phase 2)

| Class | Where | Finding | Action |
|---|---|---|---|
| BLOCKER | `crates/dns/src/module.rs:370` | `https_svcb` lacked SVCB as an input | fixed (C14) |
| BLOCKER | `crates/dns/src/module.rs:366` | `cname_apex` was tied to A, not to CNAME and SOA | fixed (C15) |
| BLOCKER | `crates/dns/src/api/check.rs:1069` | Only a timeout counted as silent, so other RawErrors stayed a measured Warn | fixed (C16) |
| BLOCKER | `crates/lens/src/config.rs:530` | HTTP now waits for the resolve, but the config did not check the budget against the HTTP window | fixed (C18); the example and dev config comments name all three windows |
| AMENDMENT | `crates/dns/src/module.rs:441` | A policy-refused NS next to a silent one lost its measured Warn | repaired in phase 2 (C17) |
| AMENDMENT | `crates/dns/src/api/check.rs:1120` | An NXDOMAIN NS host next to working ones is dropped without a finding | DEFERRED: this behaviour predates the feature; reporting it would be a new warn branch and a grade change |

Decisions in the coder's slices:
- `ttl` counts only the base record-type batches. The `_dmarc`-style TXT batches do not count, because the ttl lint reads only the base lookups.
- A check is unmeasured when any of its inputs errored; for example, `dnssec` is unmeasured on a DS SERVFAIL.
- `WireResult::NotFound` decodes both `{"NotFound":[]}` and the unit form. The V1 wire is unchanged.

### Behavioural verification

The binary was built from the branch (`cargo build -p netray`) and run as `netray lens lens.dev.toml` from `~/Documents/src/netray/target/debug/netray`. A binary started from a new path gets no outbound network on this machine, so every section times out; see `reference-new-binary-no-network` in the session memory.

```
$ curl -sN http://127.0.0.1:18086/api/check/netray.info | grep -A1 'event: summary'
{"sections":{"http":"warn","dns":"pass","tls":"warn","email":"pass"},"section_grades":{"dns":"A+","http":"A","email":"A+","tls":"A+"},"overall":"warn","grade":"A+","complete":true,"score":97.5,...}
$ curl -sN http://127.0.0.1:18086/api/check/example.com | grep -A1 'event: summary'
{"sections":{"dns":"warn","email":"warn","http":"fail","tls":"warn"},...,"grade":"B","complete":true,"score":79.0,...}
```

netray.info gets the same grade as the main build, A+ 97.5. The real resolve path produces no spurious `unmeasured`.


## Phase 3 — TLS

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R8: `tls_reachable` pass when any address handshakes, named timeouts as findings; fail when none did and one answered (refused, reset, alert); unmeasured when every tested address timed out; not-tested addresses excluded, all not-tested stays `not_tested` | green | `crates/tls/tests/reachable_unmeasured.rs` |
| C2 | R9: other per-address checks are worst-of over handshaken addresses; with none they follow `tls_reachable` (N/A behind fail, unmeasured behind unmeasured, not_tested behind not_tested), taking precedence over R10 | green | `crates/tls/tests/reachable_unmeasured.rs` |
| C3 | R10: CAA, HTTPS-record or TLSA lookup error → `caa_compliant`, `ech_advertised`, `dane_valid` unmeasured; consistency/alpn_consistency unmeasured when fewer than two handshakes because others timed out; N/A for fewer than two addresses or refused/alerted others | green | `crates/tls/tests/lookup_errors_unmeasured.rs` |
| C4 | R11: TLS contract table, worst measured + unmeasured row per check, guard on the 21 IDs | green | `crates/tls/tests/contract_status.rs` |
| C5 | two addresses both time out on 443 → `tls_reachable` unmeasured, the 20 others unmeasured | green | `crates/tls/tests/reachable_unmeasured.rs` |
| C6 | two addresses both refuse 443 → `tls_reachable` fail, the 20 others N/A "no TLS handshake" | already_implemented | `crates/tls/tests/reachable_unmeasured.rs` |
| C7 | one times out, one handshakes valid → `tls_reachable` pass with a finding naming the timed-out address | green | `crates/tls/tests/reachable_unmeasured.rs` |
| C8 | one refuses, one handshakes valid → `tls_reachable` pass (unchanged) | already_implemented | `crates/tls/tests/reachable_unmeasured.rs` |
| C9 | one refuses, one times out → `tls_reachable` fail, the other 20 incl. consistency N/A "no TLS handshake" | already_implemented | `crates/tls/tests/reachable_unmeasured.rs` |
| C10 | one not tested from here, one times out → `tls_reachable` unmeasured | green | `crates/tls/tests/reachable_unmeasured.rs` |
| C11 | every address not tested from here → `tls_reachable` stays `not_tested` | already_implemented | `crates/tls/tests/reachable_unmeasured.rs` |
| C12 | CAA lookup SERVFAIL → `caa_compliant` unmeasured, not pass | green | `crates/tls/tests/lookup_errors_unmeasured.rs` |
| C13 | definite empty CAA answer → `caa_compliant` pass | already_implemented | `crates/tls/tests/lookup_errors_unmeasured.rs` |
| C14 | one address → `consistency` N/A | already_implemented | `crates/tls/tests/lookup_errors_unmeasured.rs` |
| C15 | two addresses, one handshook, one timed out → `consistency` unmeasured | green | `crates/tls/tests/lookup_errors_unmeasured.rs` |
| C16 | two addresses, one handshook, one refused → `consistency` N/A | already_implemented | `crates/tls/tests/lookup_errors_unmeasured.rs` |
| C17 | TLS contract guard: table IDs equal the 21 TLS catalogue IDs | already_implemented | `crates/tls/tests/contract_status.rs` |
### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 TLS | 2 (green on run 1, run 2 confirmed rustfmt) | opus | 96369 | 299 |

Opus was chosen up front: the group is a single one that spans `translate`, three DNS lookups and the error-code mapping.

### Reader (phase 3)

| Class | Where | Finding | Action |
|---|---|---|---|
| NIT | `crates/tls/src/dns/tlsa.rs:43` | A TLSA owner name over 255 octets cannot be built, so the check reads "TLSA lookup failed" and is unmeasured where it should be N/A | listed |
| NIT | `crates/tls/src/module.rs:273` | The unmeasured finding names the hostname, not the CAA tree level or `_443._tcp` name that failed | listed |
| DEFERRED | `crates/tls/src/routes.rs:736` | V1 `/api/inspect` still reports CAA "pass" on a failed lookup, now next to `dns.lookup_errors: ["caa"]`; this was wrong before too | V1 defect, out of scope (plan) |

The reader traced and found sound: the error codes (only tokio's timer yields `TimedOut`), `answered()` with CNAME chains and the CAA climb, `reachable()` for C5–C11, the per-address worst-of, the consistency rule, the meaning of the lens goldens, and the HSTS probe.

Coder note: an empty address list now gives `NotTested` instead of `NotApplicable`. The module path cannot reach that case, because the section is already Incomplete when no address resolves.

### Behavioural verification

```
$ cargo build -p netray   # branch build, run from ~/Documents/src/netray/target/debug/netray
$ curl -sN http://127.0.0.1:18086/api/check/netray.info | grep -A1 'event: summary'
{"sections":{"http":"warn","email":"pass","tls":"warn","dns":"pass"},"section_grades":{"email":"A+","tls":"A+","http":"A","dns":"A+"},"grade":"A+","complete":true,"score":97.5,...}
tls section: status warn, verdicts {pass, skip, warn}
```

The grade matches main, A+ 97.5. No TLS check is spuriously unmeasured on a healthy domain.


## Phase 4 — HTTP

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R12: port-80 timeout → `https_redirect` unmeasured; refused, reset or a non-redirect response stays fail; HTTPS probe timeout or a policy-cut redirect → the ten response checks unmeasured naming the timeout or blocked hop; HTTPS refused/reset/alert → the ten fail "no HTTPS response" (A5); `cors` follows its own probe | green | `crates/http/tests/probe_outcomes.rs` |
| C2 | R13: HTTP contract table, worst measured + unmeasured row per check, guard on the 12 IDs | green | `crates/http/tests/contract_status.rs` |
| C3 | port-80 probe times out → `https_redirect` unmeasured with a finding naming port 80 | green | `crates/http/tests/probe_outcomes.rs` |
| C4 | port-80 connection refused → `https_redirect` fail | already_implemented | `crates/http/tests/probe_outcomes.rs` |
| C5 | port-80 reset after accept → `https_redirect` fail | already_implemented | `crates/http/tests/probe_outcomes.rs` |
| C6 | port-80 200 without redirect → `https_redirect` fail | already_implemented | `crates/http/tests/probe_outcomes.rs` |
| C7 | HTTPS probe times out, CORS probe answers `ACAO: *` → ten response checks unmeasured, `cors` warn | green | `crates/http/tests/probe_outcomes.rs` |
| C8 | HTTPS 200, CORS probe times out → `cors` unmeasured, response checks measured | green | `crates/http/tests/probe_outcomes.rs` |
| C9 | HTTPS 301 to an address the fetch policy blocks → ten response checks unmeasured naming the blocked hop | green | `crates/http/tests/probe_outcomes.rs` |
| C10 | HTTPS response without CSP → `csp` warn (unchanged) | already_implemented | `crates/http/tests/probe_outcomes.rs` |
| C11 | HTTPS response without HSTS → `hsts` fail | already_implemented | `crates/http/tests/probe_outcomes.rs` |
| C12 | HTTPS connection refused → response checks fail "no HTTPS response" (A5; was N/A) | green | `crates/http/tests/probe_outcomes.rs` |
| C13 | HTTP contract guard: IDs equal the 12 HTTP catalogue IDs | already_implemented | `crates/http/tests/contract_status.rs` |
| C14 | reader: port 80 redirected to https, then a later hop timed out → `https_redirect` stays pass with the message as a finding | green | `crates/http/tests/probe_outcomes_landing.rs` |
| C15 | reader + A5: HTTPS refused, reset or tls → the ten response checks fail "no HTTPS response"; a refused CORS probe stays N/A | green | `crates/http/tests/probe_outcomes_landing.rs` |
| C16 | HTTPS timeout still makes the ten unmeasured (pin) | already_implemented | `crates/http/tests/probe_outcomes_landing.rs` |

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 HTTP | 2 | opus | 91676 | 320 |
| reader round (C14, C15, NoAddress, connect fallback) | 1 | opus (same agent) | 108733 | 95 |

### Reader (phase 4)

| Class | Where | Finding | Action |
|---|---|---|---|
| AMENDMENT | `crates/http/src/module.rs:165` | Port 80 redirected to https and a later hop timed out, yet the check reported unmeasured | repaired (C14) |
| AMENDMENT | `crates/http/src/module.rs:179` | A refused redirect target made the response checks N/A, so nothing graded it | operator: fail (A5, C15) |
| AMENDMENT | `crates/http/src/module.rs:178` | A refused HTTPS on the address HTTP probes was N/A while TLS passed on another address | operator: fail (A5, C15) |
| AMENDMENT | `crates/http/src/inspect/request.rs:127` | An unresolvable redirect target was classed `blocked` and so unmeasured | repaired: kind `refused`, "Redirect destination does not resolve" |
| AMENDMENT | `crates/http/src/inspect/request.rs:160` | A local connect error (EMFILE, EPERM) fell back to `reset`, an answer from the target | repaired: an unrecognised connect error is `unreachable`; in-src unit test `unexplained_connect_error_is_unreachable` |
| NIT | `crates/http/src/module.rs:175` | An `unreachable` finding carries only reqwest's top-level text | listed |
| NIT | `crates/http/src/module.rs:163` | An unanswered port 80 drops the `redirect_limit` warning | listed |

DEFERRED from the plan:
- The V1 headline and V1 `quality.checks` are unchanged, so an unmeasured `hsts` still reads "HSTS ✗" in the transitional presentation.
- A port-80 error leaves `status_code: Some(0)` in V1 `http_upgrade`.

### Behavioural verification

```
$ curl -sN http://127.0.0.1:18086/api/check/netray.info   # branch build, from main's binary path
summary: {"sections":{"tls":"warn","http":"warn","dns":"pass","email":"pass"},"section_grades":{"http":"A",...},"grade":"A+","complete":true,"score":97.5,...}
http: https_redirect pass, hsts pass, cors warn, cookie_secure skip, csp warn, x_frame_options pass, x_content_type_options pass, referrer_policy pass, permissions_policy pass, deprecated_headers pass, info_leakage pass, caching pass
```

The grade matches main, A+ 97.5. On a healthy domain no HTTP check comes out unmeasured.

