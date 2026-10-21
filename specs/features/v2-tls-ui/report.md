# Report: v2-tls-ui

## Phase 1 — Seam, addresses and certificate

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 port view seam: sibling blocks in ViewContext, the TLS view renders the port at tls.addresses, taken blocks answer empty, anchors per taken block, evidence.rs names no module | green | crates/web/tests/it/views_tls_port.rs |
| C2 | R2 nothing lost: the port view takes a block only when it renders every value | green | crates/web/tests/it/views_tls_nothing_lost.rs |
| C3 | R3 distinct values: equal values once, differing values each with their addresses | green | crates/web/tests/it/views_tls_addresses.rs |
| C4 | R4 addresses table: address, band, protocol, cipher, leaf with same or differs; error rows; port and handshake note | green | crates/web/tests/it/views_tls_addresses.rs |
| C5 | R5 one Certificate section per distinct served chain, headed by its addresses when several | green | crates/web/tests/it/views_tls_facts.rs |
| C6 | R6 validity bar from one pure Rust layout; EXPIRY_WARN_DAYS read by check_expiry_window; expired and not yet valid | green | crates/web/tests/it/views_tls_validity.rs |
| C7 | R7 facts grid: G's eight cells, then SHA-256, SHA-1, Policy, OCSP responder, CA issuers, SCT logs; absent facts not shown; self-signed and expired chips | green | crates/web/tests/it/views_tls_facts.rs, views_tls_links.rs |
| C8 | R8 OCSP cell: stapled parts, fresh until, live chip by a fixed map, no chip without the live key | green | crates/web/tests/it/views_tls_facts.rs |
| C9 | R9 chain pills, dashed trust store pill, trust text from chain_trusted and TRUST_STORE, crt.sh links, Chain certificates details | green | crates/web/tests/it/views_tls_chain.rs |
| C10 | S two addresses serving the same chain: one Addresses, one Certificate, both same, chain and handshake blocks render nothing of their own | green | crates/web/tests/it/views_tls_port.rs |
| C11 | S two addresses with different leaves: second row differs, two Certificate sections headed by their addresses | green | crates/web/tests/it/views_tls_addresses.rs |
| C12 | S the port view carries id ev-tls.addresses and an anchor for every taken chain and handshake block | green | crates/web/tests/it/views_tls_port.rs |
| C13 | S a stored tls.chain with an unknown key renders through the generic block, the addresses table still renders | green | crates/web/tests/it/views_tls_port.rs |
| C14 | S a row with outcome HANDSHAKE_TIMEOUT shows outcome and message across the remaining columns | green | crates/web/tests/it/views_tls_addresses.rs |
| C15 | S handshakes of 22 ms and 31 ms on port 443: the note reads Port 443 and both durations with their addresses | green | crates/web/tests/it/views_tls_addresses.rs |
| C16 | S validity layout 2026-10-07 to 2027-01-06 at 2026-10-09: marker at 2 of 91 days, warn zone the last 30 days, label today 89 days left | green | crates/web/tests/it/views_tls_validity.rs |
| C17 | S an expired leaf: bar in the fail tone, expired n days ago | green | crates/web/tests/it/views_tls_validity.rs |
| C18 | S check_expiry_window compares against EXPIRY_WARN_DAYS | green | crates/web/tests/it/views_tls_validity.rs |
| C19 | S facts grid order with serial, SHA-1, policy OV, OCSP and CA issuers URLs | green | crates/web/tests/it/views_tls_facts.rs |
| C20 | S a leaf without policy and serial has neither cell | green | crates/web/tests/it/views_tls_facts.rs |
| C21 | S stapled good with this_update 2026-10-08 reads stapled · good · 2026-10-08 | green | crates/web/tests/it/views_tls_facts.rs |
| C22 | S live revoked chip with date and reason in the fail tone; no live key, no chip | green | crates/web/tests/it/views_tls_facts.rs |
| C23 | S a leaf SHA-256 4F:9A:…:E1 pill links crt.sh with the hex without colons | green | crates/web/tests/it/views_tls_chain.rs |
| C24 | S leaf and intermediate: dashed issuer pill from the trust store, trusted by TRUST_STORE, intermediate facts in Chain certificates | green | crates/web/tests/it/views_tls_chain.rs |
| C25 | S a port of two addresses with every registered key: every value's display text appears | green | crates/web/tests/it/views_tls_nothing_lost.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 0 (two constants, written by the orchestrator) | orchestrator | 0 | 0 |
| G2 | 2 | opus | 172343 | 662 |
| review fixes | 2 | opus | 84826 | 189 |

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | SANs carried no link | fixed: DNS names and addresses link through the evidence cell, wildcards stay plain; test `views_tls_links.rs` |
| BLOCKER | the validity bar read the wall clock, so a stored run could show an expiry it never measured and the stored days_remaining vanished | fixed: the run's moment is not_after minus days_remaining; requirement 6 amended; the c17 fixture now truncates days toward zero as the backend does (ADLC-Test-Change) |
| BLOCKER | inferred root pill with an empty name for an unparsed last certificate | fixed: only with self_signed false and an issuer |
| AMENDMENT | trust text on warn or fail is the catalogue finding (a view holds no findings) | requirement 9 amended |
| AMENDMENT | the port-wide chain_trusted printed under every chain section | requirement 9 amended: only with one distinct chain |
| AMENDMENT | inferred trust-store pill beside a failing chain_trusted | requirement 9 amended: only when the check passes |
| AMENDMENT | a value some addresses lack read as port-wide | requirement 3 amended: named per address |
| AMENDMENT | taken blocks' feeds lines are not shown | requirement 1 amended: the port view's feeds line is the addresses block's |
| AMENDMENT | same/differs with a lone leaf | requirement 4 amended: markers need two leaves |
| DEFERRED | every TLS part re-streams with the band port's checks | Phase 3 (requirement 13) |
| NIT | "1 days", "0 days" | fixed: singular, "expired today", "not valid for less than a day" |

### Behavioural verification

Local lens from this worktree, seeded fixture history (scratchpad `tlsui/lens-up.sh`, port 18531);
`curl -s localhost:18531/shop.example.com/tls`, the port view's text:

```
Addresses Address Band Protocol Cipher Leaf 192.0.2.10 TLSv1.3 TLS13_AES_256_GCM_SHA384 SHA256 a1:96:…:8a same
198.51.100.10 TLSv1.3 TLS13_AES_256_GCM_SHA384 SHA256 a1:96:…:8a same 2001:db8::10 HANDSHAKE_FAILED No route to host (os error 65) …
Port 443 · handshake 60 ms (192.0.2.10) · 55 ms (198.51.100.10) Handshake Key exchange X25519 Encrypted Client Hello no
Certificate not before 2026-09-26 today · 68 days left not after 2026-12-25 the last 30 days turn the expiry line amber
Subject CN=shop.example.com Alternative names shop.example.com , *.shop.example.com … Certificate Transparency 2 SCTs …
Chain shop.example.com crt.sh ↗ → Example TLS Issuing ECC CA 1 crt.sh ↗ → … trusted by the Mozilla root program Chain certificates …
```

## Phase 2 — Protocols, notices and related records

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R10 protocols grid: cells in order with states, tones from their checks, absent facts not shown except SNI, probe answers in details | green | crates/web/tests/it/views_tls_protocols.rs |
| C2 | R11 notices banner above the addresses: skipped count and limit, skipped addresses linked, each warning | green | crates/web/tests/it/views_tls_notices.rs |
| C3 | R12 related box summaries from the band port's blocks, filled when the part arrives and in a shared run | green | crates/web/tests/it/inspect_tls_related.rs |
| C4 | S TLS 1.3/1.2 enabled, 1.1/1.0 disabled, forward secrecy all suites, ALPN h2, ECH false, SNI example.com: the first eight cells and the HSTS link | green | crates/web/tests/it/views_tls_protocols.rs |
| C5 | S ech_advertised warn and legacy_versions pass: ECH bar warn, TLS 1.1 and 1.0 bars pass, HSTS no tone | green | crates/web/tests/it/views_tls_protocols.rs |
| C6 | S a handshake without sni: SNI reads none, IP target | green | crates/web/tests/it/views_tls_protocols.rs |
| C7 | S starttls smtp: a STARTTLS cell reads smtp; none: no STARTTLS cell | green | crates/web/tests/it/views_tls_protocols.rs |
| C8 | S no tls.protocols: no version cell | green | crates/web/tests/it/views_tls_protocols.rs |
| C9 | S two tls.probes blocks: Probe answers holds a column per address with each word and answer | green | crates/web/tests/it/views_tls_protocols.rs |
| C10 | S notices with one warning and two skipped addresses beside four inspected: the banner text, both addresses, the warning; no notices: no banner | green | crates/web/tests/it/views_tls_notices.rs |
| C11 | S tls.caa with two records and no TLSA block: CAA (2 records) and TLSA (not measured); an empty TLSA block: TLSA (none) | green | crates/web/tests/it/inspect_tls_related.rs |
| C12 | S the protocols, probes and notices fixtures: every value's display text appears | green | crates/web/tests/it/views_tls_nothing_lost.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | opus | 129683 | 385 |
| review fixes | 3 | opus | 49883 | 85 |

The phase was planned by the orchestrator (one group, one production file); the c9 fixture's
IPv6 qualifiers were bracketed (`BlockId` refuses a bare IPv6 address), a test change.

### Review

| class | finding | disposition |
|---|---|---|
| AMENDMENT | the related box stayed pending when the band port's part failed or timed out | requirement 12 amended and fixed: every entry "(not measured)", live and shared |
| AMENDMENT | stored runs older than the `sni` key read "SNI none, IP target" and linked HSTS to an address | requirement 10 amended and fixed: SNI and HSTS only with `sni`; c6 and c8 changed (ADLC-Test-Change) |
| AMENDMENT | a check's bar contradicted its own cell (legacy_versions on a disabled version; forward_secrecy judging the negotiated suite under "not all suites") | requirement 10 amended and fixed: bars never contradict their cell; Forward secrecy reads weak_suites |
| DEFERRED | every TLS part re-streams with the band port's checks | Phase 3 (requirement 13) |

### Behavioural verification

Local lens from this worktree, seeded history (port 18531); `curl -s localhost:18531/shop.example.com/tls`
(a stored run older than `sni`):

```
Protocols and features ALPN none Encrypted Client Hello not advertised Key exchange X25519
```

## Phase 3 — Per-port parts and tabs

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R13 each TLS part carries its port's checks, renders its views with them on arrival, and a shared run stores and renders them | open | crates/web/tests/it/inspect_tls_panels.rs |
| C2 | R14 several ports: tab row, one panel per port with its band, check list and port view, no band under the h1; tab meta with STARTTLS, tone and counts; band port current; tabs island; no-JS in-page links | open | crates/web/tests/it/inspect_tls_panels.rs |
| C3 | S example.com:443,465,587: tabs 443, 465, 587 · STARTTLS, 443 current, panels with own band and check list, no band under the h1 | open | crates/web/tests/it/inspect_tls_panels.rs |
| C4 | S port 465 failing its handshake: its tab carries the fail tone and its counts, 443's does not | open | crates/web/tests/it/inspect_tls_panels.rs |
| C5 | S a one-port run: no tab row, one band under the h1 | open | crates/web/tests/it/inspect_tls_panels.rs |
| C6 | S a TLS part carries its own port's tones and is not re-sent at band end | open | crates/web/tests/it/inspect_tls_panels.rs |
| C7 | S a three-port share key renders panels with their own bands and tones | open | crates/web/tests/it/inspect_tls_panels.rs |
| C8 | S without JavaScript a three-port shared run renders all panels in order with in-page tab links | open | crates/web/tests/it/inspect_tls_panels.rs |
