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
