# Plan: monorepo-p2

## Phase 1 — Workflows and image

Planned by the orchestrator without the planner agent: three groups over disjoint files, fully named by the spec.

## Groups

- G1: C1, C2, C3, C5, C7, C8, C9, C10 — `.github/workflows/ci.yml`, `.github/workflows/release.yml`; `git rm -r` every `crates/*/.github` and `packages/common-frontend/.github`.
- G2: C4, C11 — root `Dockerfile` data stage.
- G3: C6, C12 — rewrite `specs/rules/workflow-rules.md` for the two workflows.

## Plan

### G1
- `ci.yml`: `on: push: branches: [main]`, `pull_request`; `concurrency: { group: ci-${{ github.ref }}, cancel-in-progress: true }`; one job `check` on `ubuntu-24.04-arm`, `timeout-minutes: 30`, `permissions: contents: read`; steps: checkout, Rust toolchain from `rust-toolchain.toml`, rust-cache, setup-node 22 with npm cache on the root `package-lock.json`, install `just`, `just adlc-setup`, `just check`. Every action pinned by full SHA + `# vX` comment (SHAs from the old per-crate workflows where the same version is used; look up the rest with `gh api`).
- `release.yml`: `on: push: tags: ['v*.*.*']`, `workflow_dispatch`; one job on `ubuntu-24.04-arm`, `permissions: { contents: read, packages: write }`; steps: checkout; version = `${GITHUB_REF_NAME#v}` validated against `^[0-9]+\.[0-9]+\.[0-9]+(-rc\.[0-9]+)?$`; GHCR login with `GITHUB_TOKEN` (also reads the private data image, M16); fail if `docker buildx imagetools inspect ghcr.io/netray-info/netray:$version` succeeds; build the root Dockerfile for `linux/arm64` with `load: true`, no push; smoke: run each subcommand (`lens dns tls http email ip`) from the image with its example config and the env overrides `tests/repo/test_smoke_services.sh` uses, plus `site --bind 0.0.0.0:8080`, and `docker exec … wget` `/health` (site: `/guide/`); then `docker push ghcr.io/netray-info/netray:$version`. No `latest`, no deploy, no hook.

### G2
- `FROM ghcr.io/netray-info/ifconfig-rs-data:latest AS data`; in the runtime stage `COPY --from=data /data /netray/data` before the `chown`; `WORKDIR /netray` stays.

### G3
- `specs/rules/workflow-rules.md` rewritten: the two workflows, triggers, runner (`ubuntu-24.04-arm`, native arm64, no QEMU), SHA pinning (no exemptions), least-privilege permissions, image tags (exact version, never `latest`, never overwrite), no deploy/webhook, scheduled scans run outside this public repository (60-day rule), GeoIP data image access.
