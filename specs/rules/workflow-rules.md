# Workflow Rules — netray

Canonical rules for the GitHub Actions workflows of this repository. Apply them
when creating or modifying any file under `.github/workflows/`.

There are exactly two workflows.

| File | Purpose |
|------|---------|
| `ci.yml` | Gate: `just adlc-setup` + `just check` |
| `release.yml` | Tag-driven: build, smoke-test and push the single `netray` image |

Everything else (scheduled scans, deployment) is outside this repository; see §6.

---

## §1 Triggers

```yaml
# ci.yml
on:
  push:
    branches: [main]
  pull_request:
```

```yaml
# release.yml
on:
  push:
    tags: ['v*.*.*']
  workflow_dispatch:
```

- R-T1: `main` is the default branch. Never `master`.
- R-T2: `ci.yml` MUST filter `push:` to `branches: [main]`. A bare `on: push:` is forbidden. `pull_request:` carries no branch filter.
- R-T3: `release.yml` triggers on tags matching `v*.*.*`, plus `workflow_dispatch`; its first step accepts only `vX.Y.Z` and `vX.Y.Z-rc.N` and fails on anything else. It MUST NOT trigger on branch pushes.
- R-T4: `release.yml` runs only from a tag ref: its first step fails when `GITHUB_REF_TYPE` is not `tag`, so a manual dispatch from a branch (even one named like a tag) publishes nothing.

---

## §2 `ci.yml`

One job on `ubuntu-24.04-arm`. Setup steps provide the Rust toolchain, the caches, Node, `just` and the GeoIP data (from the private `ifconfig-rs-data` image); then `just adlc-setup` and `just check` run. The justfile is the single definition of what the gate contains.

```yaml
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  check:
    runs-on: ubuntu-24.04-arm
    permissions:
      contents: read
      packages: read
    steps:
      - uses: actions/checkout@<sha> # vX.Y.Z
      # toolchain, rust-cache, setup-node, setup-just, GHCR login, GeoIP data
      - run: just adlc-setup
      - run: just check
```

- R-J1: One job. No matrix, no `needs:` chain.
- R-J2: Besides setup steps, `ci.yml` runs only `just adlc-setup` and `just check`. Test logic lives in the justfile, not in workflow steps.
- R-J3: `concurrency` cancels superseded runs of the same ref.
- R-J4: `permissions` is `contents: read` and `packages: read` (the GeoIP data image), nothing else.
- R-J5: `ci.yml` produces no artifacts and makes no external change.
- R-J7: The checkout fetches the full history (`fetch-depth: 0`): `check-sitemap` dates every page by its last commit, which a shallow clone does not have.
- R-J6: Advisory scans (RUSTSEC, `npm audit`, `cargo-deny check advisories`) never gate push or pull request. A new advisory in a transitive dependency must not block an unrelated merge or hotfix; it is triaged on its own cadence (§6). Deterministic checks (`cargo-deny check bans licenses sources`) belong to `just check`.

---

## §3 `release.yml`

One image, `ghcr.io/netray-info/netray`, built natively for arm64 on `ubuntu-24.04-arm`. The subcommand selects the service.

- R-R1: Native build on `ubuntu-24.04-arm`. No QEMU, no `setup-qemu-action`, no second platform.
- R-R2: The image tag is the exact version without the leading `v` (`v1.2.3` pushes `1.2.3`, `v1.2.3-rc.1` pushes `1.2.3-rc.1`). Never `latest`, never a floating `major` or `major.minor` tag.
- R-R3: An existing tag is never overwritten. Before building, the workflow asks GHCR for the tag and fails if it exists — and also when the answer is neither the tag nor "manifest unknown", so a registry error never leads to an overwrite.
- R-R4: Before pushing, every subcommand is smoke-tested from the built image (loaded locally, not yet in the registry). A failing smoke test means nothing is pushed.
- R-R5: No data file is ever baked into a published image: the GeoLite2 licence forbids redistributing the `.mmdb` files, and the other ifconfig-rs lists carry their own terms. The deployment mounts the data at `/netray/data`. Before pushing, `release.yml` fails when the built image contains a `*.mmdb` file or any file under `/netray/data`; only `ci.yml` reads the private `ifconfig-rs-data` image, to run the ifconfig-rs integration tests. Login is `docker/login-action` to `ghcr.io` with `username: ${{ github.actor }}` and `password: ${{ secrets.GITHUB_TOKEN }}`.
- R-R6: `release.yml` builds and publishes. It does not deploy and calls no webhook.
- R-R7: `concurrency` groups runs by `github.ref_name` with `cancel-in-progress: false`, so a second run for the same tag queues, then fails the existence check instead of overwriting the first image.

---

## §4 Permissions

- R-P1: `ci.yml`: `contents: read`, `packages: read`.
- R-P2: `release.yml`: exactly `contents: read` and `packages: write`.
- R-P3: No personal access token. `secrets.GITHUB_TOKEN` is the only credential (§5).

---

## §5 Secrets

| Name | Used for | Scope |
|------|----------|-------|
| `GITHUB_TOKEN` | GHCR login: read `ifconfig-rs-data` (ci only), push `netray` (release) | Auto-provided |

- R-S1: No other secret. A workflow that references any other secret is defective.

---

## §6 Actions and Scheduled Scans

- R-A1: Every action is pinned to a full commit SHA with the version as a comment: `uses: actions/checkout@<sha> # v4.2.2`. No exemptions: floating refs (`@stable`, `@v2`, `@main`) are forbidden.
- R-A2: Node is `22`. The Rust toolchain is `stable` with `rustfmt` and `clippy`, matching `rust-toolchain.toml`.
- R-A3: Scheduled scans — advisories and production acceptance — do not run from this public repository: GitHub disables scheduled workflows in public repositories after 60 days without activity. They run from a private repository.
