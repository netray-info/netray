# Review: sdd-into-monorepo

## 4bd1977..51c552e

### Reader

COUNTS blockers=1 majors=0 minors=1
LENSES Engineering, Testing
BLOCKER | tests/repo/test_sdd_in_repo.sh:13 | The R4 check for invalid JSON accepts tokens that are not valid JSON, because Python's `json.load` takes `NaN`, `Infinity` and `-Infinity` by default. RFC 8259 rejects them, and so do serde_json, `JSON.parse` and Ruby's `JSON.parse`. | In a scratch tree with this repo's `CLAUDE.md`, a non-empty `specs/sdd/v2.md`, and `specs/sdd/v2-tokens.json` = `{"a":NaN}`, the script printed "ok: …" and exited 0. R4 says it must exit non-zero when the tokens file is "not valid JSON". The self-test only tries a trailing comma.
MINOR | specs/features/sdd-into-monorepo/report.md:9 | Criteria C1 (line 9) and C2 (line 10) are marked `green` with `tests/repo/test_sdd_in_repo.sh` as their test, but that test cannot fail when either file's content differs from the source. | In a scratch tree with `specs/sdd/v2.md` = `# x` and `specs/sdd/v2-tokens.json` = `{}` the test exited 0. The report's prose (line 14) says these two criteria were checked by hand.

```quote tests/repo/test_sdd_in_repo.sh:13
    elif ! python3 -I -c 'import json, sys; json.load(open(sys.argv[1]))' "$1/specs/sdd/v2-tokens.json" 2>/dev/null; then
```

```quote specs/features/sdd-into-monorepo/report.md:9
| C1 | `specs/sdd/v2.md` is e28b9fc's file except the §8.1 row and the two citations | green | `tests/repo/test_sdd_in_repo.sh` |
```

Checked and sound: the SDD differs from e28b9fc only at lines 50, 632-633 and 863 (3 insertions, 4 deletions); the tokens are byte-identical and the `v2-tokens.json:96`, `:163` citations still hold; no domain other than `example.com` or `netray.info` and no `specs/prd` or `specs/reviews` path remains; `CLAUDE.md` no longer says "planning SDD"; the test passes on the tree, fails on a missing file, and the `tests/repo/test_*.sh` glob collects it.

### Refutation

| Finding | Answer | Confidence |
|---|---|---|
| BLOCKER test_sdd_in_repo.sh:13 NaN accepted | CONFIRMED | 9 |

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| BLOCKER test_sdd_in_repo.sh:13 | CONFIRMED | 9 | held | `python3 -I -c 'import json, sys; json.load(open(sys.argv[1]))' nan.json` on `{"a":NaN}` exits 0 |

### Summary

Before and after refutation 1/0/1. Verified 1, held 1. Operator (2026-10-14): fix on the branch.

## 51c552e..3f56835

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing
MINOR | tests/repo/test_sdd_in_repo.sh:18 | Whether the "not valid JSON" check passes depends on the locale: the file is opened in the locale's encoding, so under a non-UTF-8 locale bytes that are not UTF-8 (RFC 8259 §8.1) pass. | `v2-tokens.json` = `{"a":"\xff"}`: refused under `en_US.UTF-8`, accepted under `en_US.ISO8859-1`; `-I` ignores `PYTHONUTF8`. Present since 51c552e; macOS and CI locales are UTF-8. Opening in binary mode would refuse it in every locale.

```quote tests/repo/test_sdd_in_repo.sh:18
json.load(open(sys.argv[1]), parse_constant=refuse)' "$1/specs/sdd/v2-tokens.json" 2>/dev/null; then
```

Checked and sound: `parse_constant=refuse` refuses `NaN`, `Infinity`, `-Infinity`; a copy without it fails the new self-test; the real tokens still pass; the `ADLC-Test-Change` trailer is on 3f56835; the report's hand checks for C1 and C2 hold.

### Summary

Before and after refutation 0/0/1 (nothing for the refuter). Verified 0, held 0. The minor is accepted: every machine that runs the gate has a UTF-8 locale.

Record note: the review row for 51c552e..3f56835 was written with 14 insertions, 3 deletions, 0 tokens and 0 seconds; the range has 19 and 5, and the reader used 53832 tokens over 159 seconds.
