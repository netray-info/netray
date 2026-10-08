# Spec: one CI, one release

Status: Done
Created: 2026-10-07
Finished: 2026-10-08

The repository builds as one workspace into one binary `netray` (`specs/features/monorepo-p1/spec.md`). It still carries the per-crate GitHub workflows of the repositories it came from: inert here, because GitHub only runs `.github/workflows/` at the root, but misleading. This spec gives the repository one CI workflow and one release workflow that publishes one image, retires the per-crate workflows and the rules written for them, and makes the production acceptance suite pass again.

Deploying is not this repository's job: a release builds and pushes an image; the infrastructure repository pins the tag and deploys it.

## Decisions

| # | Decision |
|---|---|
| D1 | One image `ghcr.io/netray-info/netray`. Tags are the exact version without `v` (`0.22.0`, `0.22.0-rc.1`); never `latest`, no moving or SHA tags. A tag that already exists in the registry is never overwritten. |
| D2 | Native `linux/arm64` only (production is ARM); GitHub's `ubuntu-24.04-arm` runners, no QEMU. |
| D3 | No deploy step, webhook or registry push outside `release.yml`. |
| D4 | The image bakes ifconfig-rs's GeoIP data from the private `ghcr.io/netray-info/ifconfig-rs-data:latest` at build time, under the working directory's `data/`, as the separate ifconfig-rs image did. |
| D5 | Scheduled scans (advisories, production acceptance) do not run from this public repository: GitHub disables scheduled workflows after 60 days without activity. They run elsewhere. |
| D6 | Release tags are `vX.Y.Z` and, for release tests, `vX.Y.Z-rc.N`. |

## Requirements

1. `.github/workflows/ci.yml` runs on every push to `main` and every pull request: one job on `ubuntu-24.04-arm` that installs the toolchains and `just`, runs `just adlc-setup`, then `just check`. Concurrency cancels superseded runs of the same ref.
2. `.github/workflows/release.yml` runs on pushed tags matching D6 (and `workflow_dispatch` from a tag ref). It builds the root `Dockerfile` natively for `linux/arm64`, starts every subcommand from the built image as a smoke test, and only then pushes exactly one tag, the version without `v`, to `ghcr.io/netray-info/netray`. It fails before building when that image tag already exists. It never pushes `latest`, calls no webhook and deploys nothing. Permissions: `contents: read`, `packages: write`, nothing else.
3. Every `uses:` in the two workflows pins a third-party action by its full 40-character commit SHA with the version as a trailing comment.
4. The root `Dockerfile` copies `/data` from `ghcr.io/netray-info/ifconfig-rs-data:latest` into `/netray/data`, so `netray ip` with a config whose data paths are `data/…` finds the GeoIP files when run from `/netray`.
5. No `.github/` directory exists below `crates/` or `packages/`.
6. `specs/rules/workflow-rules.md` describes this repository's two workflows and their rules (triggers, runner, pinning, permissions, image tags, no deploy, scheduled scans elsewhere) and nothing of the per-service matrix, deploy workflow or webhook it replaced.
7. The acceptance suite's `/api/meta` schema accepts lens's additional `site` object and rejects an empty ecosystem URL; every ecosystem URL is an absolute `https://` URL.

## Phase 1 — Workflows and image

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6

### Test Scenarios

- GIVEN the tree WHEN `.github/workflows/` at the root is listed THEN it holds exactly `ci.yml` and `release.yml`, and no `.github/` directory exists below `crates/` or `packages/`.
- GIVEN `ci.yml` WHEN read THEN it triggers on push to `main` and on `pull_request`, runs on `ubuntu-24.04-arm`, runs `just adlc-setup` and `just check`, and sets `concurrency` with `cancel-in-progress: true`.
- GIVEN `release.yml` WHEN read THEN it triggers on tags `v*.*.*` (accepting `-rc.N`) and `workflow_dispatch`; it targets `linux/arm64` on `ubuntu-24.04-arm`; its permissions are exactly `contents: read` and `packages: write`; it checks the registry for the tag before building; it smoke-tests every subcommand from the built image before the push step; its pushed tag is the version without `v`; it contains no `latest`, no `deploy`, and no `curl` to a hook.
- GIVEN both workflows WHEN every `uses:` line is read THEN each pins a 40-hex commit SHA followed by a `# v…` comment.
- GIVEN the root `Dockerfile` WHEN read THEN it has a stage from `ghcr.io/netray-info/ifconfig-rs-data:latest` and copies its `/data` to `/netray/data`, and the runtime stage's `WORKDIR` is `/netray`.
- GIVEN `specs/rules/workflow-rules.md` WHEN read THEN it names `ci.yml` and `release.yml`, and does not mention `deploy.yml`, `DEPLOY_WEBHOOK_SECRET`, `release-ref` or a per-service image.

## Phase 2 — Acceptance suite

**Depends on:** none
**Requirements:** 7

### Test Scenarios

- GIVEN the ecosystem-meta schema WHEN a lens `/api/meta` sample with a `site` object is validated THEN it passes.
- GIVEN the schema WHEN a sample with `"email_base_url": ""` is validated THEN it fails; WHEN a sample with `http://` or a relative URL is validated THEN it fails.
- GIVEN the suite WHEN `just acceptance` runs against production THEN it passes (operator-observed; network and browser, outside the gate).

## Open decisions

None.
