# Spec: v2-result-polish

Status: Done
Created: 2026-10-20
Finished: 2026-10-20

## Goal

The follow-ups of the operator's live review of the result page (v2-result-fidelity, PR #42).
Every protocol tab of the Evidence section shows one compact board-style table of the facts that
decide the protocol's checks, with "All evidence →" for the rest, instead of every evidence block
(today the TLS tab is ~2000 px on the desktop and the phone page 6765 px against board M's 3041).
The hero keeps the grade descriptor and the CACHED marker, and the canvas boards F and M draw them.
The run title stays on one line at 1280 px, and a long domain breaks only at its dots. The design
gate gets fixture states for a long domain and for full multi-address TLS evidence, so it measures
what the review found by eye.

## Non-goals

- The inspector pages and the full evidence view `/r/<id>/<protocol>`: unchanged.
- Evidence blocks, their keys and the API: this feature changes no block, no register, no API
  response and no contract golden. The compact tables are a view over the stored blocks.
- The canvas export: the operator re-exports the canvas later; the rows that measure board
  changes made here wait for that export (report.md lists them).
- `v2-inspector-shell` (lane A, in parallel): its header and inspector parts.

## Context and constraints

- Board F (`F.dc.html`, private) draws the TLS tab: one caption line "Certificate chain · port 443
  · 2 addresses, consistent" (13 px, muted), a table (mono 13 px; header cells weight 500, muted,
  padding 6 px 12 px; body cells padding 8 px 12 px, a 1 px `line` rule above each row, the first
  column without left padding; the leaf row's Subject and Not after weight 700), and a line with
  the feeds text on the left and "Open in the TLS inspector →" on the right. Board M draws the same
  at 12 px with 10 px cell padding, a 520 px minimum table width and the swipe note.
- SDD §3.7: shapes carry data, never presentation; no module ships templates. The compact view is
  presentation, so it lives in `netray-web` beside `explore.rs`, which already names block ids
  and keys; it reads released keys only and invents no value (an absent key is an empty cell or an
  absent row, never a placeholder).
- The design gate (`specs/rules/design-rules.md`) measures computed values per element; it cannot
  measure a text's wrap point. A state the gate cannot measure is still rendered side by side for
  the reader.
- Fixtures stay in the `example.com` family and use documentation addresses only.

## Requirements

### Compact evidence tabs

1. **One compact table per tab.** For a run with a stored entry, each protocol's tab panel holds,
   in order: the unmeasured reason when there is one, a caption line, one table, the swipe note
   (phone), and the links line (feeds text, "All evidence →" to `/r/<id>/<protocol>`, the explore
   link). It holds no `evidence-block` section. A panel whose protocol has none of its compact
   source blocks holds no caption and no table, only the reason and the links line.
2. **Feeds of the table.** The feeds text names only the lines whose checks reference one of the
   compact table's source blocks, in catalogue order, as "<CODE> <short>" separated by " · ". A
   panel without a compact table names the lines its protocol's blocks feed (the current rule).
3. **Without a stored entry.** A live or unrecorded run has no entry view: its panel holds the
   compact table and, instead of the "All evidence →" link, a closed disclosure "All evidence"
   holding every block of the protocol through the shared evidence renderer.
4. **Long cells.** A cell value longer than 64 characters shows its first 63 characters and "…",
   and carries the full value in its `title` attribute.
5. **TLS.** Source: the `tls.chain` blocks of the band port (the port of the first `tls.chain`
   block in run order). Caption "Certificate chain · port <p> · <n> address[es], consistent"
   where `<n>` counts those blocks; when the chains' `fingerprint_sha256` sequences differ, the
   caption ends "<n> addresses, chains differ · showing <address>" naming the first block's
   address. Columns #, Subject, Issuer, Key, Not after; one row per item of the first block's
   chain: # the 0-based index; Subject the `CN=` value of `subject`, else the whole subject;
   Issuer "self-signed" when `self_signed` is true, else the `CN=` value of `issuer`, else the
   whole issuer; Key the `key` value, with a trailing bit size dropped when the key type already
   names its curve ("ECDSA P-256 256" reads "ECDSA P-256", "Ed25519 256" reads "Ed25519", "RSA 2048" stays); Not after the date
   part of `not_after`. The first row's Subject and Not after are strong. Without a chain block,
   the source is `tls.addresses`: caption "Addresses · <n> tested", columns Address, Port,
   Outcome, Message.
6. **DNS.** Source: the records blocks `dns.a`, `dns.aaaa`, `dns.ns`, `dns.caa`, `dns.ds`, in that
   order (reachability, delegation, CAA and the DNSSEC anchor: what the DNS checks and
   `tls.caa_compliant` judge). Caption "Key records · <k> record types answered", `<k>` counting
   the run's `dns.` records blocks with at least one record. Columns Type, Data, TTL, one row per
   record; a source block present with no record gives one row: its type, "no records", an empty
   TTL.
7. **HTTP.** Source: `http.server` and `http.headers`. Caption "Response" followed by " · <status
   code>", " · <http version>" and " · <final url>" for each key present. Columns Header, Value;
   rows `strict-transport-security`, `content-security-policy`, `x-frame-options`,
   `x-content-type-options`, `referrer-policy`, `permissions-policy` (the headers the response
   checks judge), each with its `http.headers` value or "not sent"; without `http.headers` no row.
8. **Email.** Source: `email.mx`, `email.spf`, `email.dmarc`, `email.mta_sts`, `email.tls_rpt`,
   `email.dkim`, `email.bimi`. Caption "Mail records · <n> MX host[s]", only "Mail records" without an
   `email.mx` block (an unanswered MX lookup claims no count). Columns Record, Value; rows
   in that order, each only when its block is present: MX "<preference> <host>" joined by ", ";
   SPF, DMARC, MTA-STS, TLS-RPT and BIMI one row per `record` value (a repeated record is the fact
   that fails its check); one DKIM row per key, "<selector>: <state>"
   followed by ", <key type> <bits>" when known (an empty value is unknown).
9. **IP.** Source: `ip.addresses`. Caption "Addresses · <n> address[es]". Columns Address, ASN,
   Org, Country, Network, Listed; Listed names the true flags among `spamhaus`, `c2`, `tor`,
   `vpn` joined by ", ", else "none"; an absent or empty cell stays empty.
10. **Styles.** Desktop: caption 13 px muted; table mono 13 px, header cells weight 500 muted with
    padding 6 px 12 px, body cells padding 8 px 12 px with a 1 px `line` top rule, first column
    without left padding, strong cells weight 700. Phone (≤ 480 px): table 12 px (token `font.size.evidence`, 13 px with a 12 px phone value; the 11 px phone label is not it), cell padding
    10 px sideways, minimum width 520 px inside the scrolling panel. (Boards F and M.)

### Hero

11. **Descriptor stays.** The grade descriptor under the score stays (operator decision) and no
    longer sets the grade block's width: it wraps within the width of the letter and score line.
12. **Run title on one line.** At 1280 px the run title of every stored state the fixtures hold,
    "… · CACHED · FIRST RUN" included, is one line (its box is one line height tall).
13. **Long domains.** The h1 offers a break point after every dot of the domain (`<wbr>`) and no
    other: each label with its dot is one inline-block box, so a hyphen is no break point either; a
    label wider than the line still breaks inside its box as the last resort
    (`overflow-wrap: anywhere`), on the phone and on the desktop.

### Design gate

14. **Board states.** A manifest board may carry `state`, a non-empty string naming a page state
    of the same board title. Its files are named by the title's slug followed by "-" and the
    state's slug, and its rows, summary lines and accepted entries name the board "<title>
    [<state>]". Two boards on one title and state are refused as today.
15. **Long-domain fixture.** A seeded fixture `checkout.eu-central.storefront.example.com` (one
    entry, a first visit) carries full multi-address TLS evidence: four tested addresses (two IPv4,
    two IPv6, documentation ranges), each with a three-item chain (leaf, intermediate, self-signed
    root) and a handshake, and the matching DNS, IP and email blocks; it renders fresh and
    validates as every fixture does.
16. **Manifest.** The private manifest gains the long-domain state on the first-visit board
    (1280, light) and the mobile board (390, light), measuring hero and evidence elements, and on
    every result board the elements "title column height" (desktop boards: the run title's line
    shows as the column's height; the board's run title span reads `height: auto`, and in the
    long-domain state the h1 wraps by design, so there the one-line run title is read from the
    side-by-side image), "evidence caption",
    "evidence th", "evidence td" and an "evidence panel" whose page selector matches only a panel
    without a full evidence block. `just design-check` exits 0 with every row measured or
    accepted.
17. **Canvas.** Boards F and M of the canvas draw the descriptor under the score, "cached" in the
    run title (before "first run" on the first-visit variant) and "All evidence →" in the TLS
    tab's links line; the changed board sources are copied to netray-ops
    `specs/netray/design/canvas/` and committed there. Rows that measure these additions wait for
    the operator's re-export and are listed in report.md.

## Phases

## Phase 1 — Compact evidence tabs

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6, 7, 8, 9, 10

`crates/web/src/compact.rs` (new) builds the compact view; `evidence::panel`
carries it; `partials/result_evidence.html` renders it; `pages.css` styles it.

### Test Scenarios

- GIVEN a stored entry with two TLS addresses on port 443 and identical four-item chains WHEN the
  result page renders THEN the TLS panel holds the caption "Certificate chain · port 443 · 2
  addresses, consistent", a table with header cells #, Subject, Issuer, Key, Not after and four
  body rows, the first row reading 0, the leaf CN, the issuer CN, "ECDSA P-256", the not-after date,
  with Subject and Not after strong, and no `class="evidence-block"`.
- GIVEN two TLS chains whose fingerprints differ WHEN the panel renders THEN the caption reads
  "Certificate chain · port 443 · 2 addresses, chains differ · showing 192.0.2.10".
- GIVEN a root item with `self_signed` true WHEN the TLS table renders THEN its Issuer cell reads
  "self-signed"; GIVEN key "RSA 2048" THEN the Key cell reads "RSA 2048".
- GIVEN a TLS section with `tls.addresses` and no chain block WHEN the panel renders THEN the
  caption reads "Addresses · <n> tested" and the table's columns are Address, Port, Outcome,
  Message.
- GIVEN DNS blocks `dns.a` with two records and an empty `dns.caa` WHEN the DNS panel renders THEN
  the table has a row per A record and one row "CAA", "no records" with an empty TTL, and the
  caption counts the answered record types.
- GIVEN `http.headers` without `strict-transport-security` WHEN the HTTP panel renders THEN the
  HSTS row reads "not sent" and the caption starts "Response · 200 · h2".
- GIVEN email blocks MX, SPF, DMARC and one DKIM key WHEN the Email panel renders THEN the rows
  are MX, SPF, DMARC, DKIM in that order and the caption reads "Mail records · 1 MX host".
- GIVEN an IP row with `tor` true WHEN the IP panel renders THEN its Listed cell reads "tor", and a
  row with every flag false reads "none".
- GIVEN a TLS panel WHEN its feeds text renders THEN it names `tls.chain_trusted` (a chain check)
  and not `tls.tls_version` (a handshake check).
- GIVEN a CSP value of 200 characters WHEN its row renders THEN the cell shows 63 characters and
  "…" and its `title` holds the full value.
- GIVEN a live run streaming its sections WHEN a section's panel fragment arrives THEN it holds
  the compact table and a closed `details` "All evidence" with that protocol's blocks, and no
  "All evidence →" link.
- GIVEN the stylesheets WHEN read THEN the compact caption, header cells and body cells resolve to
  the board values of requirement 10 on desktop and phone.

## Phase 2 — Hero: descriptor, run title, long domains

**Depends on:** none
**Requirements:** 11, 12, 13

### Test Scenarios

- GIVEN the stylesheets WHEN read THEN `.grade__descriptor` does not widen the grade block (it is
  sized by the letter and score line) and wraps.
- GIVEN the domain `checkout.eu-central.storefront.example.com` WHEN the result page renders THEN
  the h1 reads `checkout.<wbr>eu-central.<wbr>storefront.<wbr>example.<wbr>com` and holds no other
  break point.
- GIVEN the stylesheets WHEN read THEN `#result-header h1` carries `overflow-wrap: anywhere` on
  the desktop and the phone.
- GIVEN the first-visit fixture at 1280 px WHEN rendered in the design stack THEN the run title box
  is one line tall (measured by `just design-check`, requirement 16).

## Phase 3 — Design gate states and canvas

**Depends on:** Phase 1, Phase 2
**Requirements:** 14, 15, 16, 17

### Test Scenarios

- GIVEN a manifest with two boards on one title and different `state` values WHEN loaded THEN it
  is accepted; GIVEN the same title and state twice THEN it is refused naming the slug.
- GIVEN a board with `state` "long domain" WHEN the check writes its files THEN they are named
  `<title slug>-long-domain-{board,page,sbs}.png` and its deviation rows name "<title> [long
  domain]"; an accepted entry for it matches by that name.
- GIVEN the fixture directory WHEN the design stack seeds it THEN every domain, the long-domain
  fixture included, renders `data-state="fresh"` and its domains are `example.com` names.
- GIVEN the private manifest WHEN `just design-check` runs THEN it exits 0.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7 (operator, 2026-10-19).
- Compact tables over the full evidence dump, the TLS tab as board F draws it, the other four defined here, and "All evidence →" for the rest (operator, 2026-10-20). The descriptor and CACHED stay and go into the canvas (operator, 2026-10-20).
- The compact view lives in `netray-web` (`compact.rs`) and names block ids and keys, as `explore.rs` already does: it is presentation, which SDD §3.7 keeps out of the shapes; a summary field per block would change every register and stored entry for a display choice (implementer under the operator's autonomy mandate, 2026-10-20).
- DNS shows A, AAAA, NS, CAA and DS: the records the DNS checks and `tls.caa_compliant` judge; MX belongs to the Email tab, TXT and the NSEC family stay in "All evidence" (implementer under the operator's autonomy mandate, 2026-10-20).
- HTTP shows the response line and the six security headers the response checks judge, "not sent" for an absent one; cookies, CORS and redirects stay in "All evidence" (implementer under the operator's autonomy mandate, 2026-10-20).
- Email shows the policy records (MX, SPF, DMARC, MTA-STS, TLS-RPT, DKIM, BIMI): each decides its own check; DNSBL and FCrDNS address rows stay in "All evidence" (implementer under the operator's autonomy mandate, 2026-10-20).
- IP shows one row per address with its network facts and the reputation flags as one Listed cell: `ip.reputation` judges those flags (implementer under the operator's autonomy mandate, 2026-10-20).
- The TLS table shows the band port's first chain; differing chains are named in the caption rather than drawn twice (implementer under the operator's autonomy mandate, 2026-10-20).
- A run without a stored entry has no evidence view to link: its blocks stay reachable in a closed `details` disclosure instead of the link, so an incomplete run loses no evidence and no JavaScript is needed (implementer under the operator's autonomy mandate, 2026-10-20).
- Run title (item 3): the descriptor stops widening the grade block, which gives the title column the width for "… · CACHED · FIRST RUN"; the title is not shortened and no part is dropped (implementer under the operator's autonomy mandate, 2026-10-20).
- Long domains (item 4): break points after the dots (`<wbr>`), each label an inline-block box so a hyphen (browsers break after one) is none (review of Phase 2), keeping board M's `overflow-wrap: anywhere` as the last resort for a single label wider than the line, on both widths; no font scaling, which would put the h1 off the board's 40 and 26 px (implementer under the operator's autonomy mandate, 2026-10-20).
- Gate blind spots (item 5): a manifest board `state` lets one canvas board measure several page states; the panel's "no full evidence block" rule is an element whose page selector only matches a compact panel, and the one-line run title is the title column's height row (the board's span has no measurable height); a wrap point is not measurable, so the long-domain states also serve the side-by-side reading (implementer under the operator's autonomy mandate, 2026-10-20).

- Finish entries (CLAUDE.md bullets, the solutions note, the check to build) written without an operator question under the mandate (implementer under the operator's autonomy mandate, 2026-10-20).
- Review of the range: the phone compact table read the 11 px phone label; a token `font.size.evidence` (13 px, 12 px at phone width) carries board M's 12 px; R16 names "title column height", the board's run title span having no measurable height (implementer under the operator's autonomy mandate, 2026-10-20).

## Open decisions

None.

## Out of scope

- The full evidence view `/r/<id>/<protocol>` and the inspectors.
- Re-exporting the canvas (operator).
