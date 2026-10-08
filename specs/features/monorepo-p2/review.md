## ce84058..db53d86

### Reader

COUNTS blockers=0 majors=2 minors=1
LENSES Engineering, Security, Testing

MAJOR | .github/workflows/ci.yml:45 | The CI GeoIP step overwrites the tracked `crates/ifconfig-rs/data/asn_patterns.toml` with the copy baked into `ifconfig-rs-data:latest`, so CI tests the image's file instead of the committed one and can disagree with the local gate on the same commit. | A commit that changes `asn_patterns.toml` together with its test is green locally and red in CI (or the reverse) until the data image is rebuilt; `from_file_asn48152_classifies_as_hosting` reads `data/asn_patterns.toml`.
MAJOR | tests/acceptance/static-site/links.spec.ts:8 | A test on the default branch is renamed and pointed at a different page without an `ADLC-Test-Change` trailer, which AGENTS.md requires. | d51eb49 changes the title and moves the request from `/` to `/tools`; the range carries trailers only for `tests/repo/test_meta_schema.sh` and `tests/repo/test_workflows.sh`.
MINOR | tests/repo/test_workflow_rules.sh:17 | The C12 test has no check for a per-service image in the rules, so that part of the criterion cannot fail. | Adding `ghcr.io/netray-info/lens:0.12.0` to the rules still passes.

```quote .github/workflows/ci.yml:45
          docker cp ifconfig-data:/data/. crates/ifconfig-rs/data/
```

```quote crates/ifconfig-rs/data/Dockerfile:5
COPY as_metadata.jsonl asn_patterns.toml /data/
```

```quote tests/acceptance/static-site/links.spec.ts:8
test('all SuiteNav links on /tools resolve to 200', async ({ request }) => {
```

```quote tests/repo/test_workflow_rules.sh:17
for pat in 'release-ref' 'deploy.yml' 'DEPLOY_WEBHOOK_SECRET' 'linux/amd64'; do
```

### Refuted

- MAJOR links.spec.ts:8 missing `ADLC-Test-Change` — REFUTED, confidence 8: `tests/acceptance/**` is `not-tests` in `adlc.toml` and the file was never added under a baseline trailer, so it is not a protected test.

### Calibration

| Finding | Refuter | Conf. | Result | Check |
|---|---|---|---|---|
| MAJOR ci.yml:45 data image overwrites tracked `asn_patterns.toml` | CONFIRMED | 7 | held | `git ls-files crates/ifconfig-rs/data` lists `asn_patterns.toml`; `data/Dockerfile` COPYs it; `cargo test -p ifconfig-rs --lib` (gate) runs `from_file_asn48152_classifies_as_hosting` |
| MAJOR links.spec.ts:8 trailer | REFUTED | 8 | not held | `adlc.toml` `not-tests = ["tests/acceptance/**"]` |

### Summary

- Before refutation: blockers 0, majors 2, minors 1. After: blockers 0, majors 1, minors 1. verified 2, held 1. No principles declared.

## db53d86..98d3e39

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing

MINOR | tests/repo/test_workflows.sh:64 | The no-clobber guard is textual: a `docker cp` into `./crates/ifconfig-rs/data/` plus an unrelated `cp -n` elsewhere would pass it. | Listed; the CI step itself is sound (coreutils 9.4 on ubuntu-24.04-arm exits 0 when `cp -n` skips; the data image holds only flat files).

### Summary

- 0/0/1. verified 0, held 0. Resolves the held major of ce84058..db53d86 (ci.yml:45).
