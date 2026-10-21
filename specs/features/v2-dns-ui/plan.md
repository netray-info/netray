# Plan: v2-dns-ui

## Phase 1 — Summary blocks, the part view and the Results tab

### Groups

G1: C1, C8 · G2: C2 to C7, C9 to C20 (depends on G1; C14's block half is covered by G1).

### Plan

#### G1

1. `crates/dns/src/record_format.rs`: `pub fn rdata_texts(&RData) -> (Option<String>, Option<String>)`, the human and explain texts of one record (TXT directly; other types via `serde_json::to_value` and the formatter table of `enrich_lookups_json`).
2. `crates/dns/src/jobs/blocks.rs`: `query_page_blocks`, `compare_page_blocks`, `authcompare_page_blocks` = `[dns.answers, dns.servers]` + the existing job blocks. Summary from `result.summary` or `summarise(&batches)`; `finished` = done present; done facts per job; absent facts omitted; `human`/`explain` attached from a `(type, rdata_text)` map over the resolved lookups (no new fields on `DnsRecordDetail`, so the protected struct literals in `job_summary.rs` stay).
3. `crates/dns/src/jobs/results.rs`: `DnsCachedJob::page_blocks()`; `blocks()` unchanged.
4. `crates/dns/src/lib.rs`: re-export the three page functions.
5. `crates/web/src/api/jobs.rs`: `start_dns` passes the page variants to `dns_part`; the API-cache replay uses `page_blocks()`.

#### G2

1. `crates/web/src/pages/views/mod.rs`: `PartView`, `render_part` (delegates to `dns::render_part`), `bar_width`.
2. `crates/web/src/pages/views/dns.rs`: the DNS part view: model of `dns.answers`, summary line, notices, cards with mode-dependent agree words and `data-rank`, groups by (outcome, data) largest first, TTL range, negative texts, bars over the job's slowest measured answer, records with human and explain, a minimal servers panel (`id="ev-dns.servers"`), and the claims (answers, servers, one-column warnings, fully shown records blocks, `{outcome, reason}` fields blocks with a card, fully shown compare and authcompare tables) with their anchors.
3. `crates/web/templates/partials/dns_view.html`: the fragment markup of the contract (not named `inspect_*`).
4. `crates/web/src/pages/inspect.rs` `part_html`: `render_part` once before the block loop; its html at the first claimed block, other claimed blocks skipped. The band re-stream check stays (the DNS view reads no statuses; re-streaming would break `inspect_dns_band.rs` and `inspect_dns_jobs.rs`).
5. `crates/web/static/pages.css`: rules for `.dns-view*`, `.dns-type*`, `.dns-group*`, `.dns-bar*`, `.dns-record*` with G's tokens; panels hidden only under `[data-ready]`; wide content scrolls inside its card.
6. `specs/sdd/v2.md` §6.1: a "Part views" bullet.
7. `crates/web/tests/it/inspect_dns_jobs.rs` `c8_compare_and_auth_flags_stream_their_tables`: assert `id="ev-<id>"` and `class="dns-view"` instead of the generic table markup (ADLC-Test-Change).

## Phase 2 — Servers tab, per-server view and tab badges

### Groups

G1: C1 to C9 (one fragment and its island; the files overlap).

### Plan

#### G1

1. `crates/web/src/pages/views/dns.rs`: `ServerCard` gains per-type rows built from `dns.answers` (the server's answer per type: outcome words, records, the type's agreement); tab badge counts from the root's `diverge` and `unmeasured` and from the `dns.servers` children with `unmeasured` above zero.
2. `crates/web/templates/partials/dns_view.html`: the root carries `data-island="dnsview"`; the "All servers" button; the Servers panel as `dns-server` cards with the hidden filter button; badges in the tab links.
3. `crates/web/static/pages.css`: `.dns-server*`, `.dns-view__tab-badge` tones, `.dns-view__all`, `.dns-view [data-off] { display: none; }`.
4. `crates/web/islands/src/dnsview.ts` (new) and `src/main.ts`: unhide the filter buttons; a click on `[data-server-filter]` sets or removes `data-server` on the root and `data-off` on non-matching `.dns-group__server` and `.dns-server`.

## Phase 3 — Toggles, explain and record details

### Groups

G1: C1 to C15 (one fragment, its stylesheet and its island).

### Plan

#### G1

1. `crates/web/src/pages/views/dns.rs`: cards gain `data-order`; records gain details rows: TTL in human units over the group's servers holding that record, and one row per answering server (mhost string, transport, response time); the SOA serial age from `soa_serial_age_days` and the SOA data's serial; a human-units function for seconds (two largest non-zero units, then the seconds).
2. `crates/web/templates/partials/dns_view.html`: the hidden toolbar; `data-order`; `<dl class="dns-record__details">`; the serial span in the summary. The Phase 1 `dns-record__ttl` paragraph becomes the dd (the Phase 1 review test's needle "TTL 600 s" becomes "10 min (600 s)", ADLC-Test-Change).
3. `crates/web/static/pages.css`: toggle rules (hide empty, only divergent, compact, explain), toolbar and details styles.
4. `crates/web/islands/src/dnsview.ts`: toolbar unhide, toggles with `aria-pressed` and root attributes, reorder by written `data-rank` or `data-order`, expand and collapse, `netray.dns.view` persistence in try/catch.
