# Spec: v2-ip-ui

Status: Ready for Implementation
Created: 2026-10-20

## Goal

The IP inspector page is the page V1's `ip.` was (inventory theme G, UI half): it shows the
caller's own address and everything known about it as soon as the page loads, without pressing
Run, and the same card for any looked-up address. The card holds the address and the reverse-DNS
host name with copy controls and links to the DNS and TLS inspectors, version, EU and flag badges,
network details (ASN, organisation, prefix, network and infrastructure type, network role, ASN
category and registration date, IANA label, cloud, VPN and bot providers), location details (city,
region, country, continent, postal code, coordinates with accuracy, timezone, registered country,
a map link), and for the caller the TCP source port, a user-agent grid and the request headers
with the X-Forwarded-For chain. An API explorer lists copyable curl commands, and the MaxMind
GeoLite2 attribution follows GeoIP-derived data wherever it is rendered, as its licence requires.

## Non-goals

- The shell (header, tabs, form, history, help and shortcuts, permalink and share, export, badges
  on addresses in other inspectors): `v2-inspector-shell` (merged, #46).
- The CLI and API surface: `v2-ip-cli` is merged (rows 6.24–6.48); the `ip.` host mount of
  `ip_host` happens on `feature/v2-cutover`.
- The domain IP module's evidence (`ip.addresses`, its columns and the reputation check of a domain
  run): unchanged; stored views gain only the attribution line.
- The FAQ (row 6.21, changed: the `/guide/ip-enrichment` guide stays the explanation); localised
  place names in the UI (6.42 is a CLI row).
- The protocol-view seam, `facts_grid` and the slim inspector band: `v2-http-ui` (merged, #49).
- An "IP inspector" board on the canvas: a proposal for the operator (Decision log).

## Context and constraints

- Inventory (private, `netray.info/specs/inventory/sections/06-ip.md`): rows 6.1, 6.2, 6.3 (no
  "reset to my IP"), 6.5, 6.6 (UI half), 6.7, 6.8, 6.9, 6.10, 6.11, 6.12, 6.13, 6.14, 6.15, 6.16,
  6.17, 6.18, 6.19, 6.20, 6.51. Operator decision 2026-10-19: theme G "rebuild; MaxMind
  attribution is a licence duty regardless".
- Backend half, merged: `specs/features/v2-ip-cli/`: `IpLookup::lookup_with(ip, LookupOptions)`
  and `lookup_caller_with(remote, user_agent, LookupOptions)` with `reverse_dns`; the filtered
  request headers (`netray_ip::extractors::{extract_headers, filter_headers}` with
  `ApiState::header_filters`) and the X-Forwarded-For chain (`api/ip.rs` `shown_headers`);
  `netray_ip::GEOIP_ATTRIBUTION` ("This product includes GeoLite2 data created by MaxMind,
  available from https://www.maxmind.com.").
- The lookup document `netray_ip::backend::Ifconfig` `{ip {addr, version, hostname}, tcp {port},
  location {...}, network {...}, user_agent {raw, device, os, browser}}`; floats (latitude,
  longitude) have no evidence value type and are carried as text.
- Today's job (`crates/web/src/api/jobs.rs` `start_ip`): empty input looks up the caller with
  `lookup_caller(SocketAddr::new(addr, 0), None)`, other input with `lookup` (no reverse DNS),
  and renders `netray_ip::translate`'s `ip.addresses` table and the IP band. `/inspect/ip?ip=`
  prefills and autostarts; `GET /inspect/ip` renders the idle shell.
- Protocol views (`crates/web/src/pages/views/`, SDD §6.1): a view claims a block only when it
  renders every key the block holds, framed as a `section` with anchor `ev-<block id>` and feeds
  line; every renderer asks the views per block. `pages/views/ip.rs` is empty.
- V1 reference: `crates/ip/frontend/src/` (`IpDisplay.tsx` copy buttons and cross-links,
  `InfoCards.tsx` badge bar, network, location, map link and user-agent card,
  `RequestHeaders.tsx`, `ApiExplorer.tsx`, the footer attribution in `App.tsx`).
- SDD: §3.7 (blocks, register, data never presentation), §6.1, §6.2 (IP inspector: the caller's
  address or any address; deadline 5 000 ms; `[api.ip]` limiters shared with `/api/v1`; the
  caller's own address is never kept), §7.2.
- The one origin of absolute links: `crates/web/src/pages/mod.rs` `ORIGIN`.
- Board: none for the IP inspector. The page follows board G's frame through the shell; the card
  uses G's section cards and `facts_grid`.
- Locally there is no GeoIP data: tests run on `netray_ip::testing::StubIpLookup` (fixture
  `tests/fixtures/contracts/ifconfig-json.json`, host name `stub.example.com` with reverse DNS on)
  or on test lookups wrapping it.
- Test data: documentation addresses (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24,
  2001:db8::/32), `example.com` host names.

## Requirements

### Blocks

1. **Lookup block.** `netray_ip::module` projects one lookup document into a fields block
   `ip.lookup` (title "Lookup"), each key only when known, registered in `REGISTER`: `address`
   (addr), `hostname`, `tcp_port` (int), network `asn` (text `AS<n>`), `org`, `prefix`,
   `network_type`, `infra_type`, `asn_category`, `network_role`, `asn_registered`, `iana_label`,
   `cloud_provider`, `cloud_service`, `cloud_region`, `vpn_provider`, `bot_provider`, the flags
   `internal`, `datacenter`, `vpn`, `tor`, `bot`, `c2`, `spamhaus`, `anycast`, `cins` (bool,
   always present), location `city`, `region`, `region_code`, `country`, `country_iso`,
   `postal_code`, `eu` (bool), `latitude`, `longitude` (text), `accuracy_km` (int), `timezone`,
   `continent`, `continent_code`, `registered_country`, `registered_country_iso`, and the user
   agent `ua_raw`, `ua_browser`, `ua_browser_version`, `ua_os`, `ua_os_version`, `ua_device`
   (a family "Other" is unknown and omitted). The inspector's translation of one lookup yields
   `ip.lookup` in place of `ip.addresses` and the `ip.reputation` check with the same status and
   findings, referencing `ip.lookup`. SDD §3.7's ip row is amended. (6.1)
2. **Headers block.** For the caller loaded by `GET` (requirement 3), a tree block `ip.headers`
   (title "Request headers"): root key `forwarded_for` repeated per X-Forwarded-For hop, client
   first; one child per filtered request header in received order with `name` and `value`. The
   filter is the one `?field=headers` uses. (6.19)

### Loading

3. **Caller on load.** `GET /inspect/ip` without `ip` and without `r` renders the caller's run
   server-side, in the markup a finished live run leaves: the per-IP admission of `[api.ip]`
   (as a POST with empty input), then `lookup_caller_with` with the connection's address and
   source port (the trusted-proxy client address with port 0 when a proxy forwarded the request),
   the request's `User-Agent` and reverse DNS on, under the IP inspector's deadline; blocks
   `ip.lookup` and `ip.headers`, the band and check list, no share key. The answer carries
   `Cache-Control: private, no-store`. A refused admission, a timed-out lookup or an IP module
   that is off renders, with status 200, the caller's address as h1 and the reason as the notice
   ("too many requests; try again in <n> s", "timed out", "IP lookups are unavailable"). (6.1,
   6.5)
4. **Any address.** `?ip=X` keeps prefilling and autostarting (6.3); its job uses `lookup_with`
   with reverse DNS on and renders the same card without the caller-only parts (port, user agent,
   headers). A POST with empty input (a re-run) uses `lookup_caller_with` with reverse DNS on and
   shows the caller as that request sees it: no port, user agent or headers, which only the page
   load carries (the posted request is the island's fetch). The lookup with reverse DNS gets three
   quarters of the time left before the deadline; past it the address is looked up without reverse
   DNS, so a reverse zone that never answers costs the host name, never the card. A run of a given
   address shows a link "Show my address" to `/inspect/ip` in its head. (6.3, 6.6)
5. **Card replaces the table.** On the inspector the card is the IP view of `ip.lookup`
   (anchor `ev-ip.lookup`, so the check list's links land on it); it renders every value of the
   block (the nothing-lost rule of `v2-http-ui` requirement 3). The band and check list stay
   above it. (6.1)

### Address

6. **Address block.** The card opens with the address (mono) and under it the host name from
   reverse DNS or "no reverse DNS"; a "Copy" control beside each; the host name links to the DNS
   inspector (`/inspect/dns?q=<host>`) and the TLS inspector (`/inspect/tls?h=<host>:443`,
   percent-encoded). Copy controls are buttons carrying the text to copy, hidden until the shell
   island reveals them; the island writes the text to the clipboard. (6.2, 6.5, 6.7)
7. **Badges.** A badge row: "IPv4" or "IPv6"; "EU" or "non-EU" when `eu` is known; the
   infrastructure type; one chip per true flag: Tor, C2, Spamhaus, CINS (fail tone), VPN (warn
   tone), datacenter, anycast, bot, internal (muted), one fixed map from flag to tone, in the
   shell's badge chip (`ip-badge`). (6.8, 6.13)

### Network and location

8. **Network facts.** A `facts_grid` "Network": ASN, organisation, prefix, network type, network
   role, ASN category, ASN registered, IANA label; Cloud "<provider> · <service> · <region>"
   (each part only when known), VPN provider, bot provider; for the caller "TCP source port"; each
   cell only when its key is present. (6.9, 6.10, 6.11, 6.12, 6.17)
9. **Location facts.** A `facts_grid` "Location": city, region (with its code), country (with its
   ISO code), continent, postal code, coordinates "<lat>, <lon>" with " ± <n> km" when the
   accuracy is known, timezone, registered country when it differs from the country; each only
   when known. (6.14, 6.15)
10. **Map link.** When coordinates are known, "Open in OpenStreetMap ↗" links to
    `https://www.openstreetmap.org/?mlat=<lat>&mlon=<lon>#map=10/<lat>/<lon>` with
    `rel="noopener noreferrer"`. (6.16)

### Caller only

11. **User agent.** When the block holds user-agent keys, a `facts_grid` "User agent": browser
    and version, operating system and version, device, and the raw header with a copy control.
    (6.18)
12. **Request headers.** The IP view of `ip.headers` is a section "Request headers" holding a
    `<details>`, closed by default: the X-Forwarded-For chain as a numbered list of hops (client
    first), or "no proxy hops" when the root holds none, and a two-column table (name, value) of
    the headers in block order; a wide table scrolls inside its section. (6.19)

### API explorer

13. **API explorer.** After the card, a section "Use it from the command line" (a `<details>`,
    open): one row per command, each with a one-line description and a copy control, built from
    `ORIGIN` (its host prefixed with `ip.` for the short forms): `curl ip.<host>`, `curl
    ip.<host>/json`, `curl ip.<host>/country`, `curl "ip.<host>/?ip=<address>"`, `curl
    "<origin>/api/v1/ip/<address>?format=yaml"`, `curl
    "<origin>/api/v1/ip?fields=ip,location&format=json"`, `curl -X POST <origin>/api/v1/ip/batch
    -d '{"addresses":["<address>"]}'`, and a link to `/api/v1/docs`. Commands are server-rendered
    text. (6.20)

### Attribution

14. **MaxMind attribution.** One partial renders `netray_ip::GEOIP_ATTRIBUTION` in muted 12 px
    text with "MaxMind" linked to `https://www.maxmind.com`; no template holds the text. It
    follows GeoIP-derived data: as the last element of the IP card; after every `ip.addresses`
    block wherever it renders (the evidence views `/<domain>/ip` and `/r/<id>/ip`, the result
    page's IP tab and "All evidence" details), the block's markup otherwise the generic block's;
    and at the end of an inspector part whose addresses carry IP badges. Without GeoIP data
    (no such block, no badge) nothing renders. (6.51)

### Acceptance

15. **No JavaScript.** The caller's card, the explorer and the headers render without
    JavaScript; copy controls stay hidden, never broken. (S7)
16. **Design gate.** `just design-check` exits 0 with this feature in the stack; the IP inspector
    (caller with every fact, an IPv6 address, an address without reverse DNS, module off) at
    1280 px and 390 px, both themes, is read beside board G by a reader with no part in the
    build; every finding is a `report.md` row and is closed. At 390 px the document is not wider
    than the viewport (the headers table and long commands scroll inside their section).

## Phase 1 — Blocks, caller on load and the address block

**Requirements:** 1, 2, 3, 4, 5, 6, 7, 15

### Test Scenarios

GIVEN the stub document for 203.0.113.42 with reverse DNS WHEN projected THEN `ip.lookup` holds address 203.0.113.42, hostname stub.example.com, asn AS64496, cloud_provider aws, latitude 52.52, the nine flags, and no key for an unknown value, and it validates against `REGISTER`.
GIVEN a lookup with Tor true WHEN the inspector translation runs THEN the `ip.reputation` check fails, its findings match `translate`'s, and it references `ip.lookup`.
GIVEN the stub and a caller 192.0.2.10 port 54321 with a User-Agent WHEN `GET /inspect/ip` THEN the page shows 192.0.2.10 as h1, the card with `stub.example.com`, the IP band and the check list, without a share key, with `Cache-Control: private, no-store`.
GIVEN a recording lookup WHEN `GET /inspect/ip` THEN `lookup_caller_with` was called once with port 54321, the User-Agent and reverse DNS on.
GIVEN a caller over the per-IP budget WHEN `GET /inspect/ip` THEN status 200, the h1 is the address and the notice reads "too many requests; try again in".
GIVEN no `[modules.ip]` WHEN `GET /inspect/ip` THEN the notice reads "IP lookups are unavailable" and the h1 is the caller's address.
GIVEN a request with `X-Forwarded-For: 192.0.2.10, 198.51.100.7` from the trusted proxy WHEN `GET /inspect/ip` THEN `ip.headers` lists the hops 192.0.2.10 then 198.51.100.7 and the filtered headers in received order.
GIVEN `ip=198.51.100.1` posted WHEN the run ends THEN `lookup_with` was called with reverse DNS on, the card shows `stub.example.com`, the head links "Show my address" to `/inspect/ip`, and no TCP port, user agent or headers render.
GIVEN the card WHEN rendered THEN it carries `id="ev-ip.lookup"` and the display text of every value of the block.
GIVEN the host name `stub.example.com` WHEN rendered THEN it links to `/inspect/dns?q=stub.example.com` and `/inspect/tls?h=stub.example.com%3A443`, and address and host name each carry a hidden copy button with their text.
GIVEN a block with eu true, tor true and vpn true WHEN the badges render THEN they read IPv4, EU, Tor in the fail tone and VPN in the warn tone.
GIVEN the shell island and a copy button WHEN clicked THEN the clipboard holds the button's text.
GIVEN no JavaScript WHEN `GET /inspect/ip` THEN the card renders complete and every copy button carries `hidden`.

## Phase 2 — Network, location, user agent and headers

**Requirements:** 8, 9, 10, 11, 12

### Test Scenarios

GIVEN a block with ASN AS64496, organisation, prefix, role, category and registration date WHEN rendered THEN the Network grid shows each.
GIVEN cloud provider, service and region WHEN rendered THEN the cell reads "aws · EC2 · eu-central-1"; GIVEN only the provider THEN only "aws".
GIVEN a VPN provider and a bot provider WHEN rendered THEN both cells show; GIVEN neither THEN neither cell exists.
GIVEN latitude 52.52, longitude 13.4 and accuracy 20 WHEN rendered THEN the Location grid reads "52.52, 13.4 ± 20 km" and the map link targets `https://www.openstreetmap.org/?mlat=52.52&mlon=13.4#map=10/52.52/13.4` with `rel="noopener noreferrer"`.
GIVEN no coordinates WHEN rendered THEN no map link exists.
GIVEN a registered country equal to the country WHEN rendered THEN no registered-country cell exists.
GIVEN tcp_port 54321 WHEN rendered THEN the Network grid reads "TCP source port" and 54321; GIVEN no tcp_port THEN no port cell.
GIVEN user-agent keys for Firefox 128 on Linux WHEN rendered THEN the User agent grid names Firefox 128, Linux and the raw header with a copy button; GIVEN no user-agent key THEN no User agent grid.
GIVEN an `ip.headers` block with two hops and three headers WHEN rendered THEN a closed details element lists the hops in order and the headers in block order; GIVEN no hop THEN it reads "no proxy hops".

## Phase 3 — API explorer and attribution

**Requirements:** 13, 14

### Test Scenarios

GIVEN the card for 192.0.2.10 WHEN the explorer renders THEN it lists `curl ip.netray.info`, `curl "ip.netray.info/?ip=192.0.2.10"` and `curl "https://netray.info/api/v1/ip/192.0.2.10?format=yaml"`, each with a copy button, and links `/api/v1/docs`.
GIVEN the IP card WHEN rendered THEN its last element is the attribution with the constant's text and a link to https://www.maxmind.com.
GIVEN an entry holding `ip.addresses` WHEN `/<domain>/ip` and `/r/<id>/ip` render THEN the block's generic markup is followed by the attribution; GIVEN the result page's live "All evidence" details THEN the IP block is followed by it.
GIVEN an HTTP inspector part whose server address carries IP badges WHEN rendered THEN the part ends with the attribution; GIVEN no badges THEN no attribution renders.
GIVEN the templates WHEN scanned THEN none holds the attribution text.

## Phase 4 — Design gate and visual reading

**Requirements:** 16

`just design-check` on the local stack; screenshots of the IP inspector states on a local server
over the stub lookup; the reader's pass beside board G; results under `### Behavioural
verification` in `report.md`.

### Test Scenarios

GIVEN the manifest WHEN `just design-check` runs THEN it exits 0.
GIVEN the screenshots WHEN read beside board G THEN every finding is a `report.md` row and none stays open.
GIVEN the IP inspector at 390 px WHEN measured THEN the document is not wider than the viewport.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7
  (operator, 2026-10-19).
- Map link to OpenStreetMap (no tracking, no API key) and the caller's card rendered on GET
  confirmed (operator, 2026-10-20).
- The caller's card renders on `GET /inspect/ip`: row 6.1 asks for data on load, the request
  headers and user agent of the page load are the ones that matter (a later `POST` carries the
  fetch's headers), and the lookup is local databases plus one PTR query under the 5 s deadline
  and the per-IP limiter. P06 (runs start on POST) protects domain runs, their throttle and
  history; an IP lookup of the caller starts no run and stores nothing, as `curl ip.` answers on
  GET (drafting agent, 2026-10-20).
- `?ip=` keeps the shell's autostart (row 6.3 kept): an arbitrary address is a job the viewer
  asked for, and a crawled link should not trigger a PTR query on GET (drafting agent,
  2026-10-20).
- The card is fed by blocks, not by the lookup document handed to a view: a new inspector-only
  fields block `ip.lookup` and, for the caller, a tree block `ip.headers`. Views stay readings of
  blocks (SDD §3.7, §6.1), and the exports, the shared run of a looked-up address (`?r=`) and the
  re-stream after the band work unchanged. The inspector run carries `ip.lookup` in place of
  `ip.addresses` (one card, no duplicate table) and `ip.reputation` references it; domain runs
  keep `ip.addresses`. The card's anchor is therefore `ev-ip.lookup`, and the protected test
  `inspect_ip_jobs.rs` changes its anchor needle (implementer under the operator's autonomy
  mandate, 2026-10-21).
- The headers of the caller ride in `ip.headers` rather than beside the blocks; it is rendered
  only on the GET page, whose run is never kept, so nothing about a caller's headers is stored
  (implementer under the operator's autonomy mandate, 2026-10-21).
- The attribution is decided by the data on the page, not by configuration: it follows every
  `ip.addresses` and `ip.lookup` block and every inspector part with IP badges, all of which exist
  only when GeoIP data was there. The IP view claims `ip.addresses` to append it, keeping the
  generic block's markup (implementer under the operator's autonomy mandate, 2026-10-21).
- Review of phase 1: a reverse zone that never answers would have timed out the whole run (the
  PTR's own 5 s timeout equals the inspector's deadline); the reverse lookup now has a budget of
  three quarters of the time left and falls back to a lookup without it (requirement 4). A re-run
  with empty input is the caller as the island's fetch sees it, without port, user agent or headers
  (requirement 4 amended) (implementer under the operator's autonomy mandate, 2026-10-21).
- The explorer is always open: the IP view cannot tell the caller's card from a looked-up one
  by its block, and on both pages the commands are the point (implementer under the operator's
  autonomy mandate, 2026-10-21).
- Commands use `pages::ORIGIN`, the one origin of absolute links on main; `v2-site-nav` may
  change how that value is set, and the explorer follows it (implementer under the operator's
  autonomy mandate, 2026-10-21).
- The map link goes to OpenStreetMap instead of V1's Google Maps: same function, no tracking
  parameters, in line with `/privacy` (drafting agent, 2026-10-20).
- The attribution sits with the data, not in the site footer of every page: the GeoLite2 licence
  asks for it where the data is used, and one partial rendering the constant keeps every
  placement identical (drafting agent, 2026-10-20).
- Badge tones are one fixed map from flag to tone (threat flags fail, VPN warn, others muted), as
  the shell's badges; the graded verdict stays the reputation check (drafting agent, 2026-10-20).
- Board: none for the IP inspector; it follows board G's frame. Proposal for the operator: add an
  "IP inspector" board (caller card with network, location, user agent, headers, explorer) to the
  canvas (drafting agent, 2026-10-20).

## Open decisions

None.

## Out of scope

- Shell parts and the badges on other inspectors: `v2-inspector-shell`.
- CLI and API: `v2-ip-cli` (merged) and the `ip.` mount on `feature/v2-cutover`.
- Rows 6.21 (FAQ) and 6.42 (localised names in the UI).
