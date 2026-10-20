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
