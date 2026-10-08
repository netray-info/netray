# Spec: no data files in the published image

Status: In progress
Created: 2026-10-08

`specs/features/monorepo-p2/spec.md` (decision D4) baked ifconfig-rs's data files into the one public image, copied from the private `ifconfig-rs-data` image. The GeoLite2 licence does not allow redistributing the MaxMind `.mmdb` files, and the other lists (Spamhaus DROP, abuse.ch, X4BNet, ipverse, uap-core, cloud provider ranges) carry their own terms. This spec supersedes D4: the published image contains no data files; the deployment mounts them.

## Decisions

| # | Decision |
|---|---|
| D1 | The image built from the root `Dockerfile` contains no file from `crates/ifconfig-rs/data/` sources and nothing from `ifconfig-rs-data`. The deployment fetches the data with `crates/ifconfig-rs/data/fetch.sh` on its host and mounts the directory read-only at `/netray/data`. |
| D2 | CI may still read the private `ifconfig-rs-data` image to run the ifconfig-rs integration tests: nothing it reads is published. |
| D3 | `release.yml` proves the absence: before pushing, it fails when the built image contains a `.mmdb` file or a non-empty `/netray/data`. |

## Requirements

1. The root `Dockerfile` has no stage from `ghcr.io/netray-info/ifconfig-rs-data` and copies no data into `/netray/data`.
2. `release.yml` reads no data image and has a step, before the push, that fails when the built image contains any `*.mmdb` file or any file under `/netray/data`. The `ip` smoke test runs without GeoIP paths.
3. `specs/rules/workflow-rules.md` states that no data file is ever baked into a published image and that only `ci.yml` reads the private data image.
4. `just image` needs no GHCR login; README and CLAUDE.md no longer say it does.

## Phase 1 — Image without data

**Depends on:** none
**Requirements:** 1, 2, 3, 4

### Test Scenarios

- GIVEN the root `Dockerfile` WHEN read THEN no line references `ifconfig-rs-data` and none copies into `/netray/data`.
- GIVEN `release.yml` WHEN read THEN it does not mention `ifconfig-rs-data`, sets no `IFCONFIG_GEOIP_` variable, and has a step before the push that searches the image for `*.mmdb` and for files under `/netray/data` and fails on a hit.
- GIVEN `specs/rules/workflow-rules.md` WHEN read THEN it forbids baking data into a published image and names `ci.yml` as the only reader of `ifconfig-rs-data`.
- GIVEN README.md and CLAUDE.md WHEN read THEN neither says `just image` needs a GHCR login.

## Open decisions

None.
