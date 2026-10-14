# Report: sdd-into-monorepo

## Phase 1 — SDD and tokens in the repository

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | `specs/sdd/v2.md` is e28b9fc's file except the §8.1 row and the two citations | green | by hand: `git diff --no-index` against e28b9fc (below) |
| C2 | `specs/sdd/v2-tokens.json` is the planning file byte for byte | green | by hand: `cmp` (below) |
| C3 | `CLAUDE.md` names `specs/sdd/v2.md` as its own SDD, no "planning SDD" | green | `tests/repo/test_sdd_in_repo.sh` |
| C4 | the test fails on a missing SDD, missing or invalid tokens, or a stale pointer | green | `tests/repo/test_sdd_in_repo.sh` |

The test checks presence, JSON validity and the pointer; the content criteria C1 and C2 were checked by hand: `git diff --no-index` against `git show e28b9fc:specs/sdd/v2.md` shows exactly the three edits (3 insertions, 4 deletions), and `cmp` finds the tokens identical. No domain other than `example.com` and `netray.info` remains in either file. `CLAUDE.md` is in the phase commit, not a separate docs commit, because requirement 3 owns it and the test reads it.

After the review (4bd1977..51c552e) the JSON check refuses `NaN` and `Infinity` too (`parse_constant`), and the self-test carries that case.

### Behavioural verification

skipped: documents only, no entry point.
