# The verbs of the netray monorepo (specs/features/monorepo-p1/spec.md, requirement 8).

# The ADLC gate: offline.
adlc-verify: test-repo

# Repository structure checks: one test per file, each exits non-zero on failure.
test-repo:
    #!/usr/bin/env bash
    set -uo pipefail
    fail=0
    for t in tests/repo/test_*.sh; do
        [ -e "$t" ] || continue
        if bash "$t"; then echo "ok   $t"; else echo "FAIL $t"; fail=1; fi
    done
    exit $fail

# What makes a fresh checkout able to run the contract, with nothing from the machine but just:
# the tools it calls, pinned (linters, test runners, the mutation tool), then the dependencies and
# the builds. CI and every new worktree run it. adlc's own tools come from the adlc checkout.
adlc-setup:
    @for t in cargo node npm bash; do command -v "$t" >/dev/null || { echo "$t is not on PATH" >&2; exit 1; }; done
