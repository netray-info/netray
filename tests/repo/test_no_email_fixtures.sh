#!/usr/bin/env bash
# specs/features/email-scoring C5: lens's legacy email fixtures are gone and nothing refers to them.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

[ -e crates/lens/tests/email_fixtures ] && fail "crates/lens/tests/email_fixtures still exists"

hits=$(grep -rn --include='*' 'email_fixtures' crates/*/src crates/*/tests 2>/dev/null)
[ -z "$hits" ] || fail "email_fixtures is still mentioned:
$hits"

# Self-test: the grep must fire on a file that mentions it.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
printf 'tests/email_fixtures/x.json\n' > "$work/f.rs"
grep -rq 'email_fixtures' "$work" || fail "self-test: grep did not fire on a fixture"

[ "$fails" -eq 0 ] && echo "ok: no email_fixtures left"
exit "$fails"
