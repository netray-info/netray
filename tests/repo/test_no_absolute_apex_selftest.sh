#!/usr/bin/env bash
# Selftest for test_no_absolute_apex.sh: builds throwaway git repositories and checks that the
# check fails on an absolute apex or subdomain URL in templates, src and islands/src, exempts the
# ORIGIN definition line, and ignores files outside those trees (islands/test fixtures).
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
root=$(pwd)

fail() { echo "FAIL: $1"; exit 1; }

check="$root/tests/repo/test_no_absolute_apex.sh"
[ -f "$check" ] || fail "tests/repo/test_no_absolute_apex.sh does not exist"

tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT

ORIGIN_LINE='pub(crate) const ORIGIN: &str = "https://netray.info";'

# new_repo <name>: a fresh git repo holding a copy of the check; prints its path.
new_repo() {
  local d="$tmp/$1"
  mkdir -p "$d/tests/repo" && cp "$check" "$d/tests/repo/" || fail "cannot copy check"
  git -C "$d" init -q || fail "git init"
  echo "$d"
}

# run_check <repo>: exit status of the check run inside the repo.
run_check() { bash "$1/tests/repo/test_no_absolute_apex.sh" >/dev/null 2>&1; }

# expect_fail <name> <relpath> <content>
expect_fail() {
  local d; d=$(new_repo "$1")
  mkdir -p "$d/$(dirname "$2")"
  printf '%s\n' "$3" > "$d/$2"
  git -C "$d" add -A
  run_check "$d" && fail "$1: expected failure for $2"
}

# expect_pass <name> <relpath> <content>
expect_pass() {
  local d; d=$(new_repo "$1")
  mkdir -p "$d/$(dirname "$2")"
  printf '%s\n' "$3" > "$d/$2"
  git -C "$d" add -A
  run_check "$d" || fail "$1: expected success for $2"
}

expect_fail template_sub crates/web/templates/x.html '<a href="https://dns.netray.info/x">dns</a>'
expect_fail template_apex crates/web/templates/x.html '<a href="https://netray.info/guides">g</a>'
expect_fail src_sub crates/web/src/pages/x.rs 'const U: &str = "https://tls.netray.info";'
expect_fail islands_src crates/web/islands/src/x.ts 'const u = "https://netray.info/a";'

expect_pass clean_template crates/web/templates/x.html '<a href="/guides">g</a>'
expect_pass origin_only crates/web/src/pages/mod.rs "$ORIGIN_LINE"
expect_pass islands_fixture crates/web/islands/test/fixture.html '<a href="https://dns.netray.info">x</a>'

# ORIGIN exemption covers that line only: the same file with a second literal still fails.
d=$(new_repo origin_plus_other)
mkdir -p "$d/crates/web/src/pages"
printf '%s\n%s\n' "$ORIGIN_LINE" 'const X: &str = "https://ip.netray.info";' > "$d/crates/web/src/pages/mod.rs"
git -C "$d" add -A
run_check "$d" && fail "origin_plus_other: expected failure for second literal"

echo "PASS: no_absolute_apex check behaves"
