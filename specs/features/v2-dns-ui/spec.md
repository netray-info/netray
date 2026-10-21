# Spec: v2-dns-ui

Status: Done
Created: 2026-10-20
Finished: 2026-10-21

## Goal

The DNS inspector shows prism's core function again (inventory theme D, UI half): who answered
what, how fast, and where resolvers diverge. A query, compare or authcompare run renders from the
per-server summary v2-dns-api added: a Results tab with an agree or diverge badge per record type
and the answers grouped by identical data, a Servers tab with one card per server, a per-server
filter, view toggles, an explain mode, expandable record details (TTL in human units, transport,
response time), the SOA serial age, a run summary line, and badges on the tabs. Trace and DNSSEC
runs render as structured views instead of key-value trees. The query input autocompletes from
`POST /api/v1/dns/parse`.

## Non-goals

- The shell (header, tabs, form, history, help, shortcuts, permalink of the query, export, the
  live check list, IP badges and address links, `?r=` sharing): `v2-inspector-shell`. A page run
  is already shared by its key there (`SharedRuns`, and for DNS the API's result cache); a shared
  DNS run renders through the same part rendering, so it shows this feature's views without
  further work.
- The protocol-view seam for one block, the facts grid and the generic value display:
  `v2-http-ui`. This feature adds a part-level hook beside it.
- Syntax colouring of the query input (2.3), a transport choice in the builder (2.57, 2.14), the
  landing's mode cards (2.1), cancel (2.32): changed rows, not rebuilt here.
- The DNS checks' scope on subdomains (inventory RESULT? flags, area 10).
- Tracing every type of a multi-type `+trace` (v2-dns-api out of scope).
- The API: v2-dns-api is merged; this feature adds no endpoint and changes no API wire shape.

## Context and constraints

- Inventory (private, `netray.info/specs/inventory/sections/02-dns.md`): rows 2.4, 2.12, 2.15,
  2.16, 2.17, 2.18, 2.19, 2.20, 2.21, 2.24, 2.25, 2.27, 2.28, 2.29, 2.30, 2.40 to 2.42 (the DNS
  part of the exports), 2.43; findings "The fan-out is gone" and "V2 inspector output is raw
  evidence text". `07-site-api.md` 7.85 (UI side). Row 2.36 and 7.86 (share by key) were closed
  by `v2-inspector-shell` and v2-dns-api. Operator decision 2026-10-19: theme D "rebuild".
- Backend half, merged on main: `specs/features/v2-dns-api/`. Data:
  - `netray_dns::jobs::summary::DnsJobSummary` `{record_types, lookups, agree, diverge, single,
    unmeasured, types, servers}`; each `DnsTypeComparison` `{record_type, agreement (agree,
    diverge, single, unmeasured), answers}`; each `DnsServerAnswer` `{server, address, name,
    transport, source, outcome (answered, no_records, nxdomain, unmeasured), response_code, error,
    response_time_ms, records}`; each record `{name, type, ttl, data, soa_serial_age_days}`; each
    `DnsServerSummary` `{server, name, transport, source, lookups, measured, unmeasured,
    median_response_time_ms}`. The collected result holds it once `done` arrived; `summarise`
    is public for a job that never finished.
  - Done events: query `{total_queries, duration_ms, warnings, transport, dnssec,
    degraded_providers}`, compare `{transports, …}`, authcompare `{auth_servers, …}`, trace
    `{hops, duration_ms}`, dnssec `{levels, duration_ms}`.
  - Trace hops carry per server `server_name`, `answer_records`, `referral_ns`, and per hop
    `referral_groups`; DNSSEC levels carry `dnskey_records`, `ds_records`, `rrsig_records` beside
    findings and latency.
  - `POST /api/v1/dns/parse` `{input, cursor_pos}` answers `{tokens, completions, query, error}`
    (byte ranges and cursor in UTF-8 bytes), under the `[api]` per-IP limiter.
- Human and explain texts per record exist in Rust: `crates/dns/src/record_format.rs`
  (`format_txt_human`, `format_txt_explain`, and the CAA, MX, SOA, SVCB, HTTPS, TLSA, NAPTR,
  DNSKEY, DS, NSEC pairs over the serialised record data).
- V1 reference: `crates/dns/frontend/src/components/` `ResultsTable.tsx`, `ServerComparison.tsx`,
  `TraceView.tsx`, `DnssecView.tsx`, `TransportComparison.tsx`, `AuthComparison.tsx`, the toolbar
  toggles in `App.tsx:1438-1470`, tab badges.
- Today's page: `crates/dns/src/jobs/blocks.rs` `query_blocks` merges every server's records into
  one deduped records block per type; compare and authcompare are one table each; trace and
  DNSSEC are tree blocks printed as text. `crates/web/src/api/jobs.rs` `dns_part` adds
  `dns.errors` and `dns.warnings`. `crates/web/src/pages/inspect.rs` `part_html` renders a part's
  blocks, each through `pages::views::render` or the generic evidence block; live runs, shared
  page runs and API-cache results all render through it.
- SDD: §3.7 (six shapes, data never presentation, no module ships templates), §6.1 (evidence view
  order, anchors), §6.2 (inspectors are never stored in the history; Rule M per record type),
  §8.4 (islands read data attributes, compute no layout, hold no verdict; S22 allows the DNS
  inspector its own island; bundle cap 30 KB gzipped).
- Board: none for the DNS inspector. The page follows board G's frame through the shell; its
  sections use G's section cards, tables and tab style (marker). Recorded as "no board; follows
  G's frame".
- Gate rules: layout (bar lengths, ordering ranks) computed in Rust; islands toggle data
  attributes and may reorder cards by server-written ranks, never write style; page tests assert
  by markup; test invalid input through the router.
- Test data: the offline stub (`netray_dns::testing`: `example.com`, `partial.example.net`,
  `diverge.example.com`, `soa.example.com`, `dnssec-failed.org`), documentation addresses.

## Requirements

### Data on the page

1. **Summary blocks.** A query, compare or authcompare part gains, before the job's own blocks,
   two tree blocks built in `netray_dns::jobs::blocks` from the collected summary (or
   `summarise` over the batches when the job never finished) and the done event: `dns.answers`
   "Answers" (root: `mode`, `finished`, the summary counts, and from done `duration`,
   `total_queries`, `transport` (repeated for compare), `auth_server`, `degraded_provider`,
   `dnssec`; one child per record type with `record_type`, `agreement`; under it one child per
   server answer with `server`, `address`, `name`, `transport`, `source`, `outcome`,
   `response_code`, `error`, `response_time`; under that one child per record with `name`,
   `type`, `ttl`, `data`, `soa_serial_age_days`, `human`, `explain`) and `dns.servers` "Servers"
   (one child per server summary). Absent facts are omitted, never a placeholder. The same blocks
   come with a result read from the API cache, followed there by the `dns.errors` and
   `dns.warnings` blocks the live part adds. (2.15, theme D)
2. **Part view.** `pages::views` gains a part-level hook asked once per part before its blocks:
   the DNS part view claims `dns.answers`, `dns.servers`, `dns.warnings` and each job block whose
   every value the view shows (a records block whose every record data appears among that type's
   answers, a "not measured" fields block whose type has a card, a compare or authcompare table
   whose every record appears among its type's answers), and renders one fragment in place of the
   first claimed block; unclaimed blocks render as before. The fragment carries `id="ev-<id>"`
   for each claimed block. SDD §6.1 is amended. (theme D)
3. **Run summary.** The fragment starts with one mono line: "<record_types> types · <lookups>
   lookups · <servers> servers · <transport> · <agree> agree · <diverge> diverge · <single> single
   · <unmeasured> not measured · <duration>", a zero count left out except agree and diverge; a
   muted line "<total_queries> queries sent" with "DNSSEC requested" when the done event says so;
   then each warning
   and each degraded provider ("<provider> skipped: circuit open") as a notice line. Compare names
   its transports, authcompare its authoritative servers. A job that never finished shows the
   notice "The query ended before every server answered" and no summary line: its counts are
   partial, and every answer they count is in the cards. (2.25, 2.12)

### Results tab

4. **Per type.** The Results tab holds one card per record type in summary order: the type (mono,
   weight 700) and its agreement badge: "agree" (pass tone), "diverge" (warn tone), "single
   answer" (muted), "not measured" (muted). Compare reads "all transports agree" and authcompare
   "auth and recursive agree" for agree. (2.15, 2.29, 2.30)
5. **Answer groups.** Inside a card, answers are grouped by identical outcome and record data, the
   largest group first; each group shows its records, "<n> / <m> servers" and per server its name
   (or address) with the transport (and the source in authcompare), the TTL range across the group
   (each record's own TTL range in its details) and a response-time bar (none for a
   not-measured answer) whose length is a Rust function of the server's time over the job's
   slowest measured answer. A diverging card shows every group. (2.15, 2.29, 2.30)
6. **Negative and failed answers.** `no_records` reads "no records", `nxdomain` reads "NXDOMAIN",
   an `unmeasured` answer reads "not measured: <response code or error>" beside the answered
   groups (Rule M), so a failure on one server is visible while another answers; the error is
   preferred to the response code, which is NOERROR for a referral. A "not measured" block is
   claimed only when its reason is among its type's answers. Record data keeps the shell's links
   and address badges. (2.21)
7. **Readable data.** Each record shows its zone-file data (mono) and, where `record_format.rs`
   has a human form that differs from the data, the human form under it in muted text. (2.20)

### Servers tab and per-server view

8. **Servers tab.** One card per server in summary order: name (or address), transport, source
   (authcompare), "<measured> / <lookups> measured", median response time (absent without a
   measured lookup); the card lists per record type that server's outcome and records. A type
   whose card diverges carries the diverge marker in every server card. (2.16)
9. **Per-server view.** Activating a server card's filter button sets `data-server` on the result
   region to that server and hides other servers' answers in both tabs; "All servers" resets.
   Without JavaScript the server cards are the per-server view. (2.16)
10. **Tab badges.** The Results tab carries "<d> diverge" (warn tone) and "<u> not measured"
    (muted) when non-zero, and a pass mark otherwise; the Servers tab carries the number of
    servers with an unmeasured lookup (muted, as on the Results tab), and a pass mark otherwise; a job that never
    finished carries no pass mark. Counts come from the server; the island never counts. (2.43)

### Toggles, explain and details

11. **View toggles.** A toolbar above the tabs, rendered `hidden` and shown by the island: "Hide
    empty" (hides `no_records` and `nxdomain` groups), "Compact" (hides groups past the first
    answered one and record details, never a not-measured group, and not while a server filter is
    on), "Only divergent" (hides agreeing and single cards; not-measured cards stay), "Sort" (by type, or
    slowest first by a server-written rank on each card), "Expand all" and "Collapse all" (record
    details). Each toggle switches a data attribute on the result region; the stylesheet hides or
    shows; the island reorders cards only by the written ranks. Choices persist per browser in
    `netray.dns.view`; storage failures keep the defaults. (2.17)
12. **Explain mode.** A toolbar toggle "Explain" shows under each record the explain sentence of
    `record_format.rs` where one exists, server-rendered outside the record's closed details and
    hidden until the toggle is on. (2.18)
13. **Record details.** Each record row is a `<details>` disclosing: TTL in human units with the
    seconds ("1 h 5 min (3900 s)"), each answering server by its name and address (the mhost form
    stays in `data-server`), the transport and the response time; for SOA the serial age ("serial 2026101501 · 4 days old" from
    `soa_serial_age_days`, absent when the serial is no date, "dated in the future" for a serial
    after today), which also shows on the SOA row itself; the details' server rows take part in the
    server filter. (2.19, 2.24)

### Trace and DNSSEC views

14. **DNSSEC view.** The `dns.dnssec_chain` block gains per level the `dnskey`, `ds` and `rrsig`
    record data (repeated text). The DNSSEC view claims it and renders one card per level, root
    first: the zone, its latency, its findings each in its severity's tone (ok pass, warning warn,
    failed fail), a badge "<w> warn · <f> fail" when either is non-zero, and a `<details>`
    disclosing its DNSKEY, DS and RRSIG records. The view's heading line counts the levels and the
    warn and fail findings. (2.27, 2.43)
15. **Trace view.** The `dns.trace` block gains per server `server_name`, `referral` (repeated NS
    names) and `answer` (repeated record data), and per hop `referral_groups` (count). The trace
    view claims it and renders one row per hop: level, zone, the servers queried (server name and
    address, latency bar from a Rust function over the slowest server, error text of a failed
    one), the referral NS names, a "servers disagree" marker when `referral_groups` is above one,
    each server's outcome in words ("referral", "answer", "answer, no records", "error"), and the
    answer records where a server answered. With several types named in a `+trace` query (the
    parser's default types of a query naming none do not count) the warnings block carries "Only
    the first type is traced: <type>.", live and in a replay from the API cache. (2.28)

### Autocomplete, exports and acceptance

16. **Autocomplete over parse.** The shell's DNS provider asks `POST /api/v1/dns/parse` with the
    input and the caret as a UTF-8 byte offset, at most once per 150 ms of typing and only with the
    caret past the domain, and lists its `completions` that match the token ahead of the table
    suggestions it lacks; while the request fails or is refused (429) it falls back to the
    server-written tables without showing an error. An answer that arrives after the input changed,
    the list closed or a row was selected is dropped. The parse `error` shows under the input as a
    muted hint before submit, and leaves with any change of the input. (2.4, 2.12, 7.85)
17. **DNS exports.** The shell's exports of a query, compare or authcompare run carry
    `dns.answers` and `dns.servers` as blocks: JSON with their stored keys and raw values, CSV
    and Markdown rows for them. (2.40, 2.41, 2.42)
18. **No JavaScript.** Without JavaScript the tabs render in order with in-page links, record
    details are `<details>`, explain sentences and the toolbar are absent, a shared result renders
    the same views. (S7)
19. **Bundle cap.** The DNS island's additions keep the bundle under 30 KB gzipped; islands write
    no style and compute no count or verdict. (§8.4, S22)
20. **Design gate.** `just design-check` exits 0 with this feature in the stack; the DNS inspector
    (a query with a diverging type and an unmeasured server, Servers tab, explain on, a trace, a
    DNSSEC chain) at 1280 px and 390 px, both themes, is read beside board G by a reader with no
    part in the build; every finding is a `report.md` row and is closed. At 390 px nothing scrolls
    sideways (wide content scrolls inside its card). ("no board; follows G's frame")

## Phase 1 — Summary blocks, the part view and the Results tab

**Depends on:** `v2-inspector-shell` and `v2-http-ui` merged
**Requirements:** 1, 2, 3, 4, 5, 6, 7

### Test Scenarios

GIVEN the stub and a query for `example.com A` WHEN the part is built THEN it holds `dns.answers` and `dns.servers` before `dns.a`, and `dns.answers` has one type child A with agreement agree.
GIVEN the stub and a query for `example.com A` WHEN the run ends THEN the Results tab holds one A card with the badge "agree" and one group with `192.0.2.10` and "<m> / <m> servers", and the generic `dns.a` block is gone.
GIVEN `diverge.example.com A` over two servers WHEN queried THEN the A card's badge reads "diverge" and it shows one group per distinct answer with its server names.
GIVEN `partial.example.net A AAAA` WHEN queried THEN A reads "agree" or "single answer" and AAAA reads "not measured" with the reason, and the summary line holds "not measured".
GIVEN one server answering and one SERVFAIL for a type WHEN rendered THEN the second server reads "not measured: SERVFAIL".
GIVEN a type every server answers NODATA WHEN rendered THEN the group reads "no records".
GIVEN a TXT record `v=spf1 include:_spf.example.com -all` WHEN rendered THEN its human form shows under the data.
GIVEN a done event with a warning WHEN rendered THEN the warning shows as a notice line and the generic Warnings table is gone.
GIVEN response times 10 and 40 ms WHEN the bars are laid out THEN the 40 ms bar is full length and the 10 ms bar a quarter.
GIVEN a compare run WHEN rendered THEN an agreeing card reads "all transports agree"; GIVEN authcompare THEN "auth and recursive agree".
GIVEN a records block holding a record no answer holds WHEN the part renders THEN that block renders through the generic evidence block.
GIVEN the stub runs WHEN the nothing-lost test runs THEN every record data of the claimed blocks appears in the fragment.
GIVEN a cached API result key of a query WHEN `/inspect/dns?r=<key>` renders THEN it shows the Results tab.

## Phase 2 — Servers tab, per-server view and tab badges

**Depends on:** Phase 1
**Requirements:** 8, 9, 10

### Test Scenarios

GIVEN a run over several servers WHEN the Servers tab renders THEN it holds one card per server in summary order, each with its transport, "<measured> / <lookups> measured" and median.
GIVEN a server with only errors WHEN rendered THEN its card shows no median and its lookups as not measured.
GIVEN a diverging A type WHEN the Servers tab renders THEN every server card marks A as divergent.
GIVEN a server card's filter button WHEN activated THEN the result region's `data-server` names it; WHEN "All servers" is activated THEN `data-server` is removed.
GIVEN one diverging and one unmeasured type WHEN the tabs render THEN Results carries "1 diverge" and "1 not measured" from server-written text.
GIVEN a run where every type agrees WHEN the tabs render THEN Results carries the pass mark.

## Phase 3 — Toggles, explain and record details

**Depends on:** Phase 1
**Requirements:** 11, 12, 13, 18, 19

### Test Scenarios

GIVEN "Hide empty" on WHEN toggled THEN the result region carries `data-hide-empty` and the island wrote no style.
GIVEN "Only divergent" on WHEN toggled THEN the region carries `data-only-divergent` and the stylesheet hides cards without `data-agreement="diverge"`.
GIVEN "Sort" by slowest WHEN chosen THEN the cards are in the order of their `data-rank` values.
GIVEN a stored view choice WHEN the island mounts THEN the toggles start in that state; GIVEN storage that throws THEN the defaults hold and nothing escapes.
GIVEN an MX record of priority 10 WHEN rendered THEN its explain sentence is in the markup inside an element hidden until explain is on.
GIVEN an A record with TTL 3900 WHEN its details render THEN they read "1 h 5 min (3900 s)", the server, the transport and the response time.
GIVEN an SOA record with serial 2026101501 and age 4 WHEN rendered THEN the row reads "serial 2026101501 · 4 days old"; GIVEN no age THEN no age text.
GIVEN "Expand all" WHEN pressed THEN every record's details are open.
GIVEN no JavaScript WHEN a run's page renders THEN the toolbar is `hidden` and the details still open.
GIVEN the islands bundle WHEN built THEN the budget test passes.

## Phase 4 — Trace and DNSSEC views

**Depends on:** Phase 1
**Requirements:** 14, 15

### Test Scenarios

GIVEN a stub DNSSEC run WHEN rendered THEN one card per level shows zone, latency and findings in their tones, and each level's details list its DNSKEY, DS and RRSIG records when it has any.
GIVEN a level with a warning finding WHEN rendered THEN its card carries "1 warn".
GIVEN a stub trace of `example.com MX` WHEN rendered THEN each hop shows level, zone, server addresses with latency bars, referral NS names where referred, and the answering hop the answer records.
GIVEN a hop whose `referral_groups` is 2 WHEN rendered THEN it carries "servers disagree".
GIVEN `example.com MX TXT +trace` WHEN run THEN the warnings carry "Only the first type is traced: MX."
GIVEN the trace and DNSSEC blocks WHEN the nothing-lost test runs THEN every value of the claimed tree blocks appears.

## Phase 5 — Autocomplete over parse and exports

**Depends on:** Phase 1
**Requirements:** 16, 17

### Test Scenarios

GIVEN the DNS input `example.com @cl` with the caret at its end WHEN typing pauses THEN one parse request is sent with `cursor_pos` 15 and the list offers the completions that start with `@cl`.
GIVEN two keystrokes within 150 ms WHEN typing THEN one parse request is sent.
GIVEN a parse request answered 429 WHEN typing THEN suggestions come from the server-written tables and no error shows.
GIVEN a parse answer with an `error` WHEN typing pauses THEN the hint under the input shows it.
GIVEN a finished query WHEN JSON is exported THEN it holds the `dns.answers` block; WHEN CSV is exported THEN it holds rows of `dns.servers`.

## Phase 6 — Design gate and visual reading

**Depends on:** Phases 1 to 5
**Requirements:** 20

`just design-check` on the local stack; screenshots of the listed states from a local server over
the offline stub; the reader's pass beside board G; results under `### Behavioural verification`
in `report.md`.

### Test Scenarios

GIVEN the manifest WHEN `just design-check` runs THEN it exits 0.
GIVEN the screenshots WHEN read beside board G THEN every finding is a `report.md` row and none stays open.
GIVEN the DNS inspector at 390 px WHEN measured THEN the document is not wider than the viewport.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- `?r=` sharing comes from the shell for every inspector; this spec only wires DNS results into it
  (operator, 2026-10-20). Re-check against main 2026-10-21: the shell (#46) already stores every
  page run under its key (`SharedRuns`) and DNS page runs also in the API cache, and both render
  through `part_html`; the draft's requirements 16 and 17 (result key from the page, share by key)
  are dropped, and a shared result shows the views because the summary blocks are part of the
  stored blocks (implementer under the operator's autonomy mandate, 2026-10-21).
- The summary reaches the page as two tree blocks (`dns.answers`, `dns.servers`) built in the DNS
  crate, not as a side channel: blocks are what the page renders, the shell exports and stores
  for sharing, and what the API cache replays, so every path gets the same data; §3.7 holds (data,
  not presentation). The human and explain texts are carried as record keys because they are
  already API data (V1's `*_human`, `*_explain`) and the web crate must not call the DNS crate's
  formatters. The job's own blocks keep their shape (v2-dns-api), so the summary blocks are added
  beside them (implementer under the operator's autonomy mandate, 2026-10-21).
- A part-level view hook beside v2-http-ui's per-block seam: one block cannot render tabs over a
  whole part, and the merged per-type records blocks must not show each answer a second time. The
  hook claims a job block only when the fragment shows all of its values, so the "lose nothing"
  rule of v2-http-ui requirement 3 holds; `dns.errors` stays generic (Rule M) (implementer under
  the operator's autonomy mandate, 2026-10-21).
- The draft's exports requirement ("JSON gains `summary`") becomes "the exports carry the summary
  blocks": the shell exports blocks, and a second, unblocked copy of the summary would fork the
  export format (implementer under the operator's autonomy mandate, 2026-10-21).
- Phase 1 review: record data in the part view keeps the shell's links and badges (it had
  dropped them for claimed records blocks); an unmeasured answer reads its error before its
  response code (a referral is NOERROR); a "not measured" block or a failed compare row is claimed
  only when its reason is shown; each record shows its own TTL range in its details (a merged
  records block keeps one TTL per record, a group's range could hide it); the summary line counts
  single answers and a meta line shows the queries sent and DNSSEC; an API-cache replay carries the
  job's errors and warnings as the live part does (Rule M). An unfinished job's partial counts stay
  unshown: its cards hold every answer they count (implementer under the operator's autonomy
  mandate, 2026-10-21).
- Phase 2 review: the server filter also hides an answer group none of whose servers is shown,
  so another server's records do not stay on screen; a server that answers under two sources in
  authcompare (a recursive and an authoritative server printing alike) gets a filter key with its
  source and its card reads its own answers; an unfinished job carries no pass mark, since its
  missing types are in no count; a server card without a name reads its address. Server record
  data in the Servers tab stays plain: the Results tab carries its links and badges
  (implementer under the operator's autonomy mandate, 2026-10-21).
- Phase 3 review: Compact keeps the first answered group and every not-measured group (Rule M),
  steps aside while a server filter is on (or a filtered card could go empty), and hides record
  details as requirement 11 says; "Only divergent" keeps not-measured cards (the scenario's "cards
  without diverge" would have hidden Rule M types); the explain sentence sits under the record,
  outside its closed details, so the toggle shows it at once; the details' server rows carry their
  server for the filter; a serial dated after today reads "dated in the future", not a negative
  age (implementer under the operator's autonomy mandate, 2026-10-21).
- Phase 4 review: a trace server's outcome shows in words, so an authoritative NODATA or NXDOMAIN
  (an answer without records) is not a bare row; the "first type" note counts the types the query
  names, not the parser's defaults, and a replay from the API cache recomputes it from the stored
  query. A claimed part is streamed again after the band only when its HTML with the checks
  differs from without, so views that read no checks (DNS) are sent once and the HTTP views still
  get their tones (implementer under the operator's autonomy mandate, 2026-10-21).
- Phase 5 review: a late parse answer is dropped once the list closed (submit, Escape, a pick,
  blur) or a row is selected, so it never reopens a list or steals the selection; the hint follows
  the input it was given for and leaves on any change; the parser's completions are added ahead of
  the table's instead of replacing them (the table lists types the parser's completion data lacks,
  NSEC3 among them); keystrokes in the domain send no request, which keeps a typed query's last
  request inside the `[api]` per-IP budget at ordinary typing speed. A faster typist can still
  meet a refused request; it falls back silently (implementer under the operator's autonomy
  mandate, 2026-10-21).
- Phase 6 reading beside board G (report.md V1 to V25, N1 to N12): rebuilt the trace rows, the
  phone layout of server rows, full-width columns with row rules, one card level (types and
  servers are sections of the tab panel), "All servers" in the toolbar, human part headings
  ("Authoritative and recursive", "Transport comparison", "Trace", "DNSSEC") with the block's
  name ("Delegation trace", "DNSSEC chain") in the card's meta line instead of a second heading,
  no bar for a not-measured answer (a bar reads as an answer's speed), 44 px targets at 390 px,
  readable muted sizes, label and value columns in record details with servers by name and
  address (requirement 13 amended), one muted tone for "not measured" on both tabs (requirement
  10 amended), and a muted summary line. Kept deliberately: the green "agree" badge and the amber
  diverge marker in every server card (requirements 4 and 8: the reader asks for tone only on
  problems, but agreement is the fact the Results tab exists to show, and the marker tells which
  types a server answered differently); one badge per DNSSEC level in its worst tone (requirement
  14); the tab counts' units come from the tab names; the hop's level and zone head its rows.
  Findings on the shell (query builder, "+check" note, run line and exports, header, 429 frame)
  belong to `v2-inspector-shell` and go to the operator as proposals (implementer under the
  operator's autonomy mandate, 2026-10-21).
- After merging main (v2-ip-ui, #51) the merge of `pages.css` dropped one closing brace, which
  nested every later rule (all of v2-ip-ui's and this feature's) inside an open rule; the browser
  dropped them and no test failed. Restored, and `static_css_balanced.rs` now checks every
  stylesheet (implementer under the operator's autonomy mandate, 2026-10-21).
- Final review (blocker): fixed on this branch rather than split. The trace note counts the types
  as the parser reads them (`netray_dns::parser::named_record_types`: duplicates dropped, aliases
  resolved, ALL expanded), so it names the type the trace runs; an API-cache replay of a trace
  from the API's own route (no `+trace` token) gets the note too. Compact also leaves record
  details alone under a server filter, and the SOA serial age is a row of the record details
  beside the row's mark (implementer under the operator's autonomy mandate, 2026-10-21).
- Finish: the distilled entries (three CLAUDE.md bullets, one solutions note) are written without
  the operator's confirmation, under the autonomy mandate; the operator may drop any of them
  (implementer under the operator's autonomy mandate, 2026-10-21).
- The trace heading names no traced type and the shared-result notices of the draft are dropped:
  the traced type is not in the collected trace result and adding it would change the API's done
  event; the run line already shows the query. A multi-type `+trace` puts "Only the first type is
  traced: <type>." into the warnings block (implementer under the operator's autonomy mandate,
  2026-10-21).
- The DNSSEC view shows the DS and DNSKEY records and the chain's findings (which already state
  each DS-to-DNSKEY match or mismatch), not a separate "DS key tag → DNSKEY" line: the DNSKEY key
  tag is not in the stored data, and deriving it in the page would be a second validation beside
  the job's (implementer under the operator's autonomy mandate, 2026-10-21).
- Tab badges only on the Results and Servers tabs: trace and DNSSEC runs have one view and no
  tabs, so their counts sit in the view's heading line (implementer under the operator's autonomy
  mandate, 2026-10-21).
- Sorting reorders cards by ranks the server wrote; the island never compares values, so it holds
  no verdict and computes no layout (§8.4) (drafting agent, 2026-10-20).
- Human forms show by default and explain sentences only in explain mode, as V1: the human form
  helps every reader, the explain sentence is long (drafting agent, 2026-10-20).
- Autocomplete asks `/dns/parse` behind a 150 ms debounce and falls back to the shell's tables, so
  the per-IP limiter never turns typing into an error (drafting agent, 2026-10-20).
- Board: none for the DNS inspector; it follows board G's frame. Proposal for the operator: add a
  "DNS inspector" board (Results tab with a diverging type, Servers tab, a trace) to the canvas,
  since S22 calls this the largest view to rebuild (drafting agent, 2026-10-20).

## Open decisions

None.

## Out of scope

- Shell parts and sharing: `v2-inspector-shell`.
- Rows 2.1, 2.3, 2.14 (builder transport), 2.32, 2.57; tracing every type of `+trace`.
- API changes.
