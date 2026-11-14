## 31435bc5..077eadbb

### Reader

COUNTS blockers=0 majors=0 minors=2
LENSES Engineering, Security, Testing

MINOR | tests/repo/test_toolchain_pin_selftest.sh:78 | The env_other_stage selftest cannot fail when the stage-boundary code it names is deleted. Its builder body holds a `RUN cargo build` line, so the check stops at that line before it reaches the next stage. Removing `if (in_b) exit;` (tests/repo/test_toolchain_pin.sh:46) leaves the selftest green, yet it changes the check's verdict. | I removed `if (in_b) exit;` from a copy of test_toolchain_pin.sh, and `bash tests/repo/test_toolchain_pin_selftest.sh` still printed `PASS: toolchain_pin check behaves`. Then I ran both checks on the Dockerfile `FROM clux/muslrust:1.99.1-stable-2026-11-03 AS builder` / `WORKDIR /build` / `COPY . .` / `FROM builder AS compile` / `ENV RUSTUP_TOOLCHAIN=stable` / `RUN cargo build --release -p netray`. The original check exits 1 ("builder stage has no ENV RUSTUP_TOOLCHAIN=stable"); the mutated check exits 0 and prints PASS.
MINOR | tests/repo/test_toolchain_pin_selftest.sh:64 | The C9 selftests (channel_stable, channel_short) assert only that the output contains `rust-toolchain.toml`. The CI-drift message at tests/repo/test_toolchain_pin.sh:65 and the docs message also contain that text, so the selftest stays green when the channel refusal at tests/repo/test_toolchain_pin.sh:22 is deleted. That breaks specs/rules/testing-rules.md:5: an error assertion must grep for text that only the intended failure path produces. | I replaced the `fail "rust-toolchain.toml: channel '${raw}' is not an exact X.Y.Z version"` line with `:`, and the selftest still printed `PASS: toolchain_pin check behaves`. In that mutated state, `channel = "stable"` makes the check print `FAIL: .github/workflows/ci.yml: toolchain '1.99.1' differs from rust-toolchain.toml channel unknown`. That blames the ci.yml value, which is correct, and never names the bad channel.

What I traced and found sound:

- Ran the builder image `clux/muslrust:1.99.1-stable-2026-11-03` offline (`--network none`) with the tree's `rust-toolchain.toml` and `RUSTUP_TOOLCHAIN=stable`. It reports rustc and cargo 1.99.1 on `stable-aarch64-unknown-linux-gnu` ("overridden by environment variable RUSTUP_TOOLCHAIN") with the `aarch64-unknown-linux-musl` target installed, so the build needs no rustup sync.
- No locked package declares a rust-version above 1.88 (`cargo metadata --locked --offline`), so pinning 1.99.1 does not break the build.
- `release.yml` installs no toolchain. ci.yml has one `toolchain:` key, and YAML reads `1.99.1` as a string. No other tracked check or doc still names `stable`.
- On the END tree, `test_toolchain_pin.sh`, its selftest and `test_advisories.sh` all exit 0.
- The new Expires grep in `test_advisories.sh` is exercised: on `deny-0459-no-expires.toml` the only failure is the Expires one, so deleting the grep turns C5 red.
- The `deny.toml` comment line keeps the 0459 entry parseable.
- 2026-11-14 plus 60 days is 2027-01-13.

Anchors:

```quote tests/repo/test_toolchain_pin_selftest.sh:78
d=$(make_repo env_other_stage '"1.99.1"' "$GOOD_FROM" $'COPY . .\nRUN cargo build --release -p netray' 1.99.1)
```

```quote tests/repo/test_toolchain_pin.sh:46
        l ~ /^[[:space:]]*from[[:space:]]/ { if (in_b) exit; if (l ~ /[[:space:]]as[[:space:]]+builder[[:space:]]*$/) in_b = 1; next }
```

```quote tests/repo/test_toolchain_pin_selftest.sh:64
expect_fail channel_stable '"stable"' "$GOOD_FROM" "$GOOD_BODY" 1.99.1 rust-toolchain.toml
```

```quote tests/repo/test_toolchain_pin.sh:65
            [ "$v" = "$channel" ] || fail ".github/workflows/ci.yml: toolchain '$v' differs from rust-toolchain.toml channel ${channel:-unknown}"
```

```quote tests/repo/test_toolchain_pin.sh:22
        fail "rust-toolchain.toml: channel '${raw}' is not an exact X.Y.Z version"
```

```quote specs/rules/testing-rules.md:5
- A check that could pass on empty or unrelated input must prove what it observed: a scan reports how many files it read and fails on zero; an error assertion greps for text only the intended failure path produces, never a key name an unrelated error also lists. Unenforced; precedents in `specs/solutions/` with class `check-passes-without-observing`.
```
### Spec reader

COUNTS blockers=0 majors=2 minors=2
REQUIREMENTS met=6 partial=2 missed=0 contradicted=0
MAJOR | tests/repo/test_toolchain_pin.sh:7 | The drift check takes no root directory override: it always `cd`s to its own location's grandparent, so fixture runs only work by copying the script into a fixture repo | R5, `ROOT=/tmp/fx bash tests/repo/test_toolchain_pin.sh` or `bash tests/repo/test_toolchain_pin.sh /tmp/fx` both ignore `/tmp/fx` and read the repository's own files
MAJOR | crates/common/CLAUDE.md:7 | `crates/common/CLAUDE.md` names `1.99.1` but not the check of R5 (`tests/repo/test_toolchain_pin.sh`) | R6, the amended bullet reads "the toolchain is `1.99.1` (`rust-toolchain.toml`)" with no mention of the check; only R-A2 names it
MINOR | tests/repo/test_toolchain_pin.sh:70 | The drift check also greps R-A2 and `crates/common/CLAUDE.md` and fails on them, although the decision log says no check greps them and R5 says it passes when all agree | R5 / decision log, channel, tag, ci.yml and ENV all on 1.99.1 but `crates/common/CLAUDE.md` without `1.99.1` -> exit 1
MINOR | tests/repo/test_toolchain_pin.sh:24 | The drift check adds a fifth fail condition the spec does not name: `components` must be exactly `["rustfmt", "clippy"]` in that order | R5, `components = ["clippy", "rustfmt"]` with all versions agreeing -> exit 1

#### Anchors

R5 asks for a root directory override:

````quote specs/features/release-toolchain-pin/spec.md:69
   - It runs offline with grep/sed over tracked files, never docker, and takes a root directory override so its scenarios run against fixture copies.
````

The check derives its root from its own path and reads no argument or variable:

````quote tests/repo/test_toolchain_pin.sh:6
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
````

The selftest gets fixture runs by copying the script into a fixture repo instead:

````quote tests/repo/test_toolchain_pin_selftest.sh:23
  mkdir -p "$d/tests/repo" "$d/.github/workflows" "$d/specs/rules" "$d/crates/common" \
    && cp "$check" "$d/tests/repo/" || fail "cannot copy check"
````

Trace: `ROOT=/tmp/fx bash tests/repo/test_toolchain_pin.sh` runs line 7, `cd`s to the worktree root, and checks the worktree's `rust-toolchain.toml`, not `/tmp/fx`'s. A path argument goes the same way, because `$1` is never read. The plan restated the clause as "root override by copying it into a fixture repo", which is a different mechanism from the one the requirement describes.

````quote specs/features/release-toolchain-pin/plan.p1.md:22
1. (no file) — none — C5 is the committed check itself (tests/repo/test_toolchain_pin.sh, root override by copying it into a fixture repo, as test_toolchain_pin_selftest.sh does); C9–C13 are its selftest scenarios, already passing; C8 turns green once G1–G4 land. Verify with `bash tests/repo/test_toolchain_pin.sh` (expect `PASS: toolchain pinned to 1.99.1 …`), `bash tests/repo/test_toolchain_pin_selftest.sh`, then `just test-repo` (its `tests/repo/test_*.sh` glob, justfile:28, already runs both) — hangs on nothing new; the check and the `test-repo` recipe, justfile:24-32, exist. C7 is verified separately with `just image` (justfile:84-85) and is not a group.
````

R6 asks both documents to name the version and the check:

````quote specs/features/release-toolchain-pin/spec.md:70
6. `specs/rules/workflow-rules.md` R-A2 and `crates/common/CLAUDE.md` name the pinned version and
   the check of 5 instead of `stable`.
````

`crates/common/CLAUDE.md` at END names the version only:

````quote crates/common/CLAUDE.md:7
- Edition 2024 (workspace), `rust-version = "1.88"` in `Cargo.toml`; the toolchain is `1.99.1`
  (`rust-toolchain.toml`). CI is the root `.github/workflows/` (`ci.yml`, `release.yml`).
````

`git show 077eadbb:crates/common/CLAUDE.md | grep toolchain_pin` prints nothing. R-A2 (`specs/rules/workflow-rules.md:111`) does name `tests/repo/test_toolchain_pin.sh`, so R6 is half built.

The check greps the docs, although the decision log says no check does:

````quote specs/features/release-toolchain-pin/spec.md:129
- No existing check asserts `stable`, so none moves; R-A2 and `crates/common/CLAUDE.md:7` are amended text, no check greps them (agent default, confirmed by operator, 2026-11-14).
````

````quote specs/features/release-toolchain-pin/spec.md:64
   builder stage lacks `RUSTUP_TOOLCHAIN=stable`; it passes when all agree. Parsing:
````

````quote tests/repo/test_toolchain_pin.sh:70
# --- docs name the version ---
if [ -n "$channel" ]; then
    r=$(grep -F 'R-A2' specs/rules/workflow-rules.md 2>/dev/null | head -n1)
    case "$r" in *"$channel"*) ;; *) fail "specs/rules/workflow-rules.md: R-A2 line does not name channel $channel" ;; esac
    case "$r" in *'tests/repo/test_toolchain_pin.sh'*) ;; *) fail "specs/rules/workflow-rules.md: R-A2 line does not name tests/repo/test_toolchain_pin.sh" ;; esac
    case "$r" in *'`stable`'*) fail "specs/rules/workflow-rules.md: R-A2 line still says the toolchain is \`stable\` instead of $channel" ;; esac
    c=$(cat crates/common/CLAUDE.md 2>/dev/null)
    case "$c" in *"$channel"*) ;; *) fail "crates/common/CLAUDE.md: does not name channel $channel" ;; esac
    case "$c" in *'the toolchain is `stable`'*) fail "crates/common/CLAUDE.md: still says the toolchain is \`stable\` instead of $channel" ;; esac
````

Trace: take a tree where the channel, the builder tag, `ci.yml` and the ENV all say 1.99.1 but `crates/common/CLAUDE.md` does not contain `1.99.1`. Line 77 fails, and the check exits 1 even though the three versions agree.

The check also requires an exact components list, which R5 does not ask for:

````quote tests/repo/test_toolchain_pin.sh:24
    grep -Eq '^[[:space:]]*components[[:space:]]*=[[:space:]]*\[[[:space:]]*"rustfmt"[[:space:]]*,[[:space:]]*"clippy"[[:space:]]*\]' rust-toolchain.toml \
        || fail "rust-toolchain.toml: components are not [\"rustfmt\", \"clippy\"]"
````

The Goal names drift between the three versions as the only thing the check refuses:

````quote specs/features/release-toolchain-pin/spec.md:8
One Rust version, 1.99.1, is named in `rust-toolchain.toml`, in the Dockerfile builder's image tag
and in CI's toolchain input, a repository check refuses any drift between the three, and the image
````

Trace: with `components = ["clippy", "rustfmt"]` and every version on 1.99.1, the regex expects `"rustfmt"` first, so it does not match and the check exits 1.

#### Met requirements

R1 (`rust-toolchain.toml:2-3`), R2 (`Dockerfile:7`), R3 (`Dockerfile:9`, set before the `RUN cargo build` at `:14`), R4 (`ci.yml:28`, components unchanged), R7 (`deny.toml:31`, the last line of the comment run with no blank line) and R8 (`tests/repo/test_advisories.sh:74-75` matches the date pattern against the joined comment with no clock comparison, `EXPECTED_IGNORES` at `:20` is unchanged, and `deny-all-correct.toml` gains the line under `ADLC-Test-Change` in cbe9fcb8) all hold at END. `bash tests/repo/test_toolchain_pin.sh`, `bash tests/repo/test_toolchain_pin_selftest.sh` and `bash tests/repo/test_advisories.sh` all print PASS.

Phase shape: there is no `bug.md`. Phase 1 has a boundary scenario (the `just image` build log and `rustc --version`), and so does Phase 2 (the `cargo deny check advisories` output), so neither phase gets a shape finding.

### Refutation

F1 | tests/repo/test_toolchain_pin.sh:7 | CONFIRMED — R5 requires a root directory override; line 7 hard-cds to the script's grandparent, no ROOT/arg handling anywhere — confidence 9 — fix right — class spec-required parameter missing from a check script; neighbours other R5 parsing clauses; R5 "naming both values" messages — new
F2 | crates/common/CLAUDE.md:7 | CONFIRMED — R6 says both files name the version and the check of 5; lines 7-8 name 1.99.1 only, no test_toolchain_pin.sh — confidence 8 — fix right — class doc amendment omits a required element of R6; neighbours R-A2 text in workflow-rules.md (verify it names the check) — new

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| F1 | CONFIRMED | 9 | held | `ROOT=/nonexistent bash tests/repo/test_toolchain_pin.sh /nonexistent` passes over the repository's own files (no override honoured) |
| F2 | CONFIRMED | 8 | held | `sed -n 5,9p crates/common/CLAUDE.md`: names `1.99.1`, not `tests/repo/test_toolchain_pin.sh` |

### Findings

F1 | MAJOR | tests/repo/test_toolchain_pin.sh:7 | spec-required parameter missing from a check script | open
F2 | MAJOR | crates/common/CLAUDE.md:7 | doc amendment omits a required element of R6 | open

### Roll call

P03 | absence — sg/rg for *Status enums and codenames in templates/API — range touches no Rust types, templates or serializers
P10 | absence — follow error/timeout branches of changed checks — no check or scoring path changed
P12 | absence — sg reqwest builders and module entry calls — no outbound call or engine code changed
P13 | absence — read limiter setup — serve command not changed
P18 | absence — sg reqwest::Client outside fetch.rs; test_config_strict.sh — no client or config struct changed
P26 | absence — rg new check IDs — no new check ID in the range
P35 | convergence — changed rule in specs/rules/ arrives with enforcing test — workflow-rules.md:111 (R-A2) enforced by tests/repo/test_toolchain_pin.sh (+ selftest); deny.toml expiry by tests/repo/test_advisories.sh
P36 | absence — generated file changes only with its source — no generated file in git diff --stat
P40 | absence — diff of module add/remove — no module added or removed

### Summary

Before refutation: reader 0/0/2, spec reader 0/2/2 (REQUIREMENTS met=6 partial=2 missed=0 contradicted=0). After refutation: blockers 0, majors 2 (F1, F2, both spec reader), minors 4 (deferred). verified=2, held=2. Roll call: answered 9, convergence 1 (P35), divergence 0, absence 8. Round 1, reader effort xhigh.
