# Intent: v2-api

## Problem

netray has no stable API. Each service has its own API with its own error and format
conventions: lens's `/api/check`, prism's `/api/query…`, tlsight's and spectra's `/api/inspect`,
beacon's `/inspect`, and ifconfig-rs's roughly thirty routes in five formats. None of these
documents is a contract. They change from release to release, and no test catches a removed or
retyped field. The history that `v2-history` records, together with its diffs, cannot be reached
by any client. The `ip.` functions are spread across route families and format suffixes, which
S23 retires.

## Proposed outcome

There is one versioned API under `netray.info/api/v1`, SDD Phase 3 (V3.1–V3.4, §7.1, §7.2, S23).
It serves:

- the stored history entries and their diffs, as stored
- fresh domain runs under v2-admission's admission
- the four inspector jobs, the email inspection, every `ip.` function, the catalogue, the profiles and the meta
- an interactive docs page at `/api/v1/docs` over the document (architecture-rules)

Its OpenAPI 3.1 document is committed and is the contract. A test validates every endpoint's
responses against that document. A second test fails when a change removes or retypes a field or
an endpoint. `curl ip.netray.info` still answers the caller's address as text. No API handler
computes a verdict. The V1 APIs keep running unchanged until `v2-cutover`.

## Affected users and systems

- API clients and scripts: they get a new, stable surface, and their V1 calls keep working.
- The monorepo:
  - `netray-web`, whose module `crates/web/src/api/` holds API v1 (SDD §3.1)
  - the lens process, which mounts the API until the cutover
  - the netray-dns, -tls, -http, -email and -ip crates, which expose their jobs as functions shared by the old and the new route
  - `netray-model` and `netray-engine`, which gain schema derives; `RunCheck` gains findings and evidence references
  - lens config `[api]`
  - SDD §3.1, §7.1 and §10
- netray-ops: routes `/api/v1` on the apex to lens until the cutover. That change is outside this repository.

## Constraints

- Built after `v2-history`, `v2-admission` and `v2-entries`. The domain and entry endpoints read their store, run path and 410 rule.
- Ships in local `0.30.0`, the only release (operator, 2026-10-17). The build runs autonomously overnight.
- Every rule in `specs/rules/architecture-rules.md` applies: utoipa 5, one error shape, `Retry-After` on 429, and the security headers.
- `tests/repo/test_lens_names_no_module.sh` stays green; `test_web_crate.sh`'s Phase 4a guards are lifted under `ADLC-Test-Change` (operator, 2026-10-17, X1).
- V1 wire, lens goldens and contract goldens stay byte-identical.

## Open questions

Resolved by the operator on 2026-10-17 (see the spec's decision log): API v1 in `netray-web`; inspector settings in `[api.<inspector>]` of the lens config; no `starttls` in v1.
