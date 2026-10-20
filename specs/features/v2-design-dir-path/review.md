# Review: v2-design-dir-path

## 0b17f07e..13301f50

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Security, Testing

No wrong result: the default resolves via `--git-common-dir` to
`/Users/lukas/Documents/src/netray/../netray.info/specs/design/canvas` (manifest, export, png
present; manifest byte-identical to the netray-ops copy, no relative paths back into netray-ops);
an explicit `NETRAY_DESIGN_DIR` still wins; no file outside `specs/features` names `netray-ops` or
`specs/netray/design`; the new test fails all seven assertions against the base files and catches a
broken `${NETRAY_DESIGN_DIR:-...}` expansion; neighbouring repo checks exit 0; trailers present.

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=3 partial=0 missed=0 contradicted=0

Process note, not a finding: the phase text said `tests/repo/test_design_rules.sh` gains the
checks; they went into the new `tests/repo/test_design_dir.sh` (spec text corrected at finish).

### Refuted

None (no BLOCKER or MAJOR to refute; no refuter spawned).

### Calibration

None.

### Roll call

| principle | verdict | evidence |
|---|---|---|
| P03 | absence | no enum, template, site or serializer change |
| P10 | absence | no check or scoring path changed |
| P12 | absence | no reqwest builder or engine module call changed |
| P13 | absence | no serve command, limiter or route changed |
| P18 | absence | no reqwest::Client, no config struct changed |
| P26 | absence | no check ID added |
| P35 | convergence | the design-rules.md change arrives with tests/repo/test_design_dir.sh |
| P36 | absence | no generated file changed |
| P40 | absence | no module added or removed |

### Summary

Before and after refutation: reader 0/0/0, spec reader 0/0/0 (REQUIREMENTS met=3). verified 0,
held 0. Roll call: answered 9, convergence 1, divergence 0, absence 8.
