# Review: ci-disk-space

## 309c6611..f802ac60

### Reader

COUNTS blockers=0 majors=0 minors=2
LENSES Engineering, Security, Testing
MINOR | tests/repo/test_workflows.sh:70 | The free-disk check passes a step that names the three paths but deletes nothing and runs `df -h /` only once. So spec scenario 2 ("without a step removing the preinstalled toolchains … fails") and report row C3 ("removes dotnet, ghc, android and prints df -h /", marked green) are not actually enforced. | With the step body set to `df -h /` then `du -sh /usr/share/dotnet /opt/ghc /usr/local/lib/android \|\| true` (no `rm`, one `df`), `bash tests/repo/test_workflows.sh` prints `ok`. I ran it in a scratch git copy of f802ac60's ci.yml, release.yml and the test.
MINOR | specs/features/ci-disk-space/bug.md:87 | The fix commit adds 8 lines above the lines that bug.md's root-cause anchors point to, so two of them are wrong at the range head. `.github/workflows/ci.yml:48` is now `username: ${{ github.actor }}`, and `:50` is the GeoIP step's name, not `- run: just check`. | At f802ac60, `adlc quote-check specs/features/ci-disk-space/bug.md --dir <worktree>` exits 1 with "2 of 4 anchors do not hold: moved .github/workflows/ci.yml:48 → 56; moved .github/workflows/ci.yml:50 → 58".

```quote tests/repo/test_workflows.sh:70
  i_free = csteps.index { |s| %w[/usr/share/dotnet /opt/ghc /usr/local/lib/android].all? { |p| s["run"].to_s.include?(p) } && s["run"].to_s.include?("df -h /") }
```

### Spec reader

COUNTS blockers=0 majors=1 minors=0
REQUIREMENTS met=3 partial=1 missed=0 contradicted=0
MAJOR | .github/workflows/ci.yml:26 | The range adds the levers Requirement 1 depends on, but the requirement's observable outcome has not happened yet: CI run 39143803077 for the head f802ac60 is still `in_progress` (it is at the "GeoIP data" step and `just check` is still pending), and a merge commit on `main` cannot exist inside this range. | Req 1: right now `gh run list --workflow ci.yml --commit f802ac60… --json conclusion --jq '.[0].conclusion'` prints `""`, not `success`. report.md marks C1 as `test-unwritable` and gives no run evidence.

```quote specs/features/ci-disk-space/spec.md:29
1. The loop command `gh run list --workflow ci.yml --commit "$(git rev-parse HEAD)" --json conclusion --jq '.[0].conclusion'`
```

```quote .github/workflows/ci.yml:26
      - name: Free runner disk
```

Requirements met: 2, 3, 4 (justfile untouched; the part of Req 4 needing the job log waits on the same run).

### Refuted

- MAJOR (spec reader) .github/workflows/ci.yml:26, Req 1 not observed: REFUTED, confidence 8; run 39143803077 for f802ac60 completed `success`, and the merge-commit half is checked after merge.

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| spec reader MAJOR ci.yml:26 (Req 1) | REFUTED | 8 | not held | `gh run list --workflow ci.yml --commit f802ac60… --json conclusion --jq '.[0].conclusion'` → `success`; `gh run view 39143803077 --log \| grep -c 'No space left'` → 0 |

### Roll call

| principle | answer |
|---|---|
| P03, P10, P12, P13, P18, P26, P36, P40 | absence: the range touches only ci.yml, test_workflows.sh and the feature's specs |
| P35 | convergence: the new CI shape is held by tests/repo/test_workflows.sh:65-72, committed before the fix with ADLC-Test-Change |

### Summary

Before refutation: reader 0/0/2, spec reader 0/1/0 (REQUIREMENTS met=3 partial=1 missed=0 contradicted=0). After: reader 0/0/2, spec reader 0/0/0 (Req 1 met on run 39143803077). verified 0, held 0. Roll call: answered 9, convergence 1, divergence 0, absence 8. Both MINORs are fixed on this branch before finish.
