# Report: monorepo-p2

## Phase 1 — Workflows and image

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Req 1: `ci.yml` on push to main and PR, one job on `ubuntu-24.04-arm`, toolchains + just, `just adlc-setup`, `just check`, concurrency cancels superseded runs | green | tests/repo/test_workflows.sh |
| C2 | Req 2: `release.yml` on D6 tags + dispatch; native arm64 build; smoke every subcommand from the image; push only `<version without v>`; fail before build if tag exists; no latest, no webhook, no deploy; permissions contents read + packages write | green | tests/repo/test_workflows.sh |
| C3 | Req 3: every `uses:` pinned by 40-hex SHA with `# v…` comment | green | tests/repo/test_workflows.sh |
| C4 | Req 4: Dockerfile copies `/data` from `ifconfig-rs-data:latest` to `/netray/data` | green | tests/repo/test_image_data.sh |
| C5 | Req 5: no `.github/` below `crates/` or `packages/` | green | tests/repo/test_workflows.sh |
| C6 | Req 6: `workflow-rules.md` describes the two workflows only | green | tests/repo/test_workflow_rules.sh |
| C7 | GIVEN tree THEN root workflows exactly `ci.yml`, `release.yml`; no nested `.github/` | green | tests/repo/test_workflows.sh |
| C8 | GIVEN `ci.yml` THEN triggers, runner, `just adlc-setup`/`just check`, concurrency cancel | green | tests/repo/test_workflows.sh |
| C9 | GIVEN `release.yml` THEN triggers, arm64 runner/platform, exact permissions, registry check before build, smoke before push, version tag without v, no latest/deploy/curl hook | green | tests/repo/test_workflows.sh |
| C10 | GIVEN both workflows THEN every `uses:` pins a 40-hex SHA + `# v…` | green | tests/repo/test_workflows.sh |
| C11 | GIVEN Dockerfile THEN data stage from `ifconfig-rs-data:latest`, `/data` → `/netray/data`, runtime `WORKDIR /netray` | green | tests/repo/test_image_data.sh |
| C12 | GIVEN `workflow-rules.md` THEN names `ci.yml`/`release.yml`, no `deploy.yml`, `DEPLOY_WEBHOOK_SECRET`, `release-ref`, per-service image | green | tests/repo/test_workflow_rules.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 workflows | 2 | sonnet; the orchestrator then hardened the tag-existence check (a registry error no longer counts as "absent"), made the `ip` smoke use the baked GeoIP data, and gave CI the data image so the ifconfig-rs integration tests run there | 47,824 | 79 |
| review fixes (orchestrator) | — | fetch-depth, concurrency, ref-type guard, cargo-deny in check, deny.toml | — | — |
| G2+G3 Dockerfile data, workflow rules | 1 | sonnet; rules aligned with the real workflows by the orchestrator (tag glob, CI setup steps and `packages: read`, toolchain source, login user, an invented `CARGO_TERM_COLOR` rule removed) | 39,527 | 25 |

### Review

First pass: 2 BLOCKER, 1 AMENDMENT, 1 NIT.

| finding | class | resolution |
|---|---|---|
| CI checkout `fetch-depth: 1` makes `check-sitemap` fail on every run (lastmod from git history) | BLOCKER | `fetch-depth: 0`; test first |
| no `concurrency` in `release.yml`: two runs for one tag both pass the existence check, the second overwrites | BLOCKER | per-ref group, `cancel-in-progress: false`; test first |
| cargo-deny bans/licenses/sources lost in the move; R-J6 claimed it runs in `just check` | AMENDMENT | root `deny.toml` (union of the per-crate allow-lists), `deny` recipe in `check`, cargo-deny installed in CI; test first (repaired) |
| `just image` now needs a GHCR login for the private data image | NIT | stated in the closing prose |

Second pass: 0 BLOCKER.
- AMENDMENT: a branch named like a tag could publish and escape the tag's concurrency group. Repaired: the run refuses non-tag refs and groups by `ref_name`; test first.
- AMENDMENT: `just check` needs cargo-deny, but `adlc-setup` did not provide it. Repaired: `adlc-setup` installs it when missing.
- DEFERRED → repaired: the root `deny.toml` had dropped the triaged `RUSTSEC-2025-0134` ignore of four crates. Restored.
- Sound: PR merge checkouts keep correct lastmod; the root allow-list equals the union; install-action resolves cargo-deny for aarch64; all SHAs exist upstream.

### Behavioural verification

skipped: both workflows run only on GitHub runners and there is no Docker daemon here. Verified statically (YAML parses, `tests/repo/test_workflows.sh`, `cargo deny check bans licenses sources` passes locally). The behavioural check is the spec's acceptance: CI on the first push and the `v0.22.0-rc.1` release run (decision M17 of the planning SDD), both operator-triggered.

## Phase 2 — Acceptance suite

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Req 7: meta schema accepts lens's `site` object, rejects empty ecosystem URLs; every ecosystem URL absolute `https://` | green | tests/repo/test_meta_schema.sh |
| C2 | GIVEN schema WHEN lens sample with `site` validated THEN passes | green | tests/repo/test_meta_schema.sh |
| C3 | GIVEN schema WHEN `email_base_url: ""` / `http://` / relative THEN fails | green | tests/repo/test_meta_schema.sh |
| C4 | GIVEN suite WHEN `just acceptance` against production THEN passes (operator-observed, outside the gate) | test-unwritable | tests/acceptance (operator-observed) |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| schema (`site`, https URLs, ajv declared) | — | orchestrator (three-line schema change; one escaping slip, caught by the test) | — | — |

### Production acceptance (2026-10-08, `TEST_ENV=production npx playwright test`)

89 tests: 72 passed, 10 skipped, 7 failed.
- 5 × `integration/cross-links.spec.ts` (browser context): local environment. Playwright's `chromium_headless_shell-1217` download hangs on this machine; no product finding.
- `static-site/links.spec.ts`: stale since the apex became the lens SPA (no SuiteNav in its HTML). Now reads the static `/tools` page and passes against production (36 passed in its project run).
- `static-site/pages.spec.ts`: `/status/` lacked `<meta name="description">`. Fixed in `site/status/index.html` and verified through `netray site`. Production serves the site frozen (infrastructure Phase 3b replaces it with `netray site`), so the fix goes live then.

C4 is `test-unwritable` in the gate: it needs network, a browser and a deployment; it is observed, not gated.

### Review

First pass: 1 BLOCKER, 1 DEFERRED, 2 NIT.
- BLOCKER: the https-only schema rejected ifconfig-rs's real `/api/meta`, which sends "" for the sibling URLs it does not link to. Repaired test-first with a live fixture: if/then/else on `site_name: "ip.netray.info"` allows "" for ifconfig-rs's siblings only, never for its own `ip_base_url`; every other service needs absolute https.
- DEFERRED → repaired: no acceptance spec validated `/api/meta` against the schema (planning N1). New `api-contract/meta-shape.spec.ts` checks all six services; it passes against production (6/6).
- NIT → repaired with the blocker's lines: URL pattern end-anchored; `site` description named a non-existent type.

Second pass: 0 BLOCKER.
- AMENDMENT: the exception keys on a configurable `site_name`. Kept, and documented in the schema: a rename makes the check fail loudly. A structural rule ("only ip set") would let a misconfigured service of that shape through.
- AMENDMENT: netray-common's contract doc still said every non-lens service sends empty strings. Rewritten: every service fills its URLs from `[ecosystem]`, with the ifconfig-rs exception.
- NIT, listed: the host part accepts degenerate hosts (`https://?`, `https://https://…`).
- NIT, listed: the repo test mutates only one key per branch.
- NIT, listed: `validate_meta.mjs` exits 1 on a mistyped `--set` path.
- NIT, listed: `site` is allowed on every service.

### Behavioural verification

```
$ TEST_ENV=production npx playwright test api-contract/meta-shape.spec.ts static-site/links.spec.ts
… ip/dns/tls/http/email/lens: /api/meta matches the ecosystem-meta schema — 6 passed
… all SuiteNav links on /tools resolve to 200 — passed
$ netray site --root site; curl /status/ | grep description
<meta name="description" content="Live uptime for the seven netray.info services." />
```
