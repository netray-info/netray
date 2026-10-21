# Spec: v2-tls-ui

Status: Ready for Implementation
Created: 2026-10-20

## Goal

The TLS inspector looks like board G (canvas boards 17/18) below the shell and shows what the TLS
backend puts into the evidence (inventory theme E, UI half): the addresses table, the certificate
section (validity bar, facts grid with serial, SHA-1, policy and OCSP, the live OCSP chip, chain
pills with crt.sh links, copy per field), the "Protocols and features" grid built from the probe
data of v2-tls-versions-coop and v2-tls-probe-close, the notices banner, the related-records
summaries, one tab per inspected port with that port's band and checks, the port chooser with
presets, and a banner when the certificate changed since the viewer's last visit. `just
design-check` exits 0 with G's certificate section, protocols grid, addresses table and port
chooser measured.

Closes inventory rows 3.3, 3.4, 3.5, 3.9, 3.16, 3.17, 3.18, 3.19, 3.20, 3.23, 3.24, 3.25, 3.6
(STARTTLS marker), the finding "Board D3 shows a Protocols and features grid that nothing backs",
and canvas rows 9.50, 9.55, 9.56, 9.57, 9.58 (summaries).

## Non-goals

- The shell parts of board G (header, inspector tabs, form card, run title, h1, back link, slim
  band, section headings, related-records link, IP badges and links): `v2-inspector-shell` (#46)
  and `v2-http-ui` (#49), merged.
- New checks, weights, catalogue texts or backend blocks. The domain run keeps judging 443 (SDD
  §3.5).
- Rows 3.10 (total duration), 3.22 (ALPN "none"), 3.27 (CAA "issuer allowed" badge) and 3.31
  beyond the leaf marker: changed rows, not rebuilt.
- Exports of a multi-port run keep today's shape (the band port's checks); per-port checks in the
  exports are a proposal.
- CRL checking.

## Context and constraints

- Inventory (private, `netray.info/specs/inventory/sections/03-tls.md`, `09-canvas.md`); operator
  decision 2026-10-19: theme E "rebuild (multi-port charges only ports it shows)"; canvas rows not
  named otherwise are errors, rebuilt to the board.
- Backend, merged: `v2-tls-backend` (#34), `v2-tls-versions-coop` (#43), `v2-tls-probe-close`
  (#48). Register (`crates/tls/src/module.rs` `REGISTER`), each key only when its fact exists:
  - `tls.addresses` (table: address, port, outcome `ok` or an error code, message).
  - `tls.chain@<addr>:<port>` (chain, served order): position (`leaf`, `leaf_self_signed`,
    `intermediate`, `root`, …), subject, issuer, sans (repeated), not_before, not_after,
    days_remaining, key, signature_algorithm, fingerprint_sha256, fingerprint_sha1, serial, policy
    (EV, OV, DV), self_signed, expired, ocsp_url, ca_issuers_url.
  - `tls.handshake@<addr>:<port>` (fields): version, cipher_suite, alpn, key_exchange_group, sni,
    starttls (`smtp`), ocsp_stapled, ocsp_status, ocsp_this_update, ocsp_next_update,
    ocsp_live_status, ocsp_live_revoked_at, ocsp_live_reason (live keys only when the responder
    answered), ech (only when the HTTPS lookup did not fail), handshake_ms.
  - `tls.probes@<addr>:<port>` (fields): ssl3, tls1_0, tls1_1, tls1_2, tls1_3 (`enabled`,
    `disabled`), suites_insecure, suites_cbc, suites_static_rsa (`accepted`, `refused`), each with
    `<key>_answer` (the server's answer); an unanswered probe is omitted.
  - `tls.protocols` (fields, per port): the same probe keys, `mixed` when addresses differ, and
    forward_secrecy_all_suites (bool).
  - `tls.ct` (table: address, sct_count, log_ids), `tls.tlsa@<port>` and `tls.caa` (records),
    `tls.notices` (fields: warning, skipped_address; band port only).
  - With more than one port, `tls.addresses`, `tls.protocols` and `tls.ct` carry `@<port>`.
  - Checks (catalogue): tls_reachable, chain_trusted, not_expired, hostname_match, chain_complete,
    strong_signature, key_strength, expiry_window, cert_lifetime, san_quality, aia_reachability,
    tls_version, forward_secrecy, aead_cipher, legacy_versions, weak_suites, ocsp_stapled,
    ct_logged, caa_compliant, dane_valid, consistency, alpn_consistency, ech_advertised.
- The job (`crates/web/src/api/jobs.rs` `start_tls`, `port_jobs`) computes one outcome per port;
  it streams one part per port (`handshake` for one port, `port <n>` for more) and one band (the
  band port, 443 else the lowest). Only the band reaches the parts' views today (v2-http-ui
  requirement 4 re-streams parts at band end). `limits.max_ports` default 7.
- The protocol-view seam (`crates/web/src/pages/views/`, v2-http-ui requirement 1): every
  renderer (inspector parts, evidence views, the result page's live "All evidence") asks per
  block, in block order; a view claims a block only when it renders every key (requirement 3,
  "nothing lost"). `facts_grid` component, slim band and the protocol-view frame exist.
- V1 reference: `crates/tls/frontend/src/components/HostInput.tsx` (presets HTTPS `:443`, E-Mail
  `:25,465,587,993,995`, All Common `:443,25,465,587,993,995,8443`), `PortTabs.tsx`,
  `CertDetail.tsx` (validity timeline, copy buttons, crt.sh link
  `https://crt.sh/?q=<sha256 hex without colons>`, localStorage certificate diff), `TlsParams.tsx`
  (OCSP live badge), `ChainView.tsx` (inferred root "from trust store").
- `check_expiry_window` (`crates/tls/src/quality/checks.rs`) warns at 30 days or fewer, a literal
  today. `chain_trusted` verifies against `webpki-roots` (Mozilla's root program).
- Board G (private, `design/canvas/G.dc.html`): form card with "Ports" (443 pressed, 8443, "+",
  44 px each); h2 sections (20 px, 650) over cards; "Addresses" table (Address weight 700, Band
  mini spectrum, Protocol, Cipher, Leaf `SHA256 4f:9a:…:e1` + "same"); "Certificate" card:
  validity bar (labels mono 12 px, bar 10 px, today marker 3 px, amber zone, caption mono 11 px),
  facts grid of eight, "Chain" pills (mono 13, padding 6 12, 1 px line border) with arrows and
  "trusted by all major stores" (12 px, pass colour); "Protocols and features" grid
  (`repeat(auto-fill, minmax(250px, 1fr))`, 1 px gaps on the line colour, cells padding 12 16,
  bar 3/5/6 px × 22 px, name 14 px 600, state mono 12 px muted); dashed related-records box.
- SDD: §3.7 (data never presentation), §6.1 (evidence view order, anchors, feeds), §6.2 (TLS
  inspector), §8.3, §8.4 (islands read data attributes, compute no layout, hold no verdict;
  bundle under 30 KB gzipped), R5 (views compute no verdict).
- Gate rules: CLAUDE.md, `frontend-rules.md`, `design-rules.md`. Layout geometry is computed in
  Rust, never in a template or island.
- Locally only a binary at the main checkout's target path reaches the network (Little Snitch);
  the design check's live TLS board needs it (Decision log).

## Requirements

### The port view

1. **Port view seam.** The seam gains the sibling blocks the renderer shows with the block
   (`views::render_with(block, ctx, siblings)`; `render` passes none)
   (the inspector part's blocks; the run's blocks of the block's protocol in the evidence view and
   the live details). A view may render several blocks in one fragment: the TLS view renders the
   whole port at `tls.addresses` (or `tls.addresses@<port>`), and every block it renders there
   answers the seam with an empty fragment, so it appears once. Whether a block is taken is one
   pure function of the sibling blocks, used by both. The fragment carries an anchor
   `id="ev-<block id>"` for every block it renders; its feeds line is the addresses block's (the
   check rows keep linking to the taken blocks' anchors). A block the port view does not take (TLSA,
   CAA, an older stored block with an unknown key) renders as before. `evidence.rs` still names
   no module, block or key. SDD §6.1 amended.
2. **Nothing lost.** The port view takes a block only when it renders every value of it, read as
   follows: `outcome` `ok` is the row having handshake columns; `self_signed` and `expired` false
   are the absence of their chip; `position` orders the chain; `port` is in the addresses note.
   A test feeds a port of two addresses with every registered key and asserts each value's
   display text appears.
3. **Distinct values.** Where one cell shows a fact several addresses report (handshake facts,
   OCSP, SCTs), equal values held by every address that completed a handshake show once;
   otherwise (values differ, or some of those addresses do not hold the fact) each value is
   followed by its addresses in parentheses. An address without a handshake is its error row.

### Addresses

4. **Addresses table.** A section "Addresses" (h2 style of v2-http-ui) with a table: Address
   (mono, weight 700, the shell's address link), Band (the port's band as a mini spectrum: one bar
   per TLS line in catalogue order, height and width from the same Rust layout as the slim band,
   status colours; with no checks yet, no bars), Protocol (`version`), Cipher (`cipher_suite`),
   Leaf (`SHA256 ` and the first two and last byte of the leaf's fingerprint in lower case with an
   ellipsis, the full value in a `title`) followed by "same" in the pass tone when the leaf equals
   the port's first leaf, else "differs" in the warn tone; the markers show only when at least two
   rows hold a leaf. A row whose outcome is not `ok` shows the outcome and message across the
   remaining columns. Under the table a muted note: "Port <n> · handshake <d>" with the display
   duration per address (requirement 3). (9.55, 3.21)

### Certificate

5. **One section per distinct chain.** A section "Certificate" per distinct served chain of the
   port, in address order; with more than one, each heading reads "Certificate · <addresses>".
6. **Validity bar.** An SVG bar (viewBox width 1000, height 10) whose geometry comes from one pure
   Rust function over not_before, not_after and now, where now is the run's moment
   (not_after minus the stored `days_remaining`, so a stored run reads as measured; the wall clock
   only when `days_remaining` is absent): the elapsed part in the pass tone, the last
   `EXPIRY_WARN_DAYS` days as the warn zone, a 3 px today marker; labels (mono 12 px muted) "not
   before <date>" left, "today · <n> days left" centred, "not after <date>" right; the caption
   (mono 11 px) "the last <n> days turn the expiry line amber". `EXPIRY_WARN_DAYS` is one public
   constant in netray-tls that `check_expiry_window` reads (the literal 30 moves there). An expired
   leaf draws the whole bar in the fail tone and the centre label reads "expired <n> days ago"; a
   leaf not yet valid reads "not valid for <n> days" with the marker at the start. (3.17, 3.30)
7. **Facts grid.** A `facts_grid` with board G's eight cells first, then the further facts:
   Subject, Alternative names (comma-joined; a DNS name or address carries the shell's link),
   Issuer, Key, Signature, OCSP, Certificate Transparency ("<n> SCTs" from `tls.ct`, absent without
   it), Serial; then SHA-256, SHA-1, Policy, OCSP responder (`ocsp_url`), CA issuers
   (`ca_issuers_url`), SCT logs (`log_ids`). A fact the block does not hold is not shown. True
   `self_signed` and `expired` show as fail-tone chips "self-signed", "expired" in the Subject
   cell. (3.15, 3.16, 3.24)
8. **OCSP cell.** "stapled · <ocsp_status> · <date of ocsp_this_update>" (parts present only when
   held) or "not stapled"; "fresh until <date>" from `ocsp_next_update`; and, when
   `ocsp_live_status` exists, a chip by one fixed map: "live: good" pass tone, "live: revoked
   <date> · <reason>" fail tone, "live: unknown" warn tone; without the key, no chip (Rule M).
   (3.24, 3.25)
9. **Chain pills and crt.sh.** A row "Chain": one pill per served certificate in served order,
   naming its subject CN (else the subject), arrows between; when `tls.chain_trusted` passes and
   the last served certificate is known not to be self-signed (`self_signed` false, an issuer),
   a dashed pill naming its issuer's CN and "from the trust store". After the pills, with one
   distinct chain on the port, `tls.chain_trusted`: on pass "trusted by <TRUST_STORE>" (pass
   tone), where `TRUST_STORE` is one netray-tls constant naming the root set the check uses; on
   warn or fail the check's catalogue finding in its tone (a view holds statuses, not findings;
   the check list carries the run's finding); nothing when unknown. With several distinct chains
   the port-wide verdict is not printed under each. Each served pill links "crt.sh ↗" to
   `https://crt.sh/?q=<fingerprint_sha256 without colons>`. The non-leaf certificates' facts
   (requirement 7's keys) sit in a `details` "Chain certificates" under the row. (3.14, 3.19)

### Protocols and features

10. **Protocols grid.** A section "Protocols and features": board G's cell grid (one component of
    `pages.css` classes, not `facts_grid`), cells of a tone bar, a name and a state, in this order:
    TLS 1.3, TLS 1.2, TLS 1.1, TLS 1.0 (from `tls.protocols`: "enabled", "disabled", "mixed";
    TLS 1.1 and 1.0 "disabled, as it should be"), ALPN (the protocols, "none" without), Forward
    secrecy ("all suites" or "not all suites"), Encrypted Client Hello ("advertised" or "not
    advertised"), HSTS ("checked in the HTTP inspector", linking `/inspect/http?url=https://<sni>/`,
    only when the handshake holds `sni`: an IP target has no host to check, and a stored run
    written before `sni` existed names none); then SSL 3.0, Cipher suites ("insecure <w> · CBC <w>
    · static RSA <w>"), Key exchange, SNI, STARTTLS. A cell whose fact is absent is not shown. Each
    bar is the status of the check that judges the fact (TLS 1.3 and 1.2 `tls_version`; SSL 3.0,
    TLS 1.1, 1.0 `legacy_versions`; Forward secrecy and Cipher suites `weak_suites`; ECH
    `ech_advertised`; ALPN `alpn_consistency`), none for a fact no check judges; a bar never
    contradicts its cell: a cell in its good state (a legacy version disabled, all suites, every
    suite group refused) shows no warn or fail bar, and a cell in its bad state shows no pass bar.
    The per-address probe answers sit in a `details` "Probe answers" table (a row per probe, a
    column per address, the word and the answer). (9.57, 3.21, 3.23, 3.6, 3.29)
11. **Notices banner.** When the port holds `tls.notices`, a warn-tone banner above the addresses:
    "<n> addresses not inspected: the inspector checks at most <m> addresses per host" when
    skipped addresses exist (<m> the number of inspected addresses), listing them with the shell's
    address links, then each `warning` as its own line. (3.9)
12. **Related records.** The inspector's related-records box reads, from the band port's blocks:
    "CAA (<n> records)" or "CAA (none published)" from `tls.caa`, "TLSA (<n>)" or "TLSA (none)"
    from `tls.tlsa@<port>`, "HTTPS (ech=)" or "HTTPS (no ech=)" from the handshake's `ech`; a block
    or key that is absent reads "(not measured)". It is filled when that part arrives (every entry
    "(not measured)" when the part failed or timed out) and in a shared run. (9.58)

### Ports

13. **Per-port checks.** Each TLS part carries its port's checks; a part renders its views with its
    own checks when it arrives (no re-stream needed for it), and a shared run stores and renders
    them.
14. **Port tabs and panels.** With more than one inspected port, the result region holds a tab row
    under the h1 and one panel per port in input order; the panel holds that port's band and check
    list (the shell's band region, ids suffixed by the part number) and its port view; no band sits
    under the h1. A tab names the port, adds "· STARTTLS" when the port's handshake holds
    `starttls`, and carries the port's worst status as a tone and "<f> fail · <w> warn" when a line
    warns or fails; otherwise unmeasured lines read "<u> not measured" without a tone (Rule M), and
    a port whose lines pass carries the pass tone; both arrive with the part. The band port's tab is current. The `tabs` island
    switches panels; without JavaScript all panels render in order and the tabs are in-page links.
    With one port there is no tab row (board G). (3.3, 3.5)
15. **Port chooser.** In the TLS form card, between the host box and "Inspect", a group "Ports"
    (12 px label): one 44 px toggle per offered port (mono 14 px, `aria-pressed`, pressed shows the
    marker and weight 700) and a "+" button (44 px, dashed border) opening a list with the presets
    "HTTPS" (443), "E-Mail" (25, 465, 587, 993, 995), "All common" (443, 25, 465, 587, 993, 995,
    8443) and a field for one more port (1 to 65535). Default: 443 pressed, 8443 offered. The host
    field is the source of truth: a toggle rewrites its port suffix (`example.com:443,465`; only
    443 pressed leaves the bare host; an IPv6 host in brackets), and typing `host:25,587` presses
    25 and 587; the chooser re-reads the field before every action, on submit and on history
    navigation (the shell writes the field without an input event), and reads a suffix as ports
    only as the server's parser does (a list it would refuse presses nothing). The extra-port field
    never blocks the form's submit. The last pressed
    port cannot be released. At most `limits.max_ports` (a data attribute) are pressed; one more is
    refused with a visible note naming the limit. The group is rendered `hidden` and shown by its
    island, so without JavaScript it is absent; the help names the `host:443,465` syntax. (3.4,
    9.50)

### Copy and changes

16. **Copy per field.** Subject, Alternative names, Issuer, Serial, SHA-256 and SHA-1 carry a copy
    button (rendered `hidden`, shown by its island) that copies the full value (SANs newline
    separated). (3.18)
17. **Changed since last visit.** In the inspector, the certificate section of a port serving one
    distinct chain carries the leaf's not_after, issuer, SANs and SHA-256 in data attributes and a
    key `<sni, lower-cased, or first address>:<port>`; only a live run of the page compares (a
    shared run renders a stored certificate, no visit of the live one), and "expiry" names a
    changed date.
    After a run its island stores them per key in `localStorage` (`netray.cert.<key>`); on a later
    run of the key, when the SHA-256 differs, a banner from a server-rendered `template` at the
    top of the certificate section names the changes: "expiry <old> → <new>", "issuer <old> → <new>", "names added: …",
    "names removed: …", or "same names and issuer, new key"; "Forget" removes the snapshot and the
    banner. Storage failures leave the page working without the banner. (3.20)

### Acceptance

18. **No JavaScript and budget.** Without JavaScript tabs are in-page links, the chooser, copy
    buttons and banner are absent; the islands bundle stays under 30 KB gzipped; islands write no
    style and compute no verdict. (S7, §8.4)
19. **Design gate.** The private manifest measures G's and GDark's port chooser, addresses table,
    certificate section (validity labels and bar, facts grid, chain pills, trust text) and
    protocols grid (the related box needs a host target; the board's live target is an IP
    address); `just design-check` exits 0, every row fixed or accepted. Screenshots at 1280 and
    390 px, both themes, of the live board target and of the stored states (two addresses with
    differing leaves, an expired leaf) and a live run of several ports (STARTTLS needs DNS for a
    mail host, blocked locally) are read beside G
    by a reader with no part in the build; every finding is a `report.md` row and closed. At 390 px
    nothing scrolls sideways. (9.50, 9.55–9.58)

## Phase 1 — Seam, addresses and certificate

**Depends on:** v2-http-ui merged
**Requirements:** 1, 2, 3, 4, 5, 6, 7, 8, 9

### Test Scenarios

GIVEN a TLS part of two addresses serving the same chain WHEN rendered THEN the fragment holds one Addresses section and one Certificate section, both rows show "same", and the chain and handshake blocks render nothing of their own.
GIVEN two addresses serving different leaves WHEN rendered THEN the second row shows "differs" and two Certificate sections follow, headed by their addresses.
GIVEN the port view WHEN rendered THEN it carries `id="ev-tls.addresses"` and an anchor `ev-<id>` for every chain and handshake block it took.
GIVEN a stored `tls.chain` with an unknown key WHEN rendered THEN that block renders through the generic block and the addresses table still renders.
GIVEN a row with outcome `HANDSHAKE_TIMEOUT` and its message WHEN rendered THEN the row shows both across the remaining columns.
GIVEN handshakes of 22 ms and 31 ms on port 443 WHEN rendered THEN the note reads "Port 443 · handshake 22 ms (…) · 31 ms (…)".
GIVEN not_before 2026-10-07, not_after 2027-01-06 and now 2026-10-09 WHEN the validity layout runs THEN the marker sits at 2 of 91 days, the warn zone covers the last 30 days and the label reads "today · 89 days left".
GIVEN an expired leaf WHEN rendered THEN the bar is in the fail tone and reads "expired <n> days ago".
GIVEN `check_expiry_window` WHEN read THEN it compares against `EXPIRY_WARN_DAYS`.
GIVEN a leaf with serial, SHA-1, policy OV, OCSP and CA issuers URLs WHEN the facts grid renders THEN Subject, Alternative names, Issuer, Key, Signature, OCSP, Certificate Transparency and Serial come first, then SHA-256, SHA-1, Policy, OCSP responder, CA issuers and SCT logs.
GIVEN a leaf without policy and serial WHEN rendered THEN neither cell exists.
GIVEN a stapled response good with this_update 2026-10-08 WHEN rendered THEN the OCSP cell reads "stapled · good · 2026-10-08".
GIVEN `ocsp_live_status` revoked with a date and reason WHEN rendered THEN the chip reads "live: revoked <date> · <reason>" in the fail tone; GIVEN no live key THEN no chip.
GIVEN a leaf with SHA-256 `4F:9A:…:E1` WHEN its pill renders THEN its crt.sh link is `https://crt.sh/?q=4F9A…E1`.
GIVEN a served chain of leaf and intermediate WHEN rendered THEN a dashed pill names the intermediate's issuer with "from the trust store", `tls.chain_trusted` pass shows "trusted by" and `TRUST_STORE`, and the intermediate's facts sit in "Chain certificates".
GIVEN a port of two addresses with every registered key WHEN the nothing-lost test runs THEN every value's display text appears in the fragment.

## Phase 2 — Protocols, notices and related records

**Depends on:** Phase 1
**Requirements:** 10, 11, 12

### Test Scenarios

GIVEN `tls.protocols` with TLS 1.3 and 1.2 enabled, 1.1 and 1.0 disabled, forward secrecy for all suites, and a handshake with ALPN h2, ECH false and SNI `example.com` WHEN rendered THEN the first eight cells read TLS 1.3 enabled, TLS 1.2 enabled, TLS 1.1 and TLS 1.0 "disabled, as it should be", ALPN h2, Forward secrecy "all suites", Encrypted Client Hello "not advertised" and HSTS linking `/inspect/http?url=https://example.com/`.
GIVEN `ech_advertised` warn and `legacy_versions` pass WHEN rendered THEN the ECH bar is warn and the TLS 1.1 and 1.0 bars pass; the HSTS cell carries no tone.
GIVEN a handshake without `sni` WHEN rendered THEN no SNI and no HSTS cell exists.
GIVEN a handshake with starttls smtp WHEN rendered THEN a STARTTLS cell reads "smtp"; GIVEN none THEN no STARTTLS cell.
GIVEN no `tls.protocols` WHEN rendered THEN no version cell exists.
GIVEN two `tls.probes` blocks WHEN rendered THEN "Probe answers" holds a column per address with each word and answer.
GIVEN `tls.notices` with one warning and two skipped addresses beside four inspected ones WHEN rendered THEN the banner reads "2 addresses not inspected: the inspector checks at most 4 addresses per host", lists both and shows the warning; GIVEN no notices THEN no banner.
GIVEN `tls.caa` with two records and no TLSA block WHEN the related box renders THEN it reads "CAA (2 records)" and "TLSA (not measured)"; GIVEN an empty TLSA block THEN "TLSA (none)".
GIVEN the protocols, probes and notices fixtures WHEN the nothing-lost test runs THEN every value's display text appears.

## Phase 3 — Per-port parts and tabs

**Depends on:** Phase 1
**Requirements:** 13, 14

### Test Scenarios

GIVEN a stub run of `example.com:443,465,587` WHEN the stream ends THEN three tabs read "443", "465" and "587 · STARTTLS", the 443 tab is current, each panel holds its own band and its check list, and no band sits under the h1.
GIVEN port 465 failing its handshake WHEN its part arrives THEN the 465 tab carries the fail tone and "<f> fail", and the 443 tab does not.
GIVEN a one-port run WHEN rendered THEN no tab row exists and one band sits under the h1.
GIVEN a TLS part WHEN it arrives THEN its views carry its own port's check tones and the stream does not re-send it at band end.
GIVEN a three-port run WHEN its share key is opened THEN the panels render with their own bands and tones.
GIVEN no JavaScript WHEN a three-port shared run is fetched THEN all panels render in order and the tabs are in-page links.

## Phase 4 — Port chooser

**Depends on:** Phase 1
**Requirements:** 15, 18

### Test Scenarios

GIVEN the idle TLS page WHEN rendered THEN the form card holds a hidden "Ports" group with 443 pressed, 8443 unpressed and "+", and `data-max-ports="7"`; other inspectors hold no group.
GIVEN the chooser island WHEN 465 is added and host `example.com` typed THEN the field reads `example.com:443,465`.
GIVEN the field `example.com:25,587` WHEN typed THEN 25 and 587 are pressed and 443 is not.
GIVEN the "+" list WHEN "E-Mail" is chosen THEN 25, 465, 587, 993 and 995 are pressed.
GIVEN a limit of 7 and seven pressed ports WHEN an eighth is pressed THEN it stays unpressed and the note names the limit.
GIVEN one pressed port WHEN it is pressed again THEN it stays pressed.
GIVEN the islands bundle WHEN built THEN the budget test passes.

## Phase 5 — Copy and certificate changed

**Depends on:** Phase 1
**Requirements:** 16, 17, 18

### Test Scenarios

GIVEN the facts grid WHEN rendered THEN Subject, Alternative names, Issuer, Serial, SHA-256 and SHA-1 carry a hidden copy button holding the full value; the island shows them and a press writes the value to the clipboard.
GIVEN a stored snapshot for `example.com:443` with another SHA-256, another not_after and one SAN fewer WHEN a run ends THEN the banner names the expiry change and the added name.
GIVEN a stored snapshot with the same SHA-256 WHEN a run ends THEN no banner shows.
GIVEN only the fingerprint differs WHEN a run ends THEN the banner reads "same names and issuer, new key".
GIVEN the banner WHEN "Forget" is pressed THEN the snapshot is removed and the banner hides.
GIVEN storage that throws WHEN a run ends THEN no banner shows and no error escapes.

## Phase 6 — Design gate and visual reading

**Depends on:** Phases 1–5
**Requirements:** 19

### Test Scenarios

GIVEN the manifest's G and GDark TLS elements WHEN `just design-check` runs THEN it exits 0.
GIVEN the screenshots WHEN read beside board G THEN every finding is a `report.md` row and none stays open.
GIVEN the TLS inspector at 390 px WHEN measured THEN the document is not wider than the viewport.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- Board G's protocols grid stays as drawn, backed by the probe data of v2-tls-versions-coop and
  v2-tls-probe-close (operator, 2026-10-20; the draft's open decision is resolved by #43 and #48).
- Re-check against main 2026-10-21: v2-http-ui (#49) built the per-block seam; board G groups
  several blocks per section (addresses with handshakes and leaves, OCSP from the handshake in the
  certificate), so the seam gains sibling blocks and one view renders a whole port at its
  addresses block, the other blocks answering empty. The alternative, one section per block,
  repeats the handshake facts and breaks G's order (implementer under the operator's autonomy
  mandate, 2026-10-21).
- Phases reordered: the board's sections first (Phases 1, 2), then tabs, chooser, copy and the
  changed banner; the draft's port-tab phase depended on nothing of them (implementer under the
  operator's autonomy mandate, 2026-10-21).
- Per-port checks travel with each part, so a part of port 465 shows 465's tones; the job's band
  stays the band port's for the exports. With several ports the band moves into each panel and
  none sits under the h1, so a page never shows two bands for one port (drafting agent,
  2026-10-20; implementer, 2026-10-21).
- Port tabs only with more than one port, so the one-port page is board G (drafting agent,
  2026-10-20).
- Presets sit in the "+" list: board G draws only toggles and "+", and V1's three presets keep
  their ports. The host field is the source of truth for the chooser, so the stream island's
  submit (registered first on the form) always posts what the chooser shows (drafting agent,
  2026-10-20; implementer, 2026-10-21).
- The Band column of the addresses table shows the port's band in each row: the checks are judged
  per port (worst over addresses), no per-address status exists, and board G itself draws the same
  mini band in both rows (implementer under the operator's autonomy mandate, 2026-10-21).
- The amber zone's length is `EXPIRY_WARN_DAYS`, read by the check and the bar, so they cannot
  disagree (drafting agent, 2026-10-20).
- The facts grid keeps G's eight cells and order and appends SHA-256, SHA-1, Policy, OCSP
  responder, CA issuers and SCT logs (theme E names them; nothing lost); the protocols grid
  appends SSL 3.0, Cipher suites, Key exchange, SNI and STARTTLS for the same reason. A reader's
  "extra element" finding on them is answered by this decision (drafting agent, 2026-10-20;
  implementer, 2026-10-21).
- "trusted by all major stores" becomes "trusted by <TRUST_STORE>", the root set the check uses;
  "2 SCTs · embedded" loses "embedded" (`tls.ct` holds no SCT source); HSTS carries no tone bar
  (no TLS check judges it). Visual rows closed as deliberately different (drafting agent,
  2026-10-20; implementer, 2026-10-21).
- The changed banner compares server-written facts with the viewer's own snapshot: a difference,
  not a verdict (§8.4); browser-local because the server diff covers recorded domain runs only
  (drafting agent, 2026-10-20).
- The live OCSP chip's tone is one fixed map; an unanswered responder shows nothing (Rule M)
  (drafting agent, 2026-10-20).
- Phase 1 review: the validity bar reads the run's moment from `days_remaining`, so a stored run
  never shows an expiry the run did not measure; the trust text and the inferred root pill follow
  `chain_trusted` and appear only where they cannot contradict it (one distinct chain; a passing
  check); a fact some addresses lack is named per address (Rule M); "same"/"differs" need two
  leaves; the port view's feeds line is the addresses block's (implementer under the operator's
  autonomy mandate, 2026-10-21).
- Phase 2 review: SNI and HSTS show only when the handshake holds `sni` (stored runs older than
  the key named no SNI though they had a host); a protocol cell's bar never contradicts its own
  state (`legacy_versions` faults the versions it found enabled, not the disabled ones), and
  Forward secrecy reads `weak_suites`, which judges the static-RSA probe the cell shows; the
  related box reads "(not measured)" when the band port's part failed (implementer under the
  operator's autonomy mandate, 2026-10-21).
- Phase 3 review: a port whose lines are unmeasured (a closed 8443) reads "<u> not measured" on
  its tab without the pass tone (implementer under the operator's autonomy mandate, 2026-10-21).
- Phase 4 review: the chooser re-reads the host field before every action (the shell restores
  and fills it without input events), parses the suffix exactly as `crates/tls/src/input.rs`
  does, brackets an IPv6 host, and its extra-port field is a text field that cannot block the
  submit (implementer under the operator's autonomy mandate, 2026-10-21).
- Phase 5 review: the changed banner needs one distinct chain on the port (several leaves would
  share and overwrite one snapshot), compares only in a live run (not a shared `?r=` page), keeps
  no memo across runs, names expiry only for a changed date and lower-cases the host in its key.
  The copy buttons reuse the shell's `.inspector-copy` primitive that v2-ip-ui built (implementer
  under the operator's autonomy mandate, 2026-10-21).
- Phase 6 reading beside board G: rebuilt the Ports label's line, edge-to-edge table and grid,
  bold addresses, the copy controls' rhythm, the trust text in the chain row, centred protocol
  cells, hanging chain arrows, the banner's gap and no amber caption on an expired bar; kept the
  marker rule, the nothing-lost notes and disclosures, Rule M absences and the 44 px phone targets
  (report.md V1–V25) (implementer under the operator's autonomy mandate, 2026-10-21).
- Final review (f8ecf55d..d9aa2d11): the blocker (a leaf lapsed within the day drew a valid bar)
  is fixed on this branch rather than splitting the change; ALPN "none" needs a handshake read;
  a refused suffix presses no port; a port's tab shows its tone as a bar. Requirements 1, 3, 17
  and 19 amended to what was built and what is measurable here (implementer under the
  operator's autonomy mandate, 2026-10-21).
- The design check's live TLS board needs network: a binary built in a worktree gets none here,
  so `netray_bin` honours `NETRAY_BIN`, and the gate runs with this branch's build copied to the
  main checkout's target path (backed up and restored) (implementer under the operator's autonomy
  mandate, 2026-10-21).

## Open decisions

None.

## Out of scope

- Shell parts of board G, IP badges, links: `v2-inspector-shell`.
- Rows 3.10, 3.22, 3.27, 3.31 (beyond the leaf marker), CRL, per-port exports.
