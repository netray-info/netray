#!/usr/bin/env bash
# Selftest for test_toolchain_pin.sh: builds throwaway git repositories, runs the repository's own
# check on each (repository root as first argument), and checks that the check
# fails on a non-X.Y.Z channel, a builder tag or ci.yml toolchain that differs from the channel, and
# a builder stage without RUSTUP_TOOLCHAIN=stable (missing, after the cargo call, or in another
# stage), refuses an empty or missing root argument, and passes on dated and undated tags and on both ENV forms.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
root=$(pwd)

fail() { echo "FAIL: $1"; exit 1; }

check="$root/tests/repo/test_toolchain_pin.sh"
[ -f "$check" ] || fail "tests/repo/test_toolchain_pin.sh does not exist"

tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT

# make_repo <name> <channel> <from-line> <builder-body> <ci-toolchain>: a fresh git repo holding
# minimal fixtures (no copy of the check); prints its path. <builder-body> is the text between the
# builder FROM and the runtime stage.
make_repo() {
  local d="$tmp/$1"
  mkdir -p "$d/.github/workflows" "$d/specs/rules" "$d/crates/common" || fail "cannot create repo"
  git -C "$d" init -q || fail "git init"
  printf '[toolchain]\nchannel = %s\ncomponents = ["rustfmt", "clippy"]\n' "$2" > "$d/rust-toolchain.toml"
  printf 'FROM node:22 AS frontend\nRUN npm ci\n\n%s\n%s\n\nFROM alpine:3.20\nCOPY --from=builder /app/netray /netray\n' \
    "$3" "$4" > "$d/Dockerfile"
  printf 'jobs:\n  test:\n    steps:\n      - uses: dtolnay/rust-toolchain@master\n        with:\n          toolchain: %s\n          components: rustfmt, clippy\n' \
    "$5" > "$d/.github/workflows/ci.yml"
  printf 'R-A2: The Rust toolchain is 1.99.1, checked by tests/repo/test_toolchain_pin.sh.\n' > "$d/specs/rules/workflow-rules.md"
  printf 'The toolchain is 1.99.1 (tests/repo/test_toolchain_pin.sh).\n' > "$d/crates/common/CLAUDE.md"
  git -C "$d" add -A
  echo "$d"
}

GOOD_FROM='FROM clux/muslrust:1.99.1-stable-2026-11-03 AS builder'
GOOD_BODY=$'ENV RUSTUP_TOOLCHAIN=stable\nCOPY . .\nRUN cargo build --release -p netray'

# run_check <repo>: runs the check inside the repo, output in $out, status in $rc.
run_check() { out=$(bash "$check" "$1" 2>&1); rc=$?; }

# expect_fail <name> <channel> <from> <body> <ci> <needle>...: non-zero and output names every needle.
expect_fail() {
  local name=$1 d n; d=$(make_repo "$1" "$2" "$3" "$4" "$5"); shift 5
  run_check "$d"
  [ "$rc" -ne 0 ] || fail "$name: expected failure"
  for n in "$@"; do
    printf '%s' "$out" | grep -qF -- "$n" || fail "$name: output does not name $n: $out"
  done
}

# expect_pass <name> <channel> <from> <body> <ci>
expect_pass() {
  local d; d=$(make_repo "$1" "$2" "$3" "$4" "$5")
  run_check "$d"
  [ "$rc" -eq 0 ] || fail "$1: expected success: $out"
}

expect_pass pinned '"1.99.1"' "$GOOD_FROM" "$GOOD_BODY" 1.99.1
expect_pass channel_comment '"1.99.1" # pinned' "$GOOD_FROM" "$GOOD_BODY" 1.99.1

# C9: channel is not an exact X.Y.Z.
expect_fail channel_stable '"stable"' "$GOOD_FROM" "$GOOD_BODY" 1.99.1 "channel 'stable' is not an exact X.Y.Z version"
expect_fail channel_short '"1.99"' "$GOOD_FROM" "$GOOD_BODY" 1.99.1 "channel '1.99' is not an exact X.Y.Z version"


# C10: builder tag version differs from the channel.
expect_fail tag_drift '"1.99.1"' 'FROM clux/muslrust:1.99.0-stable-2026-10-14 AS builder' "$GOOD_BODY" 1.99.1 1.99.0 1.99.1

# C11: ci.yml toolchain differs from the channel.
expect_fail ci_stable '"1.99.1"' "$GOOD_FROM" "$GOOD_BODY" stable ci.yml

# C12: builder stage lacks ENV RUSTUP_TOOLCHAIN=stable before the first cargo call.
expect_fail env_missing '"1.99.1"' "$GOOD_FROM" $'COPY . .\nRUN cargo build --release -p netray' 1.99.1 RUSTUP_TOOLCHAIN
expect_fail env_after_cargo '"1.99.1"' "$GOOD_FROM" \
  $'COPY . .\nRUN cargo build --release -p netray\nENV RUSTUP_TOOLCHAIN=stable' 1.99.1 RUSTUP_TOOLCHAIN

# Builder stage holds neither cargo nor ENV; only the runtime stage sets the ENV.
d=$(make_repo env_other_stage '"1.99.1"' "$GOOD_FROM" 'COPY . .' 1.99.1)
printf 'ENV RUSTUP_TOOLCHAIN=stable\n' >> "$d/Dockerfile"
git -C "$d" add -A
run_check "$d"
[ "$rc" -ne 0 ] || fail "env_other_stage: expected failure for ENV in another stage only"
printf '%s' "$out" | grep -qF -- "builder stage has no ENV RUSTUP_TOOLCHAIN=stable" \
  || fail "env_other_stage: output does not name the builder stage ENV: $out"

# C13: dated and undated tags pass; the space ENV form passes.
expect_pass tag_dated '"1.99.1"' 'FROM clux/muslrust:1.99.1-stable-2026-11-03 AS builder' "$GOOD_BODY" 1.99.1
expect_pass tag_undated '"1.99.1"' 'FROM clux/muslrust:1.99.1-stable AS builder' "$GOOD_BODY" 1.99.1
expect_pass env_space_form '"1.99.1"' "$GOOD_FROM" $'ENV RUSTUP_TOOLCHAIN stable\nCOPY . .\nRUN cargo build --release -p netray' 1.99.1

# The repository root argument: a missing or empty one exits 1, names itself and checks nothing.
missing="$tmp/no-such-dir"
out=$(bash "$check" "$missing" 2>&1); rc=$?
[ "$rc" -eq 1 ] || fail "missing root: expected exit 1, got $rc: $out"
printf '%s' "$out" | grep -qF -- "$missing" || fail "missing root: output does not name $missing: $out"
out=$(bash "$check" "" 2>&1); rc=$?
[ "$rc" -eq 1 ] || fail "empty root: expected exit 1, got $rc: $out"

# The repository's own script runs on the temp repos; none holds a copy of it.
copies=$(find "$tmp" -name test_toolchain_pin.sh)
[ -z "$copies" ] || fail "a temp repo holds a copy of the check: $copies"

echo "PASS: toolchain_pin check behaves"
