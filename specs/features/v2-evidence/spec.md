# Spec: v2 evidence

Status: Ready for Implementation
Created: 2026-10-13

## Goal

Every module projects what it measured into evidence blocks of the six shapes of SDD §3.7: `records`, `table`, `chain`, `tree`, `fields` and `text`.
- Values are typed and tagged `{t, v}`.
- Keys are registered per module.
- Each module declares its volatile paths.
- A test fails on an unregistered key or a mistyped value (V1.14).

Every check that reached `pass`, `warn` or `fail` references at least one block of the same run. That block may belong to another module (R28, R29).

`SectionOutcome::Measured` carries `evidence` instead of the transitional `presentation`. Lens derives its existing wire fields from the evidence: `headline`, `ip.addresses`, the `http` server fields and `email.grade`. It also adds two fields to its SSE, sync JSON and MCP output:
- `evidence` per section;
- `evidence` per check item.

## Non-goals

- `tree` blocks: the SPF include tree and the DNSSEC chain of trust. The shape exists in the model, but no module emits it here. Beacon keeps only the flat SPF prefix list (`AllResults.SpfFlat`).
- Rendering evidence in the lens frontend. The frontend gets types only; rendering is Phase 4 (§6 of the SDD).
- Storing evidence in V1 snapshots. Snapshots go in Phase 2 (S2).
- The history diff over evidence (Phase 2, R12) and API v1 (Phase 3).
- Removing duplicate lookups between modules (V1.5's "every module from 1b"). The resolve stage carries the blocks of the shared lookups (requirement 3a), but the modules keep their own lookups.

## Context and constraints

All facts are from the tree at 13fa7dc.

**Model and engine**

| Item | Today |
|---|---|
| `CheckResult.evidence` | `Vec<String>` (`crates/model/src/lib.rs:162-169`). No module sets it, and lens drops it when it maps checks (`crates/lens/src/modules.rs:202-208`). |
| `CheckResult` derives | `Serialize` only |
| `EvidenceRef`, block, shape, value types, schema version | none exist |
| `EvidencePath` | `crates/engine/src/lib.rs:28-30` |
| `Module::volatile()` | required (`lib.rs:111-112`); every module returns `&[]` |
| `SectionOutcome::Measured` | `{ checks, presentation: serde_json::Value }` (`lib.rs:77-92`) |

**`presentation` per module**

| Module | Keys | Built at |
|---|---|---|
| dns | `headline`, `resolved_ips` | `crates/dns/src/module.rs:509-512` |
| tls | `headline` | `crates/tls/src/module.rs:319-322`, :428-445 |
| http | `headline`, `status_code`, `http_version`, `response_duration_ms`, `server_ip`, `server_org`, `server_network_type` | `crates/http/src/module.rs:248-261` |
| email | `headline`, `grade` | `crates/email/src/module.rs:294-300` |
| ip | `headline`, `addresses[]{ip, org, geo, network_type}` | `crates/ip/src/module.rs:222-235` |

**Data each module holds**
- **DNS:** `CheckEvent::Batch` with `Lookups` per record type, covering 15 types (`crates/dns/src/api/check.rs:59-73`). `CHECK_INPUTS` already maps every check to its record types (`crates/dns/src/module.rs:364-374`).
- **TLS:** `InspectResponse` (`crates/tls/src/routes.rs:32-49`) holds:
  - per address, `TlsParams` and `chain: Vec<CertInfo>`;
  - per port, `ValidationResult` and `tlsa`;
  - `dns.caa`.
- **HTTP:** `InspectResponse` (`crates/http/src/inspect/assembler.rs:9-38`) holds redirects, `http_upgrade`, headers, security, cors, cookies, caching and enrichment.
- **Email:** The module sees only beacon's SSE categories and sub-checks. The structured data stays in `AllResults` (`crates/email/src/quality/types.rs:175-204`): `mx_hosts`, `mx_ips`, `SpfFlat`, the DMARC policy and `MtaStsInfo`. The DNSBL query structs are private (`checks/dnsbl.rs:23-41`).
  - `check_mx` drops the MX preference (`checks/mx.rs:84`) and merges every host's addresses into one list (:140).
  - `SpfFlat` holds only `authorized_prefixes` (`quality/types.rs:207`).
  - TLS-RPT and BIMI keep only a `present` bool.
  - The lens-full goldens build the email section by replaying `beacon-*.sse` through `translate` (`crates/email/src/testing.rs:35`).
  - The V1 email frontend dispatches every SSE message by its `type` field (`crates/email/frontend/src/lib/api.ts:51-70`).
- **IP:** `Ifconfig` per address (`crates/ip/src/backend/mod.rs:362`), with network and location fields. `ip.reputation` judges `is_spamhaus`, `is_c2`, `is_tor` and `is_vpn` (`crates/ip/src/module.rs:189`).
- **TLS headline:** `<version>, expires in <days_remaining>d` (`crates/tls/src/module.rs:439`). `days_remaining` is a field of `CertInfo`.
- **Facts:** `Facts` holds `a`, `aaaa`, `mx`, `caa`, `ns` and `https` as values, without TTLs (`crates/engine/src/lib.rs:61-73`). The `Lookups` they come from are dropped after projection (`crates/dns/src/module.rs:245-293`). SDD §3.6 says the `Facts` carry their DNS evidence blocks.
- **DNS fixtures:** `prism.sse` holds only A, NS and TXT batches, and `prism-no-address.sse` only A and AAAA. Yet `dns.caa` is `warn` in nine lens-full goldens.

**Lens consumers of `presentation`** (decoded in `crates/lens/src/modules.rs:106-148`, :196-274)
- SSE and sync JSON: `headline`, the http extras, `ip.addresses` and `email.grade` (`crates/lens/src/routes.rs:909-1127`).
- Snapshot `server_addresses` (`snapshot/types.rs:117-146`).
- The cache replays the decoded fields.
- The frontend shows `headline` only for an error section. It renders `ip.addresses` (`IpSection.tsx:95-113`), and uses it with `http.server_*` in `Summary.tsx:240-268` and in the export (`lib/export.ts:82-90`).
- OG and badge read no extras.

**Goldens**
- `tests/fixtures/contracts/lens-full-*.json` (10 files) pin `headline`, the http extras except `response_duration_ms`, `ip.addresses` and `email.grade` (`crates/lens/tests/lens_full_output.rs`). The test strips `response_duration_ms` as volatile (:122).
- Lens tests read the DNS `resolved_ips` (`crates/lens/tests/contract_backends.rs:59`, `engine_run.rs:160`).
- Module tests compare `presentation` with those goldens: `crates/{dns,tls,http,email,ip}/tests/module.rs`, `crates/lens/tests/contract_backends.rs`.

**Rules**
- The gate is `just adlc-verify`, and it runs offline.
- A changed protected test needs `ADLC-Test-Change`.
- Only example.com and netray.info appear as domains in fixtures.
- "A failed lookup is never an answer" (`CLAUDE.md`): an `unmeasured` check may reference no block.

## Requirements

### Phase 1 — Model

1. **The evidence model in `netray-model`.**
   - `Block { id: BlockId, title: String, shape: Shape }`.
   - `Shape` is `Records(Vec<Record>)`, `Table { key: String, columns: Vec<Column>, rows: Vec<Row> }`, `Chain(Vec<Fields>)`, `Tree(Node)`, `Fields(Vec<(Key, Value)>)` or `Text(String)`.
   - `Value` is `Text`, `Int`, `Bool`, `Time`, `Duration` or `Addr`. It serialises tagged as `{"t": "<type>", "v": <value>}`:
     - `time` is an RFC 3339 string;
     - `duration` is an int in milliseconds;
     - `addr` is an IP address string.
   - `EVIDENCE_SCHEMA_VERSION = 1`.
   - `CheckResult.evidence` becomes `Vec<BlockId>`.
   - A `BlockId` is `<module>.<name>` with an optional `@<qualifier>`.
     - `<name>` follows the check-ID rule `[a-z][a-z0-9_]*`.
     - The qualifier is an address, `<address>:<port>` or `<port>`. An IPv6 address is bracketed (`[2001:db8::10]:443`).
     - Examples: `dns.caa`, `tls.chain@192.0.2.1:443`, `tls.chain@[2001:db8::10]:443`, `tls.tlsa@443`.
     - Parsing refuses any other form.
   - The model types derive Serialize and Deserialize, and `CheckResult` gains Deserialize.
2. **The register.** Each module declares a `Register` of `(block name, key, value type)` triples, `const` in its crate. A shared helper, `netray_model::evidence::validate(&Register, &[Block])`, fails on:
   - a key that is not registered;
   - a value whose type differs from the registered one;
   - a block whose module prefix is not the module's;
   - a block whose name is not registered, even when the block is empty.
3. **The engine.**
   - `SectionOutcome::Measured { checks, evidence: Vec<Block> }`. `presentation` is removed in Phase 6, once lens no longer reads it; until then both exist.
   - `volatile()` returns `EvidencePath`s of the form `<block name>/<key>`, or `<block name>/<column>@<row key>` for a single table row (e.g. `http.headers/value@date`).

3a. **Facts carry blocks.** The resolve stage projects its lookups into the DNS-owned records blocks `dns.a`, `dns.aaaa`, `dns.mx`, `dns.caa`, `dns.ns` and `dns.https`, with TTLs. `Facts` carries them, and the DNS section publishes them, even when the DNS module timed out. The DNS module does not project those six types itself, and its checks reference the facts blocks. A lookup that errored gives no block (Rule M).
   - A helper checks every check's references against the run's blocks, across modules.

### Phases 2–5 — Projection per module

These requirements apply to every module.

4. A module emits its blocks in every measured section. Each measured check references the blocks it judged by. Each module has a test that calls `validate` on its fixtures' blocks and asserts the references for each check.
5. A module declares its volatile paths from §3.7's list. A test asserts that every volatile path names a registered key.

| Phase | Module | Blocks (block name: shape) | Check references |
|---|---|---|---|
| 2 | dns | `dns.<type>`: records, one per base-name record type the check run answered, besides the six facts types (`cname`, `dnskey`, `ds`, `nsec`, `nsec3`, `rrsig`, `soa`, `svcb`, `txt`). Keys: `owner`, `type`, `ttl`, `data`. Subdomain TXT batches belong to email and are not projected. | From `CHECK_INPUTS` with the facts blocks. `dns.ttl` references every records block of the run. |
| 3 | tls | `tls.addresses`: table (`address`, `port`, `outcome`, `message`), one row per tested address, including the failed ones. `tls.chain@<addr>:<port>`: chain of certificate `fields` (`subject`, `issuer`, `sans`, `not_before`, `not_after`, `days_remaining` (volatile), `key`, `signature_algorithm`, `fingerprint_sha256`, `position`). `tls.handshake@<addr>:<port>`: fields (`version`, `cipher_suite`, `alpn`, `key_exchange_group`, `ocsp_stapled`, `ech`, `handshake_ms` (volatile)). `tls.tlsa@<port>`: records. `tls.ct`: table. | `tls_reachable` references `tls.addresses`. Per-address checks reference that address's chain or handshake. `consistency`, `alpn_consistency` and `ocsp_stapled` reference every handshake block. `ech_advertised` references the handshakes and `dns.https`. `aia_reachability` references the chains. `caa_compliant` references `dns.caa` (a facts block). `dane_valid` references `tls.tlsa@<port>`. `ct_logged` references `tls.ct`. |
| 4 | http | `http.redirects` and `http.port80`: chains of hop `fields` (`url`, `status`, `location`, `http_version`). `http.headers`: table keyed by `name` (`name`, `value`). The row values of `date`, `age`, `set-cookie`, `server-timing`, `etag`, `last-modified` and request-ID headers are volatile (§3.7). `http.server`: fields (`final_url`, `status_code`, `http_version`, `response_ms` (volatile), `address`, `org`, `network_type`). `http.cors`: fields. `http.cookies`: table. | `https_redirect` references `http.port80`. `hsts`, `csp`, the four presence headers, `deprecated_headers`, `info_leakage` and `caching` reference `http.headers`. `cors` references `http.cors`. `cookie_secure` references `http.cookies`. |
| 5 | email | `email.mx`: table keyed by `host` (`preference`, `host`, `addresses`). `email.spf`: fields (`record`, `authorized_prefixes`, `lookup_count`). `email.dmarc`, `email.mta_sts`, `email.tls_rpt` and `email.bimi`: fields, each with its `record`. `email.mta_sts_policy`: text. `email.dkim`: table keyed by `selector`. `email.dane`: table keyed by `host`. `email.dnsbl`: table keyed by `query` (`query` = `<address>@<zone>`, `zone`, `address`, `answer`). `email.fcrdns`: table keyed by `address`. `email.summary`: fields (`grade`). | Each `email.<check>` references its block. The MX dependents also reference `email.mx`. |
| 5 | ip | `ip.addresses`: table keyed by `address` (`address`, `asn`, `org`, `prefix`, `network_type`, `country`, `city`, `spamhaus`, `c2`, `tor`, `vpn`). | `ip.reputation` references `ip.addresses`. |

6. **Email.**
   - Beacon keeps what the blocks need:
     - MX preference, and each host's addresses;
     - the SPF record and its lookup count;
     - the DMARC, TLS-RPT, BIMI and MTA-STS records, and the MTA-STS policy text;
     - the DKIM selectors with key type and bits;
     - the DANE TLSA records per host;
     - the DNSBL answers per query;
     - FCrDNS per address.
   - Beacon emits the email blocks as one additive SSE message with `type: "evidence"`, before `summary`. `translate` builds the blocks from that message, so a replayed stream and a live run take the same path.
   - The V1 email frontend ignores the new message type; a frontend test pins this.
   - The `beacon-*.sse` goldens are regenerated, and their existing messages stay byte-identical.

### Phase 6 — Lens

7. Lens derives `headline` and `ip.addresses` from the evidence, and likewise `http.status_code`, `http_version`, `server_ip`, `server_org`, `server_network_type`, `response_duration_ms`, `email.grade` and the DNS `resolved_ips` (from `dns.a`/`dns.aaaa`). The TLS headline reads `version` and `days_remaining`. The snapshot's `server_addresses` comes from the same blocks. `presentation` is then removed from `SectionOutcome`, from the modules and from lens. Apart from the added fields of requirement 8, the lens-full goldens keep every existing field byte-identical.
8. Lens adds to every section event, sync JSON section and MCP section:
   - `evidence: [Block]`, in the tagged JSON of requirement 1;
   - `evidence: [block-id]` on every check item;
   - `evidence_schema: 1` on the summary.

   `site/api/lens.html` documents all three. The frontend `types.ts` gains the types, but no rendering.
9. A lens test runs every lens-full fixture and asserts that every check item's references resolve to a block in the same run's `evidence`, across sections.

### Prose (after Phase 6)

10. `CHANGELOG.md` `[Unreleased]` lists the additive wire fields and the removal of `presentation` (internal). `crates/lens/README.md` and the module READMEs that describe output name the blocks.

## Phase 1 — Model and engine

**Depends on:** none
**Requirements:** 1, 2, 3

### Test Scenarios

- GIVEN a `Value::Time`, WHEN serialised, THEN it is `{"t":"time","v":"<RFC 3339>"}`, and it deserialises back equal. The same round trip holds for `Text`, `Int`, `Bool`, `Duration` and `Addr`.
- GIVEN the block id `tls.chain@192.0.2.1:443`, WHEN parsed, THEN module `tls`, name `chain` and qualifier `192.0.2.1:443`. GIVEN `tls..chain` or `chain`, WHEN parsed, THEN an error.
- GIVEN a register holding `(dns.caa, data, text)` and a `dns.caa` block with key `data` as Text, WHEN validated, THEN ok. With key `extra`, THEN an error naming `extra`. With `data` as Int, THEN a type error.
- GIVEN a `tls` module's register, WHEN it validates a block `dns.caa`, THEN an error, because the prefix is not the module's.
- GIVEN a run whose `tls.caa_compliant` references `dns.caa` and whose DNS section emits `dns.caa`, WHEN references are checked, THEN ok. If DNS does not emit `dns.caa`, THEN an error naming the dangling reference.

## Phase 2 — DNS

**Depends on:** Phase 1
**Requirements:** 3a, 4, 5 (dns)

### Test Scenarios

- GIVEN the DNS fixtures with a CAA batch added to `prism.sse` and NS and CAA batches added to `prism-no-address.sse` (ADLC-Test-Change), WHEN translated, THEN every block validates against the DNS register, and `dns.caa` references `dns.caa`.
- GIVEN Facts whose CAA lookup answered, WHEN the DNS section times out, THEN the run still publishes `dns.caa` and `tls.caa_compliant`'s reference resolves.
- GIVEN a CAA batch that errored, WHEN translated, THEN `dns.caa` is `unmeasured`, and no `dns.caa` block claims records. Rule M: a failed lookup is no answer. The block is absent.
- GIVEN the DNS volatile list, WHEN checked, THEN every `dns.<type>/ttl` path is registered.

## Phase 3 — TLS

**Depends on:** Phase 1
**Requirements:** 4, 5 (tls)

### Test Scenarios

- GIVEN `tlsight-inspect.json`, WHEN translated, THEN the blocks `tls.chain@<addr>:443` and `tls.handshake@<addr>:443` validate, and `tls.not_expired` references the chain of the worst address.
- GIVEN that fixture, WHEN translated, THEN `tls.caa_compliant` references `dns.caa`.
- GIVEN an address that timed out, WHEN translated, THEN it has no chain or handshake block, and `tls.addresses` holds its row with outcome `HANDSHAKE_TIMEOUT`.
- GIVEN `tlsight-unreachable.json`, WHEN translated, THEN `tls.tls_reachable` is fail and references `tls.addresses`.

## Phase 4 — HTTP

**Depends on:** Phase 1
**Requirements:** 4, 5 (http)

### Test Scenarios

- GIVEN `spectra-inspect.json`, WHEN translated, THEN `http.headers`, `http.redirects` and `http.server` validate, `http.hsts` references `http.headers`, and `http.https_redirect` references `http.port80`.
- GIVEN an HTTPS probe timeout, WHEN translated, THEN there is no `http.headers` block, and the unmeasured checks reference nothing.

## Phase 5 — Email, IP

**Depends on:** Phase 1
**Requirements:** 4, 5 (email, ip), 6

### Test Scenarios

- GIVEN the healthy beacon scenario, WHEN the module runs, THEN `email.mx`, `email.spf`, `email.dmarc` and `email.dnsbl` validate, and each measured email check references its block.
- GIVEN the same run, WHEN the V1 SSE is compared with `beacon-*.sse`, THEN every pre-existing message is byte-identical, and one `evidence` message precedes `summary`.
- GIVEN the V1 email frontend, WHEN it receives a message with `type: "evidence"`, THEN it renders nothing for it and keeps streaming.
- GIVEN an address listed on Spamhaus, WHEN IP translates, THEN its `ip.addresses` row has `spamhaus: true` and `ip.reputation` references the block.
- GIVEN the IP fixture, WHEN translated, THEN `ip.addresses` validates and `ip.reputation` references it.

## Phase 6 — Lens

**Depends on:** Phases 2, 3, 4, 5
**Requirements:** 7, 8, 9

### Test Scenarios

- GIVEN every lens-full fixture, WHEN lens streams it, THEN every pre-existing field equals the golden. That covers `headline`, `ip.addresses`, `http.server_*` and `email.grade`. The `evidence` fields are new.
- GIVEN a run, WHEN streamed, THEN every check item's `evidence` ids resolve to blocks in the run.
- GIVEN the source tree, WHEN searched, THEN no `presentation` field remains in the engine, the modules or lens.

## Decision log

- Lens derives its existing wire fields from the evidence, over removing them. The SPA keeps its IP list and server block until Phase 4 (operator, 2026-10-13).
- One block set per check owner, over full scope with trees or a minimal model. It gives Phase 2's diff substance without rebuilding beacon's SPF expansion (operator, 2026-10-13).
- The blocks go additively onto the lens wire now, over internal only. They are visible early, and MCP gets them (operator, 2026-10-13).
- Values are tagged `{t, v}`, over native JSON plus a register. A value describes itself; consumers need no register, and the diff and renderer stay generic (operator, 2026-10-13).
- Check items carry their evidence ids on the wire, cross-module included, over not carrying them (R28, R29) (operator, 2026-10-13).
- The resolve stage owns the shared lookups' blocks (`dns.a`, `aaaa`, `mx`, `caa`, `ns`, `https`), as in SDD §3.6, over letting a reference dangle. A judged check always shows its evidence (operator, 2026-10-13).
- Beacon carries its blocks in an additive SSE message `evidence`, over in-process goldens. Replay and production take one path (operator, 2026-10-13).
- Block names, keys and titles as in the table above, over renaming them before release. They are on the wire but not rendered until Phase 4, and a released key is never reused with another meaning (§3.7) (operator, 2026-10-13).

### Amendments

- **A1 (operator, 2026-10-13).** The DNS module projects every one of its 15 answered types itself, the six facts types included, and does not wait for the facts. The resolve stage still carries `dns.a`, `aaaa`, `mx`, `caa`, `ns` and `https`. This replaces 3a's "the DNS module does not project those six types itself".
- **A2 (operator, 2026-10-13).** When the same id exists in both the facts and a section, the section's block wins. The facts fill in only the ids that no section carries, such as after a DNS timeout.
- **A3 (phase 3).** `caa_compliant` references `dns.caa` only when TLS judged from the facts' CAA, otherwise `tls.caa`, otherwise the chain blocks. `dane_valid` references `tls.tlsa@<port>`, otherwise the chains. TLS blocks cover the judged port only.
- **A4 (phase 4).** HTTP adds a `http.port80_probe` fields block. `https_redirect` references `http.port80` and `http.port80_probe`. `http.cookies` is keyed by `<name>; <domain>; <path>` with an ordinal suffix on a repeat. After an HTTPS refusal, `http.server` carries `error_kind`, and the ten response checks reference it. CSP nonces are left for the Phase 2 comparison.
- **A5 (phase 5).** The email block set and keys are as listed in the report's Phase 5 amendment: `email.mx` plus `email.mx_addresses`, DNSBL rows per query with `answered`, DKIM rows per key with `state`, deciding facts as keys, and `email.dnssec`. Lens drops cross-module references that do not resolve within a run (Phase 6).

## Open decisions

None.

## Out of scope

- The SPF and DNSSEC trees: a later feature, once beacon keeps the include tree.
- Rendering evidence: Phase 4, SDD §6.
- Storing evidence and the history diff: Phase 2, SDD §4.
