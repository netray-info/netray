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

