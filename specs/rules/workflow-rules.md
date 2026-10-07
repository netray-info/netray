# Workflow Rules — netray.info Suite

Canonical rules for all GitHub Actions workflows across every subproject.
Apply these rules when creating or modifying any workflow file.

---

## §1 Workflow Files per Project Type

### Binary service (Rust + SolidJS frontend, produces Docker image)
`ifconfig-rs`, `mhost-prism`, `tlsight`, `spectra`, `lens`

| File | Purpose |
|------|---------|
| `.github/workflows/ci.yml` | PR gate: lint, test, cargo-deny (bans, licenses, sources) |
| `.github/workflows/audit.yml` | Daily schedule + manual: advisory scans (RUSTSEC, npm audit) |
| `.github/workflows/release.yml` | Tag push: test gate → Docker build → merge manifests → upload release ref |
| `.github/workflows/deploy.yml` | Fires after release succeeds → webhook to deploy.netray.info |

### Rust library (crates.io)
`netray-common`

| File | Purpose |
|------|---------|
| `.github/workflows/ci.yml` | PR gate: lint, test, cargo-deny (bans, licenses, sources) |
| `.github/workflows/audit.yml` | Daily schedule + manual: advisory scans (RUSTSEC) |

No release automation. Publish to crates.io is a manual `cargo publish` on the developer's machine.

### npm package
`netray-common-frontend`

| File | Purpose |
|------|---------|
| `.github/workflows/ci.yml` | PR gate: lint, test |
| `.github/workflows/publish.yml` | Tag push: test, publish to GitHub Packages |

---

## §2 Triggers

### `ci.yml` — always:
```yaml
on:
  push:
    branches: [main]
  pull_request:
```
`pull_request:` without a branch filter is correct — it fires on PRs targeting any branch, which is the desired behaviour.

### `release.yml` — always:
```yaml
on:
  push:
    tags: ['v*.*.*']
  workflow_dispatch:
```

### `deploy.yml` — always:
```yaml
on:
  workflow_run:
    workflows: [Release]
    types: [completed]
  workflow_dispatch:
    inputs:
      tag:
        description: 'Tag to deploy (e.g. v1.2.3)'
        required: true
```

### `audit.yml` — always:
```yaml
on:
  schedule:
    - cron: '0 6 * * *'
  workflow_dispatch:
```
Never `push:` or `pull_request:` — see R-J6.

### `publish.yml` (npm only) — same as release.yml.

**Rules:**
- R-T1: All repos use `main` as the default branch. Never `master`.
- R-T2: `ci.yml` MUST list `branches: [main]` under `push:` to avoid running on every branch push. The bare `on: push:` without a branch filter is forbidden.
- R-T3: `release.yml` triggers on semver tags only (`v*.*.*`). It also allows `workflow_dispatch:` for manual re-runs. It MUST NOT trigger on branch pushes — CI and release are separate workflows.
- R-T4: The `prod` branch pattern (lens legacy) is forbidden. Release is always tag-driven.
- R-T5: `deploy.yml` triggers on `workflow_run` (automatic) and `workflow_dispatch` with a `tag` input (manual re-deploy). The `workflow_run` name MUST match the `name:` field of `release.yml` exactly (case-sensitive).
- R-T6: When `release.yml` is manually dispatched via `workflow_dispatch`, it MUST be run from a tag ref, not a branch. Dispatching from `main` produces no semver tags (metadata-action emits only a short SHA tag) and will write `main` into the `release-ref` artifact, causing `deploy.yml` to call the webhook with `ref=main`, which the deploy server will reject.

---

## §3 Job Structure for `ci.yml` (binary services)

Five parallel jobs. No `needs:` dependencies between them — all run concurrently.

```
fmt      clippy      test      frontend      deny
```

### `fmt`
```yaml
fmt:
  name: Cargo Fmt
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@<sha> # v4.x.x
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: rustfmt
    - run: cargo fmt -- --check
```
No cache — fmt is fast and doesn't compile.

### `clippy`
```yaml
clippy:
  name: Clippy
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@<sha> # v4.x.x
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: clippy
    - uses: Swatinem/rust-cache@<sha> # v2.x.x
    - uses: actions/setup-node@<sha> # v4.x.x
      with:
        node-version: '22'
        cache: npm
        cache-dependency-path: frontend/package-lock.json
    - name: Build frontend
      run: npm ci && npm run build
      working-directory: frontend
      env:
        NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    - run: cargo clippy --locked -- -D warnings
```
Frontend must be built before clippy because `rust-embed` resolves `frontend/dist/` at compile time.

### `test`
```yaml
test:
  name: Test
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@<sha> # v4.x.x
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@<sha> # v2.x.x
    - uses: actions/setup-node@<sha> # v4.x.x
      with:
        node-version: '22'
        cache: npm
        cache-dependency-path: frontend/package-lock.json
    - name: Build frontend
      run: npm ci && npm run build
      working-directory: frontend
      env:
        NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    - run: cargo test --locked --no-fail-fast
```

### `frontend`
```yaml
frontend:
  name: Frontend
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@<sha> # v4.x.x
    - uses: actions/setup-node@<sha> # v4.x.x
      with:
        node-version: '22'
        cache: npm
        cache-dependency-path: frontend/package-lock.json
    - name: Build
      run: npm ci && npm run build
      working-directory: frontend
      env:
        NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    - name: Lint
      run: npm run lint
      working-directory: frontend
    - name: Test
      run: npm test
      working-directory: frontend
```
No Rust toolchain — frontend job is Node-only.

### `deny`
```yaml
deny:
  name: Cargo Deny (bans/licenses/sources)
  runs-on: ubuntu-latest
  permissions:
    contents: read
  steps:
    - uses: actions/checkout@<sha> # v4.x.x
    - uses: EmbarkStudios/cargo-deny-action@v2
      with:
        command: check bans licenses sources
```
Deterministic supply-chain checks only. Advisories are not in the gate.

### `audit.yml` (separate workflow, scheduled)
```yaml
jobs:
  rust:
    name: Rust advisories
    runs-on: ubuntu-latest
    permissions:
      contents: read
      checks: write
    steps:
      - uses: actions/checkout@<sha> # v4.x.x
      - uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
          # Kept in sync with deny.toml [advisories].ignore
          ignore: RUSTSEC-YYYY-NNNN
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check advisories
  npm:
    name: npm advisories
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: read
    steps:
      - uses: actions/checkout@<sha> # v4.x.x
      - uses: actions/setup-node@<sha> # v4.x.x
        with:
          node-version: '22'
          registry-url: https://npm.pkg.github.com
          cache: npm
          cache-dependency-path: frontend/package-lock.json
      - name: npm audit
        run: npm audit --audit-level=high --omit=dev
        working-directory: frontend
        env:
          NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```
`checks: write` is required by `rustsec/audit-check` to post check annotations.

**Rules:**
- R-J1: Five parallel jobs for binary services. No serial dependency chain in CI.
- R-J2: Jobs that compile Rust MUST build the frontend first (rust-embed compile-time requirement).
- R-J3: Every `npm ci` step MUST set `NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` as an env var. Missing this causes E401 from `https://npm.pkg.github.com`.
- R-J4: `cargo test` always uses `--locked --no-fail-fast`. Never add `--lib` unless the project has a dedicated `integration-test` job that runs those tests with required data files (e.g. ifconfig-rs, which needs GeoIP databases only available in the Docker-based integration job). In that case, the `test` job uses `--lib` and the `integration-test` job runs the full suite.
- R-J5: `cargo clippy` always uses `--locked -- -D warnings`.
- R-J6: Advisory scans — Rust (`rustsec/audit-check` + `cargo-deny check advisories`) and npm (`npm audit --audit-level=high --omit=dev`) — MUST run in `audit.yml` on a daily schedule plus `workflow_dispatch`, never on push or PR. A new advisory in a transitive dependency must not block an unrelated merge or hotfix; it is triaged on its own cadence. `ci.yml` keeps only the deterministic `cargo-deny check bans licenses sources`. The `ignore:` list of `rustsec/audit-check` mirrors `deny.toml` `[advisories].ignore`. Note: GitHub disables scheduled workflows in public repos after 60 days without activity; check `gh workflow list --all` after a pause.
- R-J7: Multi-step npm operations (install + build, install + test) MUST use `working-directory: frontend` on the step, not an inline `cd frontend &&` prefix. Inline `cd` defeats the cache-path contract and diverges from the canonical job templates in this spec.

---

## §4 Job Structure for `ci.yml` (Rust library, no frontend)

Four parallel jobs: `fmt`, `clippy`, `test`, `deny`. Same structure as §3 but all Node/frontend steps are absent; advisory scans go to `audit.yml` (R-J6) without the `npm` job.

```yaml
fmt:
  name: Cargo Fmt
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@<sha> # v4.x.x
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: rustfmt
    - run: cargo fmt -- --check

clippy:
  name: Clippy
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@<sha> # v4.x.x
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: clippy
    - uses: Swatinem/rust-cache@<sha> # v2.x.x
    - run: cargo clippy --locked -- -D warnings

test:
  name: Test
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@<sha> # v4.x.x
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@<sha> # v2.x.x
    - run: cargo test --locked --no-fail-fast

deny:
  name: Cargo Deny (bans/licenses/sources)
  runs-on: ubuntu-latest
  permissions:
    contents: read
  steps:
    - uses: actions/checkout@<sha> # v4.x.x
    - uses: EmbarkStudios/cargo-deny-action@v2
      with:
        command: check bans licenses sources
```

---

## §5 Job Structure for `ci.yml` (npm package)

```yaml
jobs:
  ci:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<sha>
      - uses: actions/setup-node@<sha>
        with:
          node-version: '22'
          registry-url: 'https://npm.pkg.github.com'
          scope: '@netray-info'
      - run: npm ci
      - run: npm run lint   # if available
      - run: npm test
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<sha>
      - uses: actions/setup-node@<sha>
        with:
          node-version: '22'
          registry-url: 'https://npm.pkg.github.com'
          scope: '@netray-info'
      - run: npm ci
        env:
          NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      - run: npm audit --audit-level=high --omit=dev
```

---

## §6 `release.yml` — Test Gate

The first job in every `release.yml` MUST be a fast test gate. Docker builds do not start until it passes.

```yaml
test:
  name: Test
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@<sha>
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@<sha>
    - uses: actions/setup-node@<sha> # v4.x.x
      with:
        node-version: '22'
        cache: npm
        cache-dependency-path: frontend/package-lock.json
    - name: Build frontend
      run: npm ci && npm run build
      working-directory: frontend
      env:
        NODE_AUTH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    - run: cargo test --locked --no-fail-fast
```

The frontend must be built before `cargo test` — `rust-embed` resolves `frontend/dist/` at compile time (same requirement as in CI). The Docker build will also rebuild the frontend inside the image.

**Rule:**
- R-R1: Every `release.yml` MUST have a `test` job with `cargo test --locked`. The `build` matrix MUST declare `needs: [test]`. No exceptions.

---

## §7 `release.yml` — Docker Build Matrix

Native runners for both architectures. No QEMU emulation.

```yaml
build:
  name: Build (${{ matrix.platform }})
  needs: [test]
  runs-on: ${{ matrix.runner }}
  permissions:
    contents: read
    packages: write
  strategy:
    fail-fast: false
    matrix:
      include:
        - platform: linux/amd64
          runner: ubuntu-latest
        - platform: linux/arm64
          runner: ubuntu-24.04-arm
  steps:
    - name: Prepare platform pair
      run: |
        platform=${{ matrix.platform }}
        echo "PLATFORM_PAIR=${platform//\//-}" >> $GITHUB_ENV

    - uses: actions/checkout@<sha>

    - name: Set up Docker Buildx
      uses: docker/setup-buildx-action@<sha>

    - name: Log in to GHCR
      uses: docker/login-action@<sha>
      with:
        registry: ghcr.io
        username: ${{ github.repository_owner }}
        password: ${{ secrets.GITHUB_TOKEN }}

    - name: Extract metadata
      id: meta
      uses: docker/metadata-action@<sha>
      with:
        images: ${{ env.REGISTRY_IMAGE }}

    - name: Build and push by digest
      id: build
      uses: docker/build-push-action@<sha>
      with:
        context: .
        platforms: ${{ matrix.platform }}
        labels: ${{ steps.meta.outputs.labels }}
        tags: ${{ env.REGISTRY_IMAGE }}
        outputs: type=image,push-by-digest=true,name-canonical=true,push=true
        cache-from: type=gha,scope=${{ matrix.platform }}
        cache-to: type=gha,mode=max,scope=${{ matrix.platform }}
        secrets: |
          NODE_AUTH_TOKEN=${{ secrets.GITHUB_TOKEN }}

    - name: Export digest
      run: |
        mkdir -p ${{ runner.temp }}/digests
        digest="${{ steps.build.outputs.digest }}"
        touch "${{ runner.temp }}/digests/${digest#sha256:}"

    - name: Upload digest
      uses: actions/upload-artifact@<sha>
      with:
        name: digests-${{ env.PLATFORM_PAIR }}
        path: ${{ runner.temp }}/digests/*
        if-no-files-found: error
        retention-days: 1
```

**Rules:**
- R-D1: Both `linux/amd64` and `linux/arm64` MUST be built. The production OCI VM is ARM.
- R-D2: Always use native runners (`ubuntu-latest` for amd64, `ubuntu-24.04-arm` for arm64). Never QEMU.
- R-D3: GHA Docker layer cache MUST be enabled with per-platform scope: `cache-from: type=gha,scope=${{ matrix.platform }}` and `cache-to: type=gha,mode=max,scope=${{ matrix.platform }}`.
- R-D4: The GHCR login `username` MUST be `${{ github.repository_owner }}`, never a hardcoded string.

---

## §8 `release.yml` — Manifest Merge

```yaml
merge:
  name: Merge manifests
  needs: [build]
  runs-on: ubuntu-latest
  permissions:
    contents: read
    packages: write
  steps:
    - name: Download digests
      uses: actions/download-artifact@<sha>
      with:
        path: ${{ runner.temp }}/digests
        pattern: digests-*
        merge-multiple: true

    - name: Set up Docker Buildx
      uses: docker/setup-buildx-action@<sha>

    - name: Log in to GHCR
      uses: docker/login-action@<sha>
      with:
        registry: ghcr.io
        username: ${{ github.repository_owner }}
        password: ${{ secrets.GITHUB_TOKEN }}

    - name: Extract metadata
      id: meta
      uses: docker/metadata-action@<sha>
      with:
        images: ${{ env.REGISTRY_IMAGE }}
        tags: |
          type=semver,pattern={{version}}
          type=semver,pattern={{major}}.{{minor}}
          type=raw,value=latest,enable=${{ github.ref_type == 'tag' }}
          type=sha,prefix=,format=short

    - name: Create manifest list and push
      working-directory: ${{ runner.temp }}/digests
      run: |
        docker buildx imagetools create \
          $(jq -cr '.tags | map("-t " + .) | join(" ")' <<< "$DOCKER_METADATA_OUTPUT_JSON") \
          $(printf '${{ env.REGISTRY_IMAGE }}@sha256:%s ' *)

    - name: Inspect image
      run: |
        docker buildx imagetools inspect ${{ env.REGISTRY_IMAGE }}:${{ steps.meta.outputs.version }}

    - uses: actions/checkout@<sha>

    - uses: dtolnay/rust-toolchain@stable

    - name: Cache cargo-cyclonedx binary
      id: cache-cyclonedx
      uses: actions/cache@<sha> # v4.x.x
      with:
        path: ~/.cargo/bin/cargo-cyclonedx
        key: cargo-cyclonedx-${{ runner.os }}-${{ runner.arch }}

    - name: Install cargo-cyclonedx
      if: steps.cache-cyclonedx.outputs.cache-hit != 'true'
      run: cargo install cargo-cyclonedx --locked --quiet

    - name: Generate SBOM
      run: cargo cyclonedx --format json --override-filename sbom

    - name: Upload SBOM
      uses: actions/upload-artifact@<sha>
      with:
        name: sbom
        path: sbom.cdx.json

    - name: Write release ref
      run: echo "${{ github.ref_name }}" > release-ref.txt

    - name: Upload release ref
      uses: actions/upload-artifact@<sha>
      with:
        name: release-ref
        path: release-ref.txt
        retention-days: 1
```

**Rules:**
- R-M1: Tags MUST include `{{version}}`, `{{major}}.{{minor}}`, `latest` (tag-push only), and a short SHA (`type=sha,prefix=,format=short`). The SHA tag is the rollback handle.
- R-M2: `latest` MUST be guarded with `enable=${{ github.ref_type == 'tag' }}` to prevent `workflow_dispatch` from overwriting `latest` with an untagged build.
- R-M3: The `merge` job MUST write the git ref name to `release-ref.txt` and upload it as the `release-ref` artifact. This is the only reliable way to pass the tag to `deploy.yml`, which runs in the default-branch context due to `workflow_run` semantics.
- R-M4: SBOM generation belongs in the `merge` job, not the `build` matrix. Rust dependencies do not vary by CPU architecture — a single SBOM covers both platforms and avoids per-platform artifact naming.

---

## §9 `deploy.yml` — Deploy to Production

`deploy.yml` is a separate workflow file. It fires automatically after a successful `release.yml` run and can also be triggered manually to re-deploy a specific tag without rebuilding the image.

```yaml
---
name: Deploy

on:
  workflow_run:
    workflows: [Release]   # MUST match the name: field of release.yml exactly
    types: [completed]
  workflow_dispatch:
    inputs:
      tag:
        description: 'Tag to deploy (e.g. v1.2.3)'
        required: true

jobs:
  deploy:
    name: Deploy
    runs-on: ubuntu-latest
    if: >
      github.event_name == 'workflow_dispatch' ||
      github.event.workflow_run.conclusion == 'success'
    steps:
      - name: Download release ref (workflow_run path)
        if: github.event_name == 'workflow_run'
        uses: actions/download-artifact@<sha>
        with:
          name: release-ref
          run-id: ${{ github.event.workflow_run.id }}
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: Read release ref (workflow_run path)
        if: github.event_name == 'workflow_run'
        id: read-ref
        run: echo "tag=$(cat release-ref.txt)" >> $GITHUB_OUTPUT

      - name: Trigger deploy webhook
        env:
          DEPLOY_WEBHOOK_SECRET: ${{ secrets.DEPLOY_WEBHOOK_SECRET }}
          TAG: ${{ github.event_name == 'workflow_dispatch' && inputs.tag || steps.read-ref.outputs.tag }}
        run: |
          PAYLOAD="{\"ref\":\"${TAG}\"}"
          SIG=$(printf '%s' "$PAYLOAD" | openssl dgst -sha256 -hmac "$DEPLOY_WEBHOOK_SECRET" | awk '{print $NF}')
          curl -fsS -X POST \
            -H "Content-Type: application/json" \
            -H "X-Hub-Signature-256: sha256=${SIG}" \
            -d "$PAYLOAD" \
            "https://deploy.netray.info/hooks/deploy-<service>"
```

**Rules:**
- R-W1: Webhook secret is always named `DEPLOY_WEBHOOK_SECRET`. No per-service variants.
- R-W2: HMAC signing MUST use `printf '%s'` (not `echo -n`) and `awk '{print $NF}'` (not `sed`). `printf` is POSIX-portable; `awk` is robust to OpenSSL output format variations.
- R-W3: Webhook URL pattern: `https://deploy.netray.info/hooks/deploy-<service>`. Service name mapping:

  | Repo | Hook path |
  |------|-----------|
  | `ifconfig-rs` | `/hooks/deploy-ifconfig-rs` |
  | `mhost-prism` | `/hooks/deploy-mhost-prism` |
  | `tlsight` | `/hooks/deploy-tlsight` |
  | `lens` | `/hooks/deploy-lens` |
- R-W4: `deploy.yml` MUST guard with `if: github.event_name == 'workflow_dispatch' || github.event.workflow_run.conclusion == 'success'`. Without this guard, a failed release run still triggers deploy.
- R-W5: The `workflow_run` trigger executes in the default-branch context — `github.ref_name` will be `main`, not the tag. The tag MUST be sourced from the `release-ref` artifact (automatic path) or the `tag` input (manual path). Never use `github.ref_name` directly in `deploy.yml`.
- R-W6: The `workflow_dispatch` path is for re-deploys — the image is already in GHCR. It does not rebuild anything, only fires the webhook with the supplied tag.

---

## §10 Actions Standards

### Pinning
- R-A1: Every third-party action MUST be pinned to a full commit SHA with the version as a comment: `uses: actions/checkout@<sha> # v4.x.x`.
- R-A2: `dtolnay/rust-toolchain@stable` is exempt from SHA pinning — it is a floating ref by design and has no SHA-pinnable releases.
- R-A3: `rustsec/audit-check@v2` and `EmbarkStudios/cargo-deny-action@v2` use major-version floating tags. SHA pinning is technically possible but the maintainers do not document stable pinnable SHAs in their releases. Keep at major version tags.

### Versions
- R-A4: Node.js version is `'22'` in all workflows. Update suite-wide when the LTS changes.
- R-A5: Rust toolchain is always `@stable`. Never pin to a specific Rust version.

### Caching
- R-A6: Use `Swatinem/rust-cache` for Rust compilation caching in all CI jobs that invoke `cargo`. Do not hand-roll `actions/cache` paths for Rust.
- R-A7: Use `actions/setup-node` built-in npm cache (`cache: npm`, `cache-dependency-path: frontend/package-lock.json`) for Node.

### Configuration files
- R-A8: Every Rust repo MUST ship a `deny.toml` at the repo root. `EmbarkStudios/cargo-deny-action` without a config file either errors or checks nothing useful.

---

## §11 Permissions

- R-P1: Omit `permissions:` at the workflow level. Set it per-job at the minimum scope needed.
- R-P2: Default implicit permission is `contents: read`. Only escalate where required.
- R-P3: Docker build/merge jobs need `contents: read, packages: write`.
- R-P4: The `rust` job in `audit.yml` needs `contents: read, checks: write` (for `rustsec/audit-check` annotations).
- R-P5: Never use a personal access token (`LP_GHCR_TOKEN` or equivalent). `secrets.GITHUB_TOKEN` is sufficient for GHCR reads and writes within the same org.

---

## §12 Secrets

| Name | Used for | Scope |
|------|----------|-------|
| `GITHUB_TOKEN` | GHCR push, GitHub Packages npm auth, rustsec/audit-check | Auto-provided |
| `DEPLOY_WEBHOOK_SECRET` | Signed webhook to deploy.netray.info | Org or repo secret |

No other secrets are required. If a workflow references anything else, treat it as a defect.

---

## §13 Env Block

Set `CARGO_TERM_COLOR: always` at the workflow level for all Rust workflows so cargo output is readable in GHA logs:

```yaml
env:
  CARGO_TERM_COLOR: always
```

Set `REGISTRY_IMAGE` at the workflow level in `release.yml`:

```yaml
env:
  REGISTRY_IMAGE: ghcr.io/${{ github.repository }}
```

Never hardcode the image name with the org path. `github.repository` expands to `org/repo` which is always correct.

---

## §14 Responsibility Boundaries

| Step | `ci.yml` | `audit.yml` | `release.yml` | `deploy.yml` |
|------|----------|-------------|---------------|--------------|
| Lint, test, cargo-deny (bans/licenses/sources) | yes | no | no | no |
| Advisory scans (RUSTSEC, npm audit) | no | yes | no | no |
| Docker build + push | no | no | yes | no |
| SBOM generation | no | no | yes | no |
| Manifest merge + tagging | no | no | yes | no |
| Deploy webhook | no | no | no | yes |
| Produces artifacts | no | no | digests, SBOMs, release-ref | no |
| Consumes artifacts | no | no | no | release-ref (from release.yml) |

`ci.yml` is a gate — it produces no artifacts and makes no external changes.
`release.yml` builds and publishes the image — it does not deploy.
`deploy.yml` deploys — it does not build anything.
