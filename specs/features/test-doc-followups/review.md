
## fa4ae335..26543bc3

### Reader

```
COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Security, Testing
```

I traced the new root argument of `tests/repo/test_toolchain_pin.sh` (no argument, missing directory, empty string, which is a no-op `cd` under the local bash 3.2) and the rewritten selftest. I also traced the C9 and happy-dom self-tests in `tests/repo/test_advisories.sh` with the new fixture, and the `crates/common/CLAUDE.md` line against what the check verifies. Mutation runs in scratch copies confirmed that each self-test fails when the code it names is removed:
- removing the awk `if (in_b) exit` fails `env_other_stage`;
- removing the channel refusal fails `channel_stable`;
- ignoring the root argument fails `pinned`;
- deleting or not exiting from the root guard fails the empty-root case;
- dropping the package name from the missing-package line fails C9;
- disabling `older()` or lowering the happy-dom floor fails the happy-dom case.

The real tree still passes `test_toolchain_pin.sh`, its selftest, `test_advisories.sh`, `test_crate_docs.sh` and `test_workflow_rules.sh`. I found no wrong result.
### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=7 partial=0 missed=0 contradicted=0

I traced all seven requirements of Phase 1 at 26543bc3. R1: an empty root and a missing directory both exit 1 with `FAIL: root '<arg>' is not a directory`, and with no argument the check passes on the real tree (exit 0). R2: `run_check` runs `bash "$check" "$1"`, and the selftest's `find` shows no copy of the script. R3: with the awk `if (in_b) exit` removed, `in_b` stays 1 through the alpine stage, the appended `ENV RUSTUP_TOOLCHAIN=stable` sets ok=1, the check exits 0, and `env_other_stage` fails. R4: each case looks for its own `channel '<raw>' is not an exact X.Y.Z version`. R5: C9 now looks for `has no rustix package`. R6: the new fixture makes `check_npm_floors` print `happy-dom 20.14.5 is older than 21.0.4`, and the diff stays inside the self-test block. R7: the bullet that names `1.99.1` now names `tests/repo/test_toolchain_pin.sh`. The Phase 1 header and its scenarios include boundary THENs (the exit code and output of `bash tests/repo/test_toolchain_pin.sh <root>`), and the range adds no behaviour beyond the requirements.

### Findings


### Roll call

P03 | absence — sg/rg for Status/Verdict enums and codenames in changed files — range touches only tests/repo/*.sh, crates/common/CLAUDE.md, specs/features, advisory fixture; no enums, templates or serializers
P10 | absence — follow error/timeout branches of changed check or scoring paths — no check or scoring code changed (git diff --stat fa4ae335..26543bc3)
P12 | absence — sg for reqwest builders and engine entry calls — none added or changed in range
P13 | absence — read serve limiter setup — no serve or route change in range
P18 | absence — sg for reqwest::Client outside fetch.rs; test_config_strict.sh — no Rust code or config structs changed; test_config_strict.sh untouched
P26 | absence — rg new check IDs for one emitter — no new check ID in range
P35 | absence — new/changed specs/rules rule arrives with enforcing check — no specs/rules file changed in range
P36 | absence — generated file changes only with source/generator — no generated file in diff --stat
P40 | absence — diff --stat of module add/remove; sg engine for module crate paths — no module added or removed, crates/engine untouched

### Summary

Reader: 0/0/0 before and after refutation. Spec reader: 0/0/0, REQUIREMENTS met=7 partial=0 missed=0 contradicted=0. No refutation pass (no BLOCKER or MAJOR). verified 0, held 0. Roll call: answered 9, convergence 0, divergence 0, absence 9.
