# Bug: ci-disk-space

Written by `adlc feature start --bug`. The first four sections are yours; diagnose fills the last three.

## Symptom

CI run 39131412716 on `main` at 309c6611 (merge of PR #27, v2-api) fails in the `check` job, step
`just check`, recipe `test-rust`: linking the lens test binaries (`api_v1_runs_validate` and three
more) stops with `/usr/bin/ld: final link failed: No space left on device`, exit 101. Because the
`test-rust` recipe never finishes, the GeoIP-dependent ifconfig-rs tests it runs with the data image
never complete in CI.

## Expected

The `check` job of `ci.yml` concludes `success` on `ubuntu-24.04-arm`, with `just check` running the
full Rust suite including the GeoIP data path of `test-rust`.

## Reproduction

Push any commit of `main` at or after 309c6611; the `CI` workflow's `check` job fails as above.

## Environment

GitHub-hosted `ubuntu-24.04-arm`, Rust 1.99.1 (`rust-toolchain.toml`), Swatinem/rust-cache v2.9.1
restoring a ~1212 MB cache, `CARGO_INCREMENTAL=0` (set by dtolnay/rust-toolchain), dev profile with
full debuginfo, GeoIP data copied from `ghcr.io/netray-info/ifconfig-rs-data:latest`.

## Loop

The symptom shows only on the hosted runner, so the loop reads the CI result of the commit checked
out, run from the repository root:

```sh
gh run list --workflow ci.yml --commit "$(git rev-parse HEAD)" --json conclusion --jq '.[0].conclusion'
gh run view "$(gh run list --workflow ci.yml --commit "$(git rev-parse HEAD)" --json databaseId --jq '.[0].databaseId')" --log-failed | grep -c 'No space left on device'
```

Run twice at HEAD 309c6611, both red on the symptom:

```
failure
4
failure
4
```

## Hypotheses

1. **Incremental compilation artefacts fill the disk** (`ci.yml` sets no `CARGO_INCREMENTAL`).
   Refuted if incremental is already off. **Refuted**: the run log's toolchain step exports
   `CARGO_INCREMENTAL: 0` (`gh run view 39131412716 --log | grep CARGO_INCREMENTAL`), and every later
   step carries it.
2. **The restored rust-cache alone exhausts the disk.** Refuted if a green run restored the same
   cache size. **Refuted**: green run 39075065223 (df5a5a7e) restored `Cache Size: ~1212 MB`, the same
   as the failing run (`grep 'Cache Size'` on both logs).
3. **`test-rust`'s GeoIP path builds a second full set of test binaries** (`cargo test --workspace`
   after `adlc-verify`'s `cargo test --workspace --exclude netray-ip` resolves features differently).
   Refuted if lens compiles once per run. **Held, as a standing precondition**: lens compiles at
   13:31:49 and again at 13:44:46 in the failing run, and twice in the green run as well
   (22:31:54, 22:39:43; `grep 'Compiling.*lens v'`), so two target trees of test binaries were
   already on disk before PR #27.
4. **PR #27 grew the test binaries past the runner's free disk.** Refuted if the failing run builds
   nothing the green run did not. **Held**: the failing run compiles `jsonschema` and `criterion`
   (3 matches, 0 in the green run), and df5a5a7e..309c6611 adds 58 test files (+8180 lines) under
   `crates/lens/tests` and `crates/web/tests`; each lens test binary links the whole workspace.
5. **Debuginfo is a large share of the target directory.** Refuted if dropping it saves nothing.
   **Held**: `CARGO_TARGET_DIR=<tmp> cargo test -p lens --no-run` with `CARGO_INCREMENTAL=0` takes
   6782 MB; with `CARGO_PROFILE_DEV_DEBUG=line-tables-only` 5642 MB (macOS, where debuginfo stays in
   object files; on Linux it is linked into each test binary, so the share there is larger).

## Root cause

The `check` job builds two full target trees of debug test binaries on one runner disk: first under
`just adlc-verify`'s offline suite, then again under `test-rust`'s GeoIP branch, which unifies
features across the whole workspace:

```quote justfile:60
    if [ -f crates/ip/data/GeoLite2-City.mmdb ]; then
```

```quote justfile:61
        cargo test --workspace
```

CI takes that branch because it copies the data image into `crates/ip/data`:

```quote .github/workflows/ci.yml:56
          cp -n "$RUNNER_TEMP"/ifconfig-data/* crates/ip/data/
```

Every test binary is linked with full debuginfo (the dev profile's default), on a runner whose disk
also holds the preinstalled toolchains and the restored ~1.2 GB cache. PR #27 added test binaries
and dependencies (`jsonschema`, `criterion`) until the second tree no longer fit: the job sets
nothing that bounds the target directory's size or frees runner disk before the build:

```quote .github/workflows/ci.yml:58
      - run: just check
```
