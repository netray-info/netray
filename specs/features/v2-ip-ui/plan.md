# Plan: v2-ip-ui

## Phase 1 — Blocks, caller on load and the address block

### Groups

G1: C1, C9, C10 · G2: C5, C6, C7, C17, C18, C19 · G3: C20 · G4: C2, C3, C4, C8, C11, C12, C13, C14, C15, C16, C21.
G1, G2 and G3 touch no common file; G4 needs G1 and G2. All Rust tests sit in the one `it`
binary, which compiles only once G1's functions exist, so G1 runs first (G3, TypeScript only,
beside it), then G2, then G4.

### Plan

G1 (`crates/ip/src/module.rs`, `specs/sdd/v2.md` §3.7)
1. `REGISTER` gains `ip.lookup` (Fields) with the requirement-1 keys and `ip.headers` (Tree) with
   `forwarded_for`, `name`, `value`.
2. `lookup_block(&Ifconfig) -> Block`: each key only when known, the nine flags always; `ua_*`
   only for a user agent, family "Other" and empty versions dropped.
3. `translate_lookup(ip, Result<Ifconfig, String>)`: `translate(&[(ip, r)], 1)`, evidence swapped
   to `[lookup_block]`, the reputation check referencing `ip.lookup`.
4. `headers_block(forwarded_for, headers)`: tree, root `forwarded_for` per hop, a leaf per header.
5. SDD §3.7 ip row amended.

G2 (`crates/web/src/pages/views/ip.rs`, `templates/partials/ip_card.html`, `ip_headers.html`,
`static/pages.css`, `static/inspector_shell.css`)
1. `render` dispatches `ip.lookup` and `ip.headers` (no qualifier).
2. `lookup`: `claim` over every key; address block (address, host name or "no reverse DNS",
   hidden `inspector-copy` buttons, DNS and TLS host links), badge row (`ip-badge`, fixed tones),
   the other keys in a `facts_grid` (replaced by Phase 2's grids); `frame(block, "Lookup", …)`.
3. `headers`: `claim_tree`; closed `<details>`, hops `<ol>` or "no proxy hops", name/value table.
4. CSS with tokens only; `.inspector-copy` in `inspector_shell.css`.

G3 (`islands/src/controls.ts`, `islands/src/shell.ts`, `islands/dist/islands.js`)
1. `unhide` reveals `.inspector-copy`; 2. the shell's click handler writes `data-copy-text` to the
   clipboard; 3. rebuild the bundle.

G4 (`api/ip.rs`, `api/jobs.rs`, `pages/inspect.rs`, `pages/mod.rs`, `partials/inspect_start.html`,
new `partials/inspect_caller_notice.html`, SDD §6.2)
1. `shown_headers`/`Shown` shared with `jobs.rs`.
2. `CallerRequest { remote, user_agent, headers }`; `InspectorJobs::caller(&self, req) ->
   Option<Result<Job, String>>`, default `None` (stub jobs in protected shell tests keep the idle
   page); re-exported from `pages`.
3. `start_ip`: `lookup_with` / `lookup_caller_with` with reverse DNS, `translate_lookup`.
4. `InspectJobs::caller`: off → `Some(Err(IP_OFF))`; else a job whose part yields `ip.lookup` and
   `ip.headers` and whose band is the reputation check; the lookup runs inside the future.
5. `StartFragment.own` → "Show my address" (`/inspect/ip`) in the run head for IP runs of a given
   address, streamed and shared.
6. `show`: for IP without prefill and `r`: a prefetch answers 503 `no-store`; `caller` → None idle;
   Err, busy slot, refused admission, timeout → caller notice (h1 address, notice); else the
   finished run (no share key, export ready). Every caller answer `private, no-store`.
7. Test change (orchestrator, ADLC-Test-Change): `inspect_ip_jobs.rs` anchor needle → `ev-ip.lookup`.
8. SDD §6.2 amended.
