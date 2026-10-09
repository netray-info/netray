# Spec: backend correctness

Status: Ready for Implementation
Created: 2026-10-09

## Goal

prism's `+check` keeps its explicit RRSIG question for 0.23.0 and counts every record once, however many resolvers answered. lens scores IP reputation from ifconfig-rs's boolean flags, makes the section incomplete when an enrichment fails, and samples addresses deterministically across both families. Lint and check messages, which carry DNS data an attacker controls, are rendered as text everywhere: both frontends, the snapshot page and the Markdown export. prism's UI offers `@system` only when the server allows it, and the production fixture turns it off.

## Non-goals

- prism's raw nameserver queries under the target policy (SDD R5.8, its own feature, landed at the release).
- mhost lint semantics beyond deduplicated input (mhost-security session).
- Any UI redesign (V2 Phase 4).

## Context and constraints

Line numbers at `8eb895a`.

- prism `+check` asks for `RRSIG` as an explicit qtype (`crates/mhost-prism/src/api/check.rs:69`, one of `CHECK_RECORD_TYPES` `:57-73`, `CHECK_TOTAL_STEPS = 19` `:76`) with `dnssec: false` (`:168`); lint input is `all_lookups`, every resolver's lookups merged without deduplication (`:353`, `:401`); lint results are emitted verbatim per category (`:506-535`). `crates/mhost-prism/tests/lint_results_table.rs` pins "Found 2 KSK(s) and 2 ZSK(s)" for one zone answered by two resolvers (`:135-144`).
- lens dns turns each lint category into one check with the worst verdict and pushes every Warn/Fail/NotFound message (`crates/lens/src/backends/dns.rs:290-366`); `dnssec` weighs 5 (`crates/lens/profiles/default.toml:10`).
- lens ip (`crates/lens/src/backends/ip.rs`): `MAX_IPS = 5` taken in arrival order (`:114`, `:132`; order from `dns.rs:234-277`); a failed enrichment is pushed as `unknown` and adds nothing (`:202-210`); the verdict comes from `network.type` only (`:246-260`), whose priority in ifconfig-rs puts cloud, bot and vpn before tor and spamhaus (`crates/ifconfig-rs/src/backend/mod.rs:601-626`); `/range` classifies in another order without internal, bot or tor (`crates/ifconfig-rs/src/routes.rs:1816-1828`). The ifconfig golden carries `is_spamhaus`, `is_c2`, `is_tor`, `is_vpn` (`tests/fixtures/contracts/ifconfig-json.json`).
- No raw-HTML sink exists in `crates/*/frontend/src` or `packages/common-frontend/src`; prism's `LintTab.tsx:179` and lens's `CheckList.tsx:73-76` render messages as JSX text; the snapshot renderer escapes (`crates/lens/src/snapshot/render.rs:123-125`, `:810-816`); the Markdown export inserts messages and the domain raw (`crates/lens/frontend/src/lib/export.ts:34,53-54,77-78`). prism's vitest runs in `node` with `*.test.ts` only (`crates/mhost-prism/frontend/vitest.config.ts`); lens's runs in jsdom with `@solidjs/testing-library`.
- prism offers `@system` unconditionally (`QueryInput.tsx:33-41`, `App.tsx:1620`); the server refuses it when `allow_system_resolvers` is false (`crates/mhost-prism/src/security/query_policy.rs:81-91`, default true `config.rs:118-119,198`); the frontend reads only `/api/config` (`App.tsx:1099`), whose `ClientConfig` has no resolver flags (`api/meta.rs:156-167`); the production fixture sets no `allow_system_resolvers` (`crates/mhost-prism/tests/fixtures/prism.production.toml:24-25`).
- Without the DO bit hickory 0.26.3 keeps only DNSSEC records of the queried type (`hickory-proto-0.26.3/src/op/message.rs:205`), so the explicit RRSIG question is today's only RRSIG source; mhost 0.12's `check_dnssec` reads `lookups.rrsig()` for its presence, binding, expiry and algorithm lines (`mhost-0.12.0/src/lints/dnssec_lint.rs:27`). mhost's `Record` equality ignores TTL (`resources/record.rs:31`) and `check_ttl` keeps the highest TTL per record (`lints/ttl.rs:26`). prism's own `check_dnskey_algorithms` pushes one line per key (`check.rs:719`).
- ifconfig-rs refuses a non-global address with 400 unless `internal_mode` (`crates/ifconfig-rs/src/routes.rs:312`); lens forwards every resolved address (`ip.rs:132`). `/json` ranks internal > c2 > bot > cloud > vpn > tor > spamhaus > datacenter > residential (`backend/mod.rs:602`); `/range` classifies a CIDR's network address (`routes.rs:1755`). prism's own Markdown export escapes only `|` and newlines (`crates/mhost-prism/frontend/src/lib/export.ts:135`). prism's frontend fetches `/api/config` in `onMount` and swallows failures (`App.tsx:1099-1109`).
- Pinned results that move, each with `ADLC-Test-Change` naming its requirement: `lint_results_table.rs` (rewired through prism's lint function; two-resolver rows), `crates/lens/tests/unknown_verdicts.rs` ip rows, `tokenizer.test.ts:83` if `@system` handling moves. `check.rs`'s `test_check_total_steps_is_19` and the prism contract golden's batch `total` stay, since requirement 1 keeps the RRSIG question. No lens golden moves through DNS (prism.sse is literal-built) or IP (every fixture serves ifconfig-json.json).
- Neutral wording: R5.5 and R5.9 are hardening; no exploit strings beyond the SDD's hostile fixture.

## Requirements

1. **RRSIG question kept (R5.1).** prism keeps the explicit RRSIG question for 0.23.0 (mhost 0.12.0 offers no DO bit; amended in Phase 1). mhost's RRSIG lines (expiry, algorithm, binding) keep running on its answers; `CHECK_TOTAL_STEPS` stays 19.
2. **Records counted once (R5.1).** prism lints through one function over the unique records of all resolvers (one record per name, type and data; of duplicates the one with the highest TTL), so a zone answered identically by two resolvers gives the same lint lines as by one; identical lint lines within a category are emitted once.
3. **Reputation from flags (R5.2).** lens scores an address from ifconfig-rs's booleans: `is_spamhaus`, `is_c2` or `is_tor` → Fail; `is_vpn` → Warn; otherwise Pass. lens enriches only public addresses (`netray_common::target_policy`); a non-public one is not sent and counts as not checked. A public address whose enrichment failed or timed out makes the IP section Errored (the result incomplete). lens enriches at most eight public addresses, up to four IPv4 and up to four IPv6, each family sorted; when it checked fewer than it resolved, the reputation detail states "checked N of M addresses".
4. **One classifier (R5.2).** ifconfig-rs classifies with one function, in `/json`'s priority order (internal > c2 > bot > cloud > vpn > tor > spamhaus > datacenter > residential), used by `/json`, `/network` and `/range` (which applies it to the range's network address).
5. **Messages are text (R5.5).** A lint or check message is rendered as text in prism's and lens's frontends, in lens's snapshot HTML and in SVG/OG text; lens's and prism's Markdown exports put every message and the domain in an inline code span (a backtick fence longer than any backtick run in the value), so a copied report carries no live link, autolink or HTML. A `tests/repo` convention test fails when a frontend under `crates/*/frontend/src` or `packages/common-frontend/src` uses `innerHTML`, `outerHTML`, `insertAdjacentHTML` or `dangerouslySetInnerHTML`.
6. **`@system` only when allowed (R5.9, SC17).** prism's `/api/config` reports whether system resolvers are allowed; the UI lists and documents `@system` only when it said so (hidden before the answer and when the request fails). The server keeps refusing it when disallowed. `crates/mhost-prism/tests/fixtures/prism.production.toml` sets `[dns] allow_system_resolvers = false` (K5).

## Phase 1 — prism lints

**Depends on:** none
**Requirements:** 1, 2

### Test Scenarios

- GIVEN a signed zone whose DNSKEY answer carries its RRSIG (expiring in 3 days) WHEN prism lints it THEN mhost's near-expiry line appears.
- GIVEN a signed zone answered identically by two resolvers WHEN prism lints it THEN "Found 1 KSK(s) and 1 ZSK(s)" (the two-resolver rows of `lint_results_table.rs` move to the one-resolver lines).
- GIVEN two resolvers returning different records for one name WHEN prism lints THEN both records count.
- GIVEN one A record with TTL 45 from one resolver and 300 from another WHEN prism lints THEN no low-TTL warning (the higher TTL survives).
- GIVEN two identical lint lines in one category WHEN prism emits the category THEN the line appears once.
- GIVEN one resolver WHEN prism lints THEN the lines equal today's, except that identical lines collapse (two RSASHA1 keys give one deprecation line).

## Phase 2 — IP reputation

**Depends on:** none
**Requirements:** 3, 4

### Test Scenarios

- GIVEN the ifconfig golden with `type: cloud` and `is_spamhaus: true` WHEN lens scores it THEN reputation Fail.
- GIVEN `is_vpn: true` and no other flag WHEN scored THEN Warn; GIVEN no flag THEN Pass (the golden's row, as today).
- GIVEN every enrichment call answering 500 WHEN scored THEN the IP section is Errored and the result incomplete.
- GIVEN one of two addresses timing out WHEN scored THEN Errored.
- GIVEN ten A and two AAAA addresses WHEN lens enriches THEN four A (lowest four sorted) and two AAAA are enriched and the detail says "checked 6 of 12 addresses".
- GIVEN five A addresses WHEN lens enriches THEN the lowest four, detail "checked 4 of 5 addresses".
- GIVEN three A addresses WHEN lens enriches THEN all three, no "checked" detail.
- GIVEN A 198.51.100.7 and A 10.0.0.5 WHEN lens enriches THEN only 198.51.100.7 is sent, the section is scored (not Errored), detail "checked 1 of 2 addresses".
- GIVEN one fixture address WHEN ifconfig-rs answers `/json`, `/network` and `/range` for `<addr>/32` THEN all three report the same type.

## Phase 3 — Messages as text

**Depends on:** none
**Requirements:** 5

### Test Scenarios

The hostile value is `"><img src=x onerror=alert(1)>[x](javascript:alert(1))`, carried as a lint message.

- GIVEN a prism lint result with the hostile message WHEN the lint tab renders THEN the DOM has no `img` element and shows the text (pins today).
- GIVEN a lens DNS check with the hostile message WHEN `CheckList` renders THEN no `img` element (pins today).
- GIVEN a snapshot with the hostile message WHEN `/r/<id>` renders THEN the HTML contains `&lt;img` and no `<img src=x` (pins today).
- GIVEN the hostile message WHEN lens's `toMarkdown` runs THEN it appears only inside a code span (fails today).
- GIVEN a message `MX www.example.com. has no A/AAAA records` WHEN exported THEN the host is inside a code span, so it is not autolinked (fails today).
- GIVEN a message containing a backtick WHEN exported THEN the code span's fence is longer and the message survives verbatim.
- GIVEN the hostile value in a TXT record WHEN prism's Markdown export runs THEN it appears only inside a code span (fails today).
- GIVEN a frontend file assigning `innerHTML` WHEN the convention test runs THEN it fails; GIVEN the tree THEN it passes (its self-test fixture fails today).

## Phase 4 — `@system`

**Depends on:** none
**Requirements:** 6

### Test Scenarios

- GIVEN `allow_system_resolvers = false` WHEN `/api/config` is read THEN it reports system resolvers off.
- GIVEN the config off WHEN the query input suggests servers and the help opens THEN no `@system` suggestion and no help row.
- GIVEN the config on WHEN the same THEN both appear.
- GIVEN `/api/config` not yet answered or failing WHEN the input renders THEN no `@system` suggestion.
- GIVEN `allow_system_resolvers = false` WHEN a query names `@system` THEN `SYSTEM_RESOLVERS_DISABLED`, as today.
- GIVEN `prism.production.toml` WHEN loaded THEN `allow_system_resolvers` is false.

## Decision log

- Records deduplicated before linting, over deduplicating output lines only: the doubled KSK/ZSK count is one line, not two (lint results table, 2026-10-09).
- `+check` keeps the explicit RRSIG question for 0.23.0, over the DO bit: mhost 0.12.0's `Resolver` cannot set DO (`ResolverOpts::to_proto` never sets hickory's `validate`, the only source of `edns_set_dnssec_ok`); the DO bit follows with a DO option in mhost (mhost-security session). The expiry and algorithm lines stay (operator, 2026-10-09, amended in Phase 1; AMENDMENT to SDD R5.1).
- Non-public addresses are filtered before enrichment, over counting ifconfig-rs's refusal as a failure: such a domain would be incomplete forever (operator, 2026-10-09; AMENDMENT to SDD R5.2).
- `@system` hidden until the server allows it (operator, 2026-10-09).
- Messages and the domain as inline code in both Markdown exports, over escaping metacharacters: escaping leaves autolinks live (operator, 2026-10-09).
- Duplicates keep the highest TTL, as mhost's `check_ttl` does; one classifier in `/json`'s order, applied to a range's network address (independent reading).
- prism's frontend gains jsdom and `@solidjs/testing-library` as dev dependencies for its rendering test, matching lens's setup.
- Phases are independent; built in order 1–4.

## Open decisions

None.

## Out of scope

- R5.8 (prism nameserver queries under the target policy): its own feature.
- The lens and prism frontends beyond rendering safety and `@system`: V2.
