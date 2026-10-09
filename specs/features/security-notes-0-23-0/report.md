# Report: security notes 0.23.0

## Phase 1 — Security section

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | `## [0.23.0]` gains `### Security` after `### Fixed`, one entry each for R5.8 and SC17 | test-unwritable | — |
| C2 | The R5.8 entry names the three query paths and the glue resolution, says a refused address is never queried, and that every earlier release is affected | test-unwritable | — |
| C3 | The SC17 entry advises `[dns] allow_system_resolvers = false` and says the default stays `true` | test-unwritable | — |
| C4 | No other section of `CHANGELOG.md` changes | test-unwritable | — |
| C5 | `## [0.23.0]` holds Changed, Fixed, Security in that order | test-unwritable | — |
| C6 | The diff only adds lines inside `## [0.23.0]` | test-unwritable | — |

Prose in a released changelog section; a standing test would assert on one release's wording. Checked once, below.

### Behavioural verification

```
$ sed -n '/^## \[0.23.0\]/,/^## \[0.22.2\]/p' CHANGELOG.md | grep -n '^## \|^### '
1:## [0.23.0] - 2026-10-09
5:### Changed
23:### Fixed
29:### Security
39:## [0.22.2] - 2026-10-09
$ git diff --stat main -- CHANGELOG.md
 CHANGELOG.md | 10 ++++++++++
$ git diff -U0 main -- CHANGELOG.md | grep -c '^-[^-]'
0
```
