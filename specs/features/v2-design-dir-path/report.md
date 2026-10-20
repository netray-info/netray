# Report: v2-design-dir-path

## Phase 1 — Default design directory in netray.info

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 design-check defaults NETRAY_DESIGN_DIR to the netray.info canvas, explicit value wins | green | tests/repo/test_design_dir.sh |
| C2 | R2 design-rules.md names the netray.info checkout and the default | green | tests/repo/test_design_dir.sh |
| C3 | R3 neither file names netray-ops or specs/netray/design | green | tests/repo/test_design_dir.sh |
| C4 | Scenario: recipe default is the netray.info path behind the expansion | green | tests/repo/test_design_dir.sh |
| C5 | Scenario: rules location text names the netray.info path and checkout | green | tests/repo/test_design_dir.sh |
| C6 | Scenario: no match for the old location in either file | green | tests/repo/test_design_dir.sh |
| C7 | Scenario: just design-check without arguments reads the netray.info manifest | green | behavioural verification |

RED: `bash tests/repo/test_design_dir.sh` failed all seven assertions before the change; green after.
Tests and code written by the orchestrator directly (four changed lines, one test file); no
test-writer, planner or coder runs.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 justfile + design-rules.md | 0 | orchestrator | 0 | 0 |

### Behavioural verification

From the worktree, without arguments and without `NETRAY_DESIGN_DIR` set (main checkout
`/Users/lukas/Documents/src/netray`, canvas at `../netray.info/specs/design/canvas`):

```
$ just design-check
seeded 7 fixture(s) into .../history.db
0 open deviation(s), 0 stale accepted entries
report: .../target/design/report.md
exit=0
```

The recipe's default expands to `/Users/lukas/Documents/src/netray/../netray.info/specs/design/canvas`
(`manifest.json` present there); the netray-ops copy still exists but is no longer read.

### Review (phase reader)

| class | finding | disposition |
|---|---|---|
| AMENDMENT | R3 check searched only the recipe body, so the recipe's header comment could name netray-ops unseen | repaired in phase: the R3 loop now greps the whole justfile |
