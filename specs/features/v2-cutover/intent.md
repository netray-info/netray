# Intent: v2-cutover

## Problem

After Phases 2, 3, 4a and 4 (`v2-history`, `v2-admission`, `v2-entries`, `v2-api`, `v2-spectrum`, `v2-pages`, `v2-result-page`, `v2-inspectors`), the V2 product exists, but it still runs beside the V1 shape. The binary has seven subcommands (`crates/netray/src/main.rs:26-83`), so production runs seven processes. There are six config files with six env prefixes (`LENS_`, `crates/lens/src/config.rs:475`; `NETRAY_<P>_`, e.g. `crates/dns/src/config.rs:369-372`). The six Solid apps, `packages/common-frontend` and the hand-written `site/` are still in the tree, and so is the snapshot store (`crates/lens/src/snapshot/`, `/r/{shortid}` at `crates/lens/src/routes.rs:1570`). Every metric has its own per-service namespace (`lens_*`, `prism_*`, `beacon_*`, …). Nothing sends the V1 URLs to their V2 successors. The workspace stands at `0.26.0` (`Cargo.toml:6`), and nothing of Phases 2–4 has been released.

## Proposed outcome

SDD Phase 5, V5.1–V5.6 (`specs/sdd/v2.md` §10), on the code side:

- `netray serve` is the only subcommand. One process serves every host by `Host`. The old routers, the Solid apps, `packages/common-frontend` and `site/` are deleted.
- One `netray.toml` under `deny_unknown_fields` with the env prefix `NETRAY_`. Every old config file, table and prefix is refused with a message that names its replacement.
- Every redirect and `410` of §7.3 and §7.2, each with its own test.
- The metrics of §9.2 under `netray_`, held by a registry test.
- The local agreement test of §9.1 and a narrow `just acceptance` project `prod`.
- `snapshots.db` code removed. V1 `/r/` IDs answer `410`.

Then a CHANGELOG whose claims are traced to code, the SDD amended for the operator's release decision of 2026-10-17 (only `0.30.0` is cut), the CLAUDE.md conventions corrected where the cutover makes them false, and `just release 0.30.0` run locally (never pushed).

## Affected users and systems

- Every visitor and API caller: V1 APIs answer `410`, V1 hosts redirect, and V1 snapshot links answer `410`.
- The operator:
  - one container, one config and one metrics target replace seven of each;
  - dashboards and alerts move to `netray_*` (S11);
  - the second baseline pull (V0.3) must run on argus before the deploy.
- In the repository: `crates/netray`, `crates/web`, and the module crates (their service halves go), `crates/lens` (deleted), the gate scripts under `tests/repo/`, the Playwright suite, `Dockerfile`, both workflows, `CLAUDE.md`, `README.md`, `specs/rules/`, `specs/sdd/v2.md` and `CHANGELOG.md`.
- netray-ops (private) changes, but not here: it gets a handoff list.

## Constraints

- Built last, after the eight Phase 2–4 features above are merged.
- S24: a `0.x` minor.
- S10 and S11: one config, one metrics namespace. Together they supersede CLAUDE.md "Config stays per service".
- The deploy waits for the second baseline pull (S2, S4, V0.3). The pull and the deploy belong to the operator.
- The release is local only. `just release` never pushes, and while the tag is unpushed nothing may run `git fetch`, because `fetch.pruneTags` is set.
- Only `netray.info` and `example.com` in tests and docs.

## Open questions

Resolved by the operator on 2026-10-17 (see the spec's Decision log):

- An unknown `Host` answers 404, and `www.` stays with Traefik.
- The bare field routes of `ip.` answer `410` pointing to `/api/v1/ip?field=`.
- The binary answers the `lens.` and `email.` redirects, each with a test row.
