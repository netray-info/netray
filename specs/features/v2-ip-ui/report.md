# Report: v2-ip-ui

## Phase 1 — Blocks, caller on load and the address block

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 ip.lookup fields block projected from one lookup, registered; inspector translation references it | green | crates/web/tests/it/ip_lookup_block.rs |
| C2 | R2 ip.headers tree block for the GET caller: forwarded_for hops, filtered headers in order | green | crates/web/tests/it/inspect_ip_headers.rs |
| C3 | R3 GET /inspect/ip renders the caller's run server-side, private no-store, refusals as notice with h1 address | green | crates/web/tests/it/inspect_ip_caller.rs |
| C4 | R4 ?ip= job uses lookup_with with reverse DNS; Show my address link; no caller parts | green | crates/web/tests/it/inspect_ip_address.rs |
| C5 | R5 the card is the IP view of ip.lookup with anchor ev-ip.lookup, every value rendered | green | crates/web/tests/it/views_ip_card.rs |
| C6 | R6 address block: address and host name with hidden copy buttons; host links to DNS and TLS | green | crates/web/tests/it/views_ip_address.rs |
| C7 | R7 badge row: version, EU, infra type, flag chips in fixed tones | green | crates/web/tests/it/views_ip_badges.rs |
| C8 | R15 no JavaScript: card renders, copy buttons hidden | green | crates/web/tests/it/inspect_ip_nojs.rs |
| C9 | S the stub document projects into ip.lookup with known keys only and validates | green | crates/web/tests/it/ip_lookup_block.rs |
| C10 | S Tor true fails ip.reputation with translate's findings, referencing ip.lookup | green | crates/web/tests/it/ip_lookup_block.rs |
| C11 | S GET as 192.0.2.10:54321 shows h1, card, band, checks, no share key, private no-store | green | crates/web/tests/it/inspect_ip_caller.rs |
| C12 | S GET calls lookup_caller_with once with port 54321, the User-Agent and reverse DNS | green | crates/web/tests/it/inspect_ip_caller.rs |
| C13 | S a caller over the per-IP budget gets 200, h1 address and the too-many-requests notice | green | crates/web/tests/it/inspect_ip_caller.rs |
| C14 | S no modules.ip: notice IP lookups are unavailable and h1 address | green | crates/web/tests/it/inspect_ip_caller.rs |
| C15 | S X-Forwarded-For hops and filtered headers in ip.headers in order | green | crates/web/tests/it/inspect_ip_headers.rs |
| C16 | S posted ip=198.51.100.1 uses lookup_with reverse DNS, Show my address, no caller parts | green | crates/web/tests/it/inspect_ip_address.rs |
| C17 | S the card carries id ev-ip.lookup and every value's display text | green | crates/web/tests/it/views_ip_card.rs |
| C18 | S host links to the DNS and TLS inspectors; copy buttons on address and host name | green | crates/web/tests/it/views_ip_address.rs |
| C19 | S eu, tor, vpn true read IPv4, EU, Tor fail, VPN warn | green | crates/web/tests/it/views_ip_badges.rs |
| C20 | S the shell island copies a copy button's text to the clipboard | green | crates/web/islands/test/shell-copy.test.ts |
| C21 | S without JavaScript the card renders and every copy button is hidden | green | crates/web/tests/it/inspect_ip_nojs.rs |
| C22 | S review blocker: a reverse lookup that never answers costs the host name, not the posted or GET card | green | crates/web/tests/it/inspect_ip_slow_ptr.rs |

C22 was added after the review (BLOCKER below); `inspect_ip_jobs.rs` (protected, v2-inspectors)
changes its anchor needle from `ev-ip.addresses` to `ev-ip.lookup` under `ADLC-Test-Change`, and
the phase-1 tests were rustfmt'ed in their own commit.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 53303 | 92 |
| G2 | 1 | sonnet | 57039 | 101 |
| G3 | 1 | sonnet | 31360 | 39 |
| G4 | 1 | opus | 103458 | 241 |

G4 went to opus directly (the largest group, four files and the show route). After the run the
full gate showed `inspect_backlink` red: the "Show my address" link reused the back link's class,
which that test counts; it got its own class `inspector-own` (orchestrator).

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | a reverse zone that never answers timed out the whole run: the PTR's 5 s timeout equals the IP deadline | fixed: `bounded_reverse` gives the reverse lookup three quarters of the time left, then looks up without it; test `inspect_ip_slow_ptr.rs` |
| AMENDMENT | a re-run with empty input loses port, user agent and headers the page load showed | requirement 4 amended: the POST is the island's fetch and carries none of them; repaired in phase |
| DEFERRED | the remaining facts are labelled with raw keys | Phase 2 (requirements 8–11) |
| NIT | `ip.headers` could carry an empty hop from a trailing comma | fixed: empty hops are dropped |
| NIT | the protected `inspect_ip_jobs.rs` needle change needs its trailer | carried by the phase commit |

### Behavioural verification

Local lens from this worktree (`lens.dev.toml`, no `[modules.ip]`: no GeoIP data locally), port 18977:

```
$ curl -s -D - http://127.0.0.1:18977/inspect/ip   # status, cache-control, result region
HTTP/1.1 200 OK
cache-control: private, no-store
<div id="inspect-result"><h1 class="inspector-run__target">127.0.0.1</h1><p class="inspector-notice">IP lookups are unavailable</p></div>
$ curl -s -o /dev/null -w "%{http_code}" -H "Sec-Purpose: prefetch;prerender" http://127.0.0.1:18977/inspect/ip
503
$ curl -s -D - -o /dev/null "http://127.0.0.1:18977/inspect/ip?ip=192.0.2.1" | grep -i cache-control
cache-control: no-cache
```

The card with data is verified over the stub lookup by the router tests; screenshots follow in
Phase 4.


## Phase 2 — Network, location, user agent and headers

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8 Network facts grid: ASN, organisation, prefix, network type and role, ASN category and registered, IANA label, cloud, VPN and bot providers, caller TCP source port | green | crates/web/tests/it/views_ip_network.rs |
| C2 | R9 Location facts grid: city, region with code, country with ISO code, continent, postal code, coordinates with accuracy, timezone, differing registered country | green | crates/web/tests/it/views_ip_location.rs |
| C3 | R10 OpenStreetMap link for known coordinates with rel noopener noreferrer | green | crates/web/tests/it/views_ip_location.rs |
| C4 | R11 User agent facts grid with browser, OS, device and raw header with copy button | green | crates/web/tests/it/views_ip_user_agent.rs |
| C5 | R12 Request headers: closed details, hops list or no proxy hops, name and value table in block order | already_implemented | crates/web/tests/it/views_ip_headers.rs |
| C6 | S ASN AS64496, organisation, prefix, role, category and registration date each in the Network grid | green | crates/web/tests/it/views_ip_network.rs |
| C7 | S cloud cell reads aws · EC2 · eu-central-1, or only aws | green | crates/web/tests/it/views_ip_network.rs |
| C8 | S VPN and bot provider cells only when named | green | crates/web/tests/it/views_ip_network.rs |
| C9 | S coordinates read 52.52, 13.4 ± 20 km and the map link targets OpenStreetMap | green | crates/web/tests/it/views_ip_location.rs |
| C10 | S no coordinates, no map link | green | crates/web/tests/it/views_ip_location.rs |
| C11 | S a registered country equal to the country has no cell | green | crates/web/tests/it/views_ip_location.rs |
| C12 | S tcp_port 54321 reads TCP source port; none without it | green | crates/web/tests/it/views_ip_network.rs |
| C13 | S Firefox 128 on Linux user agent grid with raw header and copy; none without keys | green | crates/web/tests/it/views_ip_user_agent.rs |
| C14 | S headers block with two hops and three headers in a closed details; no hop reads no proxy hops | already_implemented | crates/web/tests/it/views_ip_headers.rs |

C5 and C14 passed on the baseline: Phase 1 built the `ip.headers` view.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 51163 | 84 |

### Review

| class | finding | disposition |
|---|---|---|
| NIT | the raw header's copy button sits on its own line under the User agent grid, not in the cell | listed; the Phase 4 reading judges its placement |

### Behavioural verification

skipped: the grids render only with GeoIP data, which does not exist locally; the views are
verified over hand-built blocks by the router-free view tests, and Phase 4 screenshots the page
over the stub lookup.


## Phase 3 — API explorer and attribution

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R13 explorer section after the card with copyable curl commands built from ORIGIN and a docs link | green | crates/web/tests/it/views_ip_explorer.rs |
| C2 | R14 one partial renders GEOIP_ATTRIBUTION with MaxMind linked, following GeoIP-derived data | green | crates/web/tests/it/views_ip_attribution.rs |
| C3 | S explorer for 192.0.2.10 lists the short, ?ip= and API yaml commands with copy buttons and links /api/v1/docs | green | crates/web/tests/it/views_ip_explorer.rs |
| C4 | S the IP card's last element is the attribution with a MaxMind link | green | crates/web/tests/it/views_ip_attribution.rs |
| C5 | S ip.addresses on the evidence views and the live details is followed by the attribution | green | crates/web/tests/it/ip_attribution_pages.rs |
| C6 | S an HTTP inspector part with IP badges ends with the attribution; none without badges | green | crates/web/tests/it/ip_attribution_badges.rs |
| C7 | S no template holds the attribution text | already_implemented | crates/web/tests/it/views_ip_attribution.rs |
| C8 | S the stored result page IP tab follows its compact table with the attribution (added after planning: the compact table shows GeoIP data) | green | crates/web/tests/it/ip_attribution_compact.rs |
| C9 | S review blocker: an IP tab without ip.addresses renders no attribution | green | crates/web/tests/it/ip_attribution_no_data.rs |

C7 passed on the baseline (no template ever held the text). C8 and C9 were added after planning
and after the review. `views_ip_explorer.rs` (protected since this phase's test commit) changes
its batch command to `--json` under `ADLC-Test-Change` (review amendment).

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 + G2 (one test binary) | 1 | sonnet | 64165 | 100 |

The review fixes (attribution beside the compact table only, the claimed-block flag, `--json`)
were made by the orchestrator.

### Review

| class | finding | disposition |
|---|---|---|
| BLOCKER | the result page's IP tab showed the attribution on runs without GeoIP data | fixed: only beside the compact table, which exists only with `ip.addresses`; test `ip_attribution_no_data.rs` |
| AMENDMENT | the batch command with `-d` sends a form content type; the route answers 400 | requirement 13 amended to `curl --json`; repaired in phase |
| DEFERRED | the double-attribution guard searched the rendered part's text, which target content can contain | fixed: a flag set when a claimed IP block renders |
| NIT | the claimed `ip.addresses` carries its feeds line in the live result panel's "All evidence" details, which the generic block there omits | kept: every claimed view renders its feeds line (v2-http-ui requirement 2) |

On a live result page (no entry yet) the IP tab can show the attribution twice: under the compact
table and after `ip.addresses` inside the collapsed "All evidence" details; each follows its data.

### Behavioural verification

```
$ curl -s http://127.0.0.1:18977/example.com/ip    # local lens, seeded history, see Phase 1
```
skipped for the live server: the seeded fixtures' `ip.addresses` pages are covered by
`ip_attribution_pages.rs` (`/example.com/ip` through the real evidence router); Phase 4 reads
the pages in a browser.


## Phase 4 — Design gate and visual reading

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R16 design-check exits 0 with this feature; the IP inspector states read beside board G by an outside reader; findings closed; 390 px not wider than the viewport | green | design-check, reading below |
| C2 | S design-check exits 0 | green | design-check |
| C3 | S every finding of the reading beside board G is a report row and none stays open | green | reading below |
| C4 | S the IP inspector at 390 px is not wider than the viewport | green | measured below |

No test files: the phase's evidence is the design check, the reading and the measurement below.

### Behavioural verification

`NETRAY_DESIGN_DIR=/Users/lukas/Documents/src/netray.info/specs/design/canvas just design-check <manifest>`:
`0 open deviation(s), 0 stale accepted entries`; 12 boards, 865 elements, 1614 properties, run
before and after the reading's fixes. The manifest is the canvas manifest at netray.info
`bdf1de3`: the canvas head `29c44ea` adds rows for the Inspectors menu, the footer and board M's
menu sheet (a `click` key this branch's checker does not know, and footer markup), which belong to
`v2-site-nav`; against it the check refuses the manifest (exit 2), and without the two `click`
boards it reports 66 footer rows, none from this feature.

The IP inspector has no board. Its states were rendered on a local server over a stub lookup
(`StubIpLookup` wrapped to add every fact for the caller and no reverse DNS for 198.51.100.99; a
temporary example, not committed) and shot with Playwright at 1280 and 390 px in both themes:
caller with every fact, `?ip=2001:db8::1`, `?ip=198.51.100.99`, and the module off.

```
caller 390 light: scrollWidth 390 viewport 390     ipv6 390 light: scrollWidth 390 viewport 390
caller 390 dark:  scrollWidth 390 viewport 390     ipv6 390 dark:  scrollWidth 390 viewport 390
noptr 390 light:  scrollWidth 390 viewport 390     off 390 light:  scrollWidth 390 viewport 390
noptr 390 dark:   scrollWidth 390 viewport 390     off 390 dark:   scrollWidth 390 viewport 390
```

After the fixes no link, button or summary of these pages is under 44 px tall at 390 px.

### Visual reading beside board G

A reader with no part in the build compared the 16 screenshots with boards 17, 18, 28, 29 and 30.

| id | finding | status | disposition |
|---|---|---|---|
| V1 | two headings stacked: the part's h2 "lookup" and the card's title "Lookup" | green | rebuilt: the card's title reads "Address details"; the part heading over its views is the shell's, as on the HTTP inspector |
| V2 | the check list card has no line border, unlike the cards below | green | not this feature: the shell's live check list (v2-inspector-shell; v2-http-ui V9) |
| V3 | a lone "Lookup" link under the check card | green | not this feature: the shell's check row links each referenced block by its title |
| V4 | the command explorer had no heading and no card | green | rebuilt: a section "Use it from the command line" over a card, as the protocol views |
| V5 | long commands clipped at 390 px without a cue | green | rebuilt: a command wraps inside its card |
| V6 | targets under 44 px at 390 px (host and map links, MaxMind and docs links, summaries) | green | rebuilt: 44 px at phone width; measured above |
| V7 | the band's heading runs into its count | green | not this feature: the shell's band heading |
| V8 | the module-off notice is bare text under the h1 | green | kept: the notice is the shell's notice markup, as every inspector's refusal |
| V9 | slim one-line footer, rule wider than the column | green | not this feature: `v2-site-nav` (footer) |
| V10 | a "?" in the tab row; no caret on "Inspectors" | green | not this feature: the shell's help button; the menu is `v2-site-nav` |
| V11 | the raw header wraps in a third-width cell and its Copy was indented | green | rebuilt the indent; kept the cell: a full-width cell needs a `facts_grid` option (component change), proposal below |
| V12 | Network, Location and User agent are sub-headed grids in one card, not cards of their own | green | kept: one block, one card, one anchor and one feeds line (v2-http-ui requirement 2); proposal below |
| V13 | copy controls far right of their commands at 1280 px | green | rebuilt: the copy control follows its command |

Proposals for the operator: an "IP inspector" board (caller card with network, location, user
agent, request headers and the explorer); a full-width `facts_grid` cell for long values (raw user
agent, certificate chains).

### Review

| class | finding | disposition |
|---|---|---|
| AMENDMENT | at phone width `inline-flex` removed the summaries' disclosure marker | fixed: the summaries keep `list-item` and reach 44 px with `min-height` |
| AMENDMENT | the explorer's summary "Commands for this address" mislabels the caller's own commands | fixed: "Commands" |
| NIT | the explorer, styled as a protocol view, is a j/k stop though it is no evidence block | kept: one more stop between the card and the headers; the help text names evidence blocks |

