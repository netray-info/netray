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
| C1 | R13 each TLS part carries its port's checks, renders its views with them on arrival, and a shared run stores and renders them | green | crates/web/tests/it/inspect_tls_panels.rs |
| C2 | R14 several ports: tab row, one panel per port with its band, check list and port view, no band under the h1; tab meta with STARTTLS, tone and counts; band port current; tabs island; no-JS in-page links | green | crates/web/tests/it/inspect_tls_panels.rs |
| C3 | S example.com:443,465,587: tabs 443, 465, 587 · STARTTLS, 443 current, panels with own band and check list, no band under the h1 | green | crates/web/tests/it/inspect_tls_panels.rs |
| C4 | S port 465 failing its handshake: its tab carries the fail tone and its counts, 443's does not | green | crates/web/tests/it/inspect_tls_panels.rs |
| C5 | S a one-port run: no tab row, one band under the h1 | green | crates/web/tests/it/inspect_tls_panels.rs |
| C6 | S a TLS part carries its own port's tones and is not re-sent at band end | green | crates/web/tests/it/inspect_tls_panels.rs |
| C7 | S a three-port share key renders panels with their own bands and tones | green | crates/web/tests/it/inspect_tls_panels.rs |
| C8 | S without JavaScript a three-port shared run renders all panels in order with in-page tab links | green | crates/web/tests/it/inspect_tls_panels.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | opus | 111588 | 332 |

Planned by the orchestrator (one group). The review's amendment (unmeasured tab) was fixed by the
orchestrator with the test `c4_an_unmeasured_port_reads_not_measured_without_the_pass_tone`.

### Review

| class | finding | disposition |
|---|---|---|
| AMENDMENT | a port with only unmeasured lines carried the pass tone on its tab | requirement 14 amended and fixed; test added (ADLC-Test-Change) |
| NIT | each panel's slim spectrum repeats the id `band-tls` | kept: nothing references it; the spectrum component owns the id |
| NIT | a part past the deadline has no band region live but "timed out" in the shared run | kept: the shared run states what the run ended with |
| DEFERRED | a part whose checks future never resolves loses its evidence at the deadline | `port_jobs` shares one future for both; documented on `with_part_checks` as a proposal |

### Behavioural verification

This branch's build at the main checkout's target path (network allowed), lens on port 18541,
`curl -X POST -H 'Accept: text/event-stream' --data 'h=1.1.1.1:443,853' localhost:18541/inspect/tls/run`;
fragment ids and texts:

```
inspect-result      … 1.1.1.1:443,853 443 853 port 443 pending port 853 pending
inspect-part-0      port 443 [slim band, 23 lines] 1 line needs attention 0 fail · 1 warn …
inspect-port-meta-0 · 0 fail · 1 warn
inspect-part-1      port 853 [slim band] No line needs attention 23 not measured … TLS reachable 1.1.1.1 …
inspect-port-meta-1 (empty: before the review fix; now "· 23 not measured")
inspect-share, inspect-export
```

No `inspect-band` fragment in the panel run; each part arrived once.

## Phase 4 — Port chooser

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R15 port chooser in the TLS form card: toggles, +, presets, field, host field as source of truth, limit with note, hidden until the island shows it, help names host:443,465 | green | crates/web/tests/it/inspect_tls_chooser.rs, crates/web/islands/test/ports.test.ts |
| C2 | R18 no JavaScript: the chooser is absent (hidden); bundle under budget; the island writes no style | green | crates/web/tests/it/inspect_tls_chooser.rs, crates/web/islands/test/budget.test.ts |
| C3 | S the idle TLS page holds a hidden Ports group with 443 pressed, 8443 unpressed, + and data-max-ports 7; other inspectors none | green | crates/web/tests/it/inspect_tls_chooser.rs |
| C4 | S adding 465 with host example.com: the field reads example.com:443,465 | green | crates/web/islands/test/ports.test.ts |
| C5 | S typing example.com:25,587 presses 25 and 587 and releases 443 | green | crates/web/islands/test/ports.test.ts |
| C6 | S the E-Mail preset presses 25, 465, 587, 993, 995 | green | crates/web/islands/test/ports.test.ts |
| C7 | S a limit of 7 with seven pressed: an eighth stays unpressed and the note names the limit | green | crates/web/islands/test/ports.test.ts |
| C8 | S the last pressed port stays pressed | green | crates/web/islands/test/ports.test.ts |
| C9 | S the islands bundle stays within the budget | green | crates/web/islands/test/budget.test.ts |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | opus | 83710 | 316 |
| review fixes | 1 | opus | 50119 | 165 |

Planned by the orchestrator (one group). The empty-field note was added by the orchestrator.

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | the shell writes the host field without input events, so the chooser showed a stale set and a toggle rewrote the field from it | fixed: the field is re-read before every action, on submit and popstate; test added |
| BLOCKER | the number-typed extra-port field blocked every submit once it held an invalid value | fixed: a text field, cleared after every value; test added |
| AMENDMENT | unbracketed IPv6 and non-port suffixes were rewritten into other targets | requirement 15 amended and fixed: the suffix is read as the server's parser reads it, IPv6 bracketed; tests added |
| NIT | the chooser accepted lists the server refuses (spaces, empty items, duplicates past the limit) | fixed with the parser |

With an empty host field a toggle press shows "Enter a host first." instead of appearing inert.

### Behavioural verification

The island runs in happy-dom (`crates/web/islands/test/ports.test.ts`, 26 tests) and the reviewer
drove the real `shell.ts` and `ports.ts` in headless Chromium (scratchpad `pc/`), which found the
two blockers above; the browser pass over the finished page is Phase 6's design check.

## Phase 5 — Copy and certificate changed

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R16 Subject, Alternative names, Issuer, Serial, SHA-256 and SHA-1 carry the shell's copy button with the full value (SANs newline separated) | green | crates/web/tests/it/views_tls_copy.rs |
| C2 | R17 the certificate section carries the leaf's facts and key; the island stores a snapshot per key and shows a banner naming the changes when the SHA-256 differs; Forget; storage failures harmless | green | crates/web/tests/it/views_tls_copy.rs, crates/web/islands/test/certdiff.test.ts |
| C3 | R18 no JavaScript: copy buttons and the banner hidden; bundle under budget; islands write no style | green | crates/web/tests/it/views_tls_copy.rs, crates/web/islands/test/budget.test.ts |
| C4 | S the facts grid's copy buttons hold the full values, hidden until the shell island shows them; a press writes the value | green | crates/web/tests/it/views_tls_copy.rs |
| C5 | S a stored snapshot with another SHA-256, another not_after and one SAN fewer: the banner names the expiry change and the added name | green | crates/web/islands/test/certdiff.test.ts |
| C6 | S a stored snapshot with the same SHA-256: no banner | green | crates/web/islands/test/certdiff.test.ts |
| C7 | S only the fingerprint differs: same names and issuer, new key | green | crates/web/islands/test/certdiff.test.ts |
| C8 | S Forget removes the snapshot and hides the banner | green | crates/web/islands/test/certdiff.test.ts |
| C9 | S storage that throws: no banner, no error | green | crates/web/islands/test/certdiff.test.ts |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | opus | 70229 | 170 |

Planned by the orchestrator. Two test fixtures were wrong (an unbracketed IPv6 block id; a remount
that kept the first mount's DOM) and were corrected; the review fixes were made by the orchestrator.

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | a port with several leaves gave every section the same key, so the snapshots overwrote each other and banners were false | requirement 17 amended and fixed: the banner needs one distinct chain |
| AMENDMENT | the memo outlived a run, so later runs in the page neither compared nor stored | fixed: no memo (TLS parts are not re-sent since Phase 3); test replaced (ADLC-Test-Change) |
| AMENDMENT | a shared run compared a stored certificate and rolled the snapshot back | requirement 17 amended and fixed: only a live run compares; test added |
| NIT | expiry named for the same date | fixed: dates compared; test added |
| NIT | key case followed the typed host | fixed: lower-cased; test added |
| NIT | changed tests need trailers | on the test commit |

### Behavioural verification

`just design-check` with this branch's build at the main checkout's target path rendered the live
TLS inspector for `1.1.1.1:443` with the copy buttons beside Subject, Alternative names, Issuer,
Serial, SHA-256 and SHA-1 (`target/design/d3-tls-inspector-light-page.png`); the banner logic runs
in happy-dom (`certdiff.test.ts`, 17 tests).

## Phase 6 — Design gate and visual reading

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R19 the manifest measures G's and GDark's port chooser, addresses table, certificate section, protocols grid; design-check exits 0; the reader's findings are rows and closed; 390 px without sideways scroll | green | design-check, reading below |
| C2 | S design-check exits 0 with the TLS elements of G and GDark | green | design-check |
| C3 | S every finding of the reading beside board G is a report row and none stays open | green | reading below |
| C4 | S the TLS inspector at 390 px is not wider than the viewport | green | scratchpad `tlsui/shots.mjs` |

### Design check

`just design-check` with this branch's build at the main checkout's target path (scratchpad
`tlsui/dc.sh`; a worktree binary gets no network here): `0 open deviation(s), 0 stale accepted
entries`; 14 boards, 1026 elements, 1925 properties in the manifest. New rows, 35 per board on G
and GDark (netray.info `specs/design/canvas/manifest.json`): ports label, pressed and offered port
toggles, the add button, the addresses head and address cell, validity label, bar and caption, the
certificate facts grid with label and value, a chain pill, the trust text, a protocols cell with
name and state. The related box is not measured: the board's live target is an IP address
(`1.1.1.1:443`), which has no DNS records to relate. Rebuilt during the check: the protocols cell
name weight 600 (board).

### Visual reading beside board G

A reader with no part in the build compared the live run of `1.1.1.1:443` (1280 and 390 px, light
and dark), a two-port live run (`1.1.1.1:443,853`), and stored runs with differing leaves and an
expired leaf (evidence view, crafted fixtures) with boards 17 and 18.

| id | finding | status | disposition |
|---|---|---|---|
| V1 | "Ports" sat about 23 px below "Host" | green | rebuilt: the label stands on the Host label's line; at phone width it returns above the toggles |
| V2 | the pressed toggle's bar is its left border, not an inset bar | green | kept: the marker is one rule in base.css (frontend-rules §4); an inset bar would be a second marker |
| V3 | the addresses table sat inset in a padded card | green | rebuilt: the table runs to the card's edges |
| V4 | a note "Port 443 · handshake …" and the feeds line under the table | green | kept: requirement 4 (nothing lost) and v2-http-ui requirement 2 |
| V5 | the address in a lighter weight with an underline | green | rebuilt: weight 700; the underline is the shell's address link (v2-inspector-shell requirement 19) |
| V6 | copy buttons opened gaps in the facts' rhythm | green | rebuilt: at desktop width the copy control sits on the value's line; 44 px only at phone width |
| V7 | the trust text on its own line | green | rebuilt: it follows the last pill in the chain row |
| V8 | crt.sh links on pills and the dashed trust-store pill | green | kept: requirement 9 (3.14, 3.19) |
| V9 | the "Chain certificates" disclosure | green | kept: requirement 9, nothing lost |
| V10 | the protocols grid sat inset in a padded card | green | rebuilt: the grid runs to the card's edges |
| V11 | cells in a row with a wrapped cell pushed their state to the bottom | green | rebuilt: name and state stay together, centred as on the board |
| V12 | no ECH cell for the IP target | green | kept: the HTTPS lookup failed (DNS blocked locally), so `ech` is absent (Rule M) |
| V13 | Forward secrecy carries a neutral bar | green | kept: `weak_suites` warns on CBC, and a bar never contradicts its cell (requirement 10) |
| V14 | an unfilled last row | green | kept: the probe data adds cells beyond G's eight (decision log); the remainder is the card's surface |
| V15 | the "Probe answers" disclosure | green | kept: requirement 10, nothing lost |
| V16 | no related box in the evidence view | green | not this view: the box is the inspector's (requirement 12); the evidence view frame is v2-inspectors' |
| V17 | an unstyled "Explore live" address list in the evidence view | green | not this feature: the evidence view frame |
| V18 | a lone "HTTPS" link above Addresses in the evidence view | green | not this feature: the shared-evidence line (R29) of the evidence view |
| V19 | the notices banner nearly touched the check list in a panel | green | rebuilt: the banner keeps a section gap above it |
| V20 | the addresses table is cut at 390 px | green | kept: it scrolls inside its card, as board G's table does (`overflow-x: auto`, min-width 720 px) |
| V21 | wide gaps between wrapped SAN links at 390 px | green | kept: each SAN is a link with the shell's 44 px phone target |
| V22 | an arrow dangled at a row's end | green | rebuilt: the arrow hangs before its pill, outside the border, so a row never ends in one |
| V23 | the centre validity label reached the card padding at 390 px | green | kept: within the card; no overflow (scroll width 390) |
| V24 | an expired bar still showed the amber caption | green | rebuilt: no caption for an expired leaf |
| V25 | the HSTS link wraps | green | kept: the cell's state is the link text |

Rebuilt after the reading; `just design-check` again: `0 open deviation(s), 0 stale accepted entries`.

### Behavioural verification

Document width and tap targets (Playwright, scratchpad `tlsui/shots.mjs`, lens on the main
checkout's target path):

```
live 1280 light/dark: scrollWidth 1280 viewport 1280
live 390 light/dark: scrollWidth 390 viewport 390
two-ports 1280 light: scrollWidth 1280 viewport 1280
differs 1280/390 light/dark: scrollWidth = viewport
expired 1280/390 light/dark: scrollWidth = viewport
```

The crt.sh links measured 13 px tall at 390 px and now have a 44 px phone target. A three-port run
with a STARTTLS port needs DNS for a mail host, blocked locally; the panels are verified with the
two-port live run and `inspect_tls_panels.rs`.

### Final review fixes

The full-range review (f8ecf55d..d9aa2d11) held one blocker (a leaf lapsed within the day drew a
valid bar) and six majors; the fixes and amendments are in `review.md`, and a second reading of the
fix range held four majors, fixed in turn. Two-port live run after the tab tone bar:

```
two-ports 1280 light/dark: scrollWidth 1280 viewport 1280
two-ports 390 light/dark: scrollWidth 390 viewport 390 (no target below 44 px)
```
