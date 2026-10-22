# Review: v2-ip-ui

## 39ccc6e9..3e07e5f3

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Security, Testing

I found nothing that goes wrong, from `crates/web/src/pages/inspect.rs` through `crates/web/src/api/jobs.rs`, `crates/ip/src/module.rs`, `crates/web/src/pages/views/ip.rs` and `crates/web/islands/src/shell.ts`. I checked each item below against concrete inputs:
- **Caller run on `GET /inspect/ip`.** Slot first, then the per-IP and per-target admission, then the deadline. The port is 0 when a trusted proxy supplied the address. Refusals, timeouts and a disabled module come back as a 200 notice under the address with `private, no-store`, and a prefetch gets 503.
- **Slow reverse DNS.** `bounded_reverse` drops a stalled PTR after three quarters of the remaining time and looks the address up again without it. Nothing is cached when it is cancelled.
- **`translate_lookup` and the new blocks.** The checks keep their status and findings and now point at `ip.lookup`. `lookup_block` omits unknown keys and the "Other" family. `headers_block` drops empty hops.
- **The IP view loses nothing.** Every key reaches the card in every combination I tried: country or registered country with or without ISO, latitude without longitude, accuracy alone, a cloud service without a provider.
- **Escaping of untrusted text.** PTR host names, header names and values, and forwarded-for hops are HTML-escaped by askama or percent-encoded in links.
- **Explorer commands.** I checked each one against the real `ip_host`, `/api/v1/ip` and batch routes and the batch body (`addresses`).
- **Attribution placements.** The card, `ip.addresses` in the evidence views and the result page's "All evidence", the compact IP tab, and inspector parts with badged addresses.
- **Tests.** The copy-button selector and click handler work, and all 21 new test modules are registered in `crates/web/tests/it/main.rs`.

Two things I left out because the code does what the spec asks; they only affect how the page looks:
- A datacenter address outside a cloud shows two identical "datacenter" chips (the infra-type chip and the flag chip). The same happens with "internal".
- In the result page's "All evidence" details, the `ip.addresses` block now carries a "Feeds lines" paragraph that the generic block there does not show.

### Spec reader

COUNTS blockers=0 majors=2 minors=1
REQUIREMENTS met=14 partial=2 missed=0 contradicted=0
MAJOR | crates/web/src/pages/views/ip.rs:396 | On the live result page's IP tab, inside the "All evidence" details, the claimed `ip.addresses` view adds a "Feeds lines" paragraph that the generic block there does not render, so the block's markup is no longer the generic block's. | R14: take a live result page with no entry yet, where `ip.reputation` references `ip.addresses`. `panel()` builds the generic slot with `feeds: Vec::new()` (evidence.rs:288) and passes `block_feeds(...)` only to the view (evidence.rs:675). The view sets `feeds: ctx.feeds.to_vec()`, so it emits `<p ...>Feeds lines: ...</p>` before the attribution. report.md Phase 3 kept this as a NIT, but the spec was not amended.
MAJOR | crates/web/static/pages.css:1201 | At 390 px a long explorer command wraps (`overflow-wrap: anywhere`), but the requirement says long commands scroll inside their section. | R16: the batch command `curl https://netray.info/api/v1/ip/batch --json '{"addresses":["192.0.2.10"]}'` at 390 px breaks onto several lines; nothing scrolls. report.md V5 says "rebuilt: a command wraps", but the spec's Phase 4 decision log does not record the change. The document still measures 390 = 390, so the width half of the requirement is met.
MINOR | crates/web/src/pages/inspect.rs:649 | A prefetch or prerender `GET /inspect/ip` with neither `ip` nor `r` gets a bare `503` with `Cache-Control: no-store` and no page; the spec never names this. | R3 / unrequested: `curl -H "Sec-Purpose: prefetch;prerender" /inspect/ip` returns 503 (report.md Phase 1 shows it). Requirement 3 says this GET renders the caller's run. Only the range's own SDD edit names "a prefetch gets 503".

```quote specs/features/v2-ip-ui/spec.md:165
14. **MaxMind attribution.** One partial renders `netray_ip::GEOIP_ATTRIBUTION` in muted 12 px
    text with "MaxMind" linked to `https://www.maxmind.com`; no template holds the text. It
    follows GeoIP-derived data: as the last element of the IP card; after every `ip.addresses`
    block wherever it renders (the evidence views `/<domain>/ip` and `/r/<id>/ip`, the result
    page's IP tab and "All evidence" details), the block's markup otherwise the generic block's;
```

```quote crates/web/src/pages/views/ip.rs:394
fn addresses(block: &Block, ctx: &ViewContext<'_>) -> Option<String> {
    let view = EvidenceBlockView {
        feeds: ctx.feeds.to_vec(),
        ..block_view_with(block, ctx.decorate)
    };
    Some(evidence_block(&view).ok()? + &attribution()?)
}
```

```quote crates/web/src/evidence.rs:285
    EvidenceBlockView {
        id: format!("ev-{}", block.id),
        title: block.title.clone(),
        feeds: Vec::new(),
```

```quote crates/web/templates/partials/result_evidence.html:15
{% for block in panel.blocks %}{% if let Some(html) = block.view %}{{ html|safe }}{% else %}{% call evidence_block::evidence_block(block.block) %}{% endcall %}{% endif %}
```

```quote specs/features/v2-ip-ui/spec.md:180
    1280 px and 390 px, both themes, is read beside board G by a reader with no part in the
    build; every finding is a `report.md` row and is closed. At 390 px the document is not wider
    than the viewport (the headers table and long commands scroll inside their section).
```

```quote crates/web/static/pages.css:1200
/* A long command wraps inside the card rather than running past it at phone width. */
.ip-explorer__code {
  min-width: 0;
  color: var(--color-ink);
  font-family: var(--font-mono);
  font-size: var(--font-size-small);
  overflow-wrap: anywhere;
}
```

```quote specs/features/v2-ip-ui/spec.md:93
3. **Caller on load.** `GET /inspect/ip` without `ip` and without `r` renders the caller's run
   server-side, in the markup a finished live run leaves: the per-IP admission of `[api.ip]`
```

```quote crates/web/src/pages/inspect.rs:648
    if tool == Tool::Ip && shared_key.is_none() && prefill.is_none() {
        if netray_common::prefetch::is_prefetch(req.headers()) {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(CACHE_CONTROL, "no-store")],
            )
                .into_response();
```

### Refutation

| finding | refuter | confidence | kept as |
|---|---|---|---|
| F1 views/ip.rs:396 feeds line in the live "All evidence" details | CONFIRMED | 8 | MAJOR |
| F2 pages.css:1201 commands wrap instead of scrolling | CONFIRMED | 7 | MAJOR |

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| F1 | CONFIRMED | 8 | held | read `evidence.rs` `panel` (slot with `block_feeds`) against `block_view_with` (`feeds: Vec::new()`) and `views/ip.rs` `addresses` |
| F2 | CONFIRMED | 7 | held | read `pages.css` `.ip-explorer__code` (`overflow-wrap: anywhere`, no `overflow-x`); Phase 4 shots at 390 px show the batch command on three lines |

Both hold as differences between the spec text and the built page, each a decision already taken in
the work (Phase 3 review NIT, Phase 4 reading V5) that the spec did not record; finish amends
requirements 14 and 16 and records the prefetch answer of requirement 3 (the minor).

### Roll call

| principle | answer |
|---|---|
| P03 | convergence: no codename or new status enum in the added lines |
| P10 | convergence: every error, refusal and timeout of the caller run is a notice; a PTR timeout drops only the host name |
| P12 | convergence: `bounded_reverse` and the caller run's `timeout_at` bound every new lookup |
| P13 | convergence: the caller GET takes the IP inspector's slot and its `[api.ip]` admission |
| P18 | convergence: no new outbound client; no config struct changed |
| P26 | absence: no new check id; the new blocks come from the ip module alone |
| P35 | absence: no rule under `specs/rules/` changed |
| P36 | absence: no generated file changed |
| P40 | absence: no module added or removed |

### Summary

Before refutation: reader 0/0/0; spec reader 0/2/1 (`REQUIREMENTS met=14 partial=2 missed=0
contradicted=0`). After refutation: 0 blockers, 2 majors, 1 minor (reader 0/0/0, spec reader
0/2/1). verified 2, held 2. Roll call: 9 answered, 5 convergence, 0 divergence, 4 absence.
