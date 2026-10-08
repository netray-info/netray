# Report: monorepo-no-data-in-image

## Phase 1 — Image without data

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Req 1: Dockerfile has no `ifconfig-rs-data` stage and copies nothing into `/netray/data` | green | tests/repo/test_image_data.sh |
| C2 | Req 2: release.yml reads no data image, smoke `ip` without GeoIP paths, a pre-push step fails on `*.mmdb` or files under `/netray/data` | green | tests/repo/test_image_data.sh |
| C3 | Req 3: workflow rules forbid baked data and name `ci.yml` as the only reader of `ifconfig-rs-data` | green | tests/repo/test_image_data.sh |
| C4 | Req 4: README/CLAUDE.md do not say `just image` needs a GHCR login | green | tests/repo/test_image_data.sh |
| C5–C8 | the four scenarios (same checks) | green | tests/repo/test_image_data.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| one group (Dockerfile, release.yml, rules, docs) | — | orchestrator, without writer/planner/coder agents: a revert of monorepo-p2 D4 across five files, test inverted first | — | — |

### Review

First pass: 1 BLOCKER, 1 AMENDMENT, 2 NIT.
- BLOCKER: `[ -d /netray/data ] && find` returned 1 on a clean image and aborted the step under `bash -e`, so every release would have stopped. Repaired test-first with `if … then … fi`.
- AMENDMENT: the search ran as user `netray` and could not see root-only directories. Repaired: `--user 0`.
- Orchestrator follow-up: a `find` error was masked by the trailing `if`. Repaired: `sh -ec`.
- NIT (listed): the Dockerfile check catches only the absolute `/netray/data` destination; the release step catches the rest.
- NIT (repaired): README now says the data paths come from the production config.

### Behavioural verification

The step's shell logic, simulated under `bash -e` with `sh -ec` (scratch dirs standing in for the image):
```
clean:       PASS step                      -> exit 0
dirty:       FAIL step: found …/x.mmdb      -> exit 1
unreadable:  find: …/missing: No such file  -> exit 1
data-dir:    FAIL step: found …/regexes.yaml -> exit 1
```
The real run is the `v0.22.0-rc.1` release (operator-triggered).

Second pass (over the fixes): 0 BLOCKER.
- DEFERRED → repaired: the runtime check misses a file a later layer deletes, or one that sits in a VOLUME, because the layer still ships it. `release.yml` now also lists every layer from `docker save` and fails on any `.mmdb` or `netray/data/` path, and when no layer is found; test first.
- AMENDMENT → repaired: no test required `sh -e`; the test now does.
- NIT (repaired): README says `asn_patterns.toml` comes from the repository.
- NIT (listed): the clean-image guard matches one spelling.
- NIT (listed): a symlinked `/netray/data` is not followed by the runtime `find`; the layer scan does not depend on it.

Layer scan, simulated with hand-built `docker save` layouts:
```
clean     scanned 2 layers: no data files                                   exit 0
whiteout  ::error:: … ./tmp/GeoLite2-City.mmdb ./tmp/.wh.GeoLite2-City.mmdb  exit 1
datadir   ::error:: … ./netray/data/regexes.yaml                            exit 1
empty     ::error::no image layer found to scan                             exit 1
```
