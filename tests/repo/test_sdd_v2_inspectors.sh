#!/usr/bin/env bash
# specs/features/v2-inspectors, requirement 22: specs/sdd/v2.md 6.1, 6.2 and the Phase 4 row V4.7 are
# amended with the code that builds the evidence views and inspectors.
set -euo pipefail
cd "$(dirname "$0")/../.."

sdd=specs/sdd/v2.md
fails=0
pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1"; fails=1; }
# has <text> <fixed string> <description>
has() { if grep -qF -- "$2" <<< "$1"; then pass "$3"; else fail "$3"; fi; }

s61=$(awk '/^### 6\.1 / { on = 1; next } /^### / { on = 0 } on' "$sdd")
s62=$(awk '/^### 6\.2 / { on = 1; next } /^## |^### / { on = 0 } on' "$sdd")
p4=$(awk '/^### Phase 4 / { on = 1; next } /^### / { on = 0 } on' "$sdd")
v47=$(grep -E '^\| V4\.7 ' <<< "$p4" || true)

[ -n "$s61" ] && pass "SDD has section 6.1" || fail "SDD has no section 6.1"
[ -n "$s62" ] && pass "SDD has section 6.2" || fail "SDD has no section 6.2"

if grep -qF 'crates/tlsight/' <<< "$s61"; then fail "6.1 still cites crates/tlsight/"; else pass "6.1 does not cite crates/tlsight/"; fi
if grep -qF 'crates/tlsight/' <<< "$s62"; then fail "6.2 still cites crates/tlsight/"; else pass "6.2 does not cite crates/tlsight/"; fi
has "$s62" 'crates/tls/src/tls/connect.rs' "6.2 cites crates/tls/src/tls/connect.rs"

has "$s61" 'ev-<block id>' "6.1 names the ev-<block id> anchors"
has "$s61" 'created_at' "6.1 names the run line's created_at timestamp"
has "$s61" '/inspect/' "6.1 states the explore-live derivation under /inspect/"

has "$s62" 'POST /inspect/<tool>/run' "6.2 names POST /inspect/<tool>/run"
has "$s62" '[inspect.' "6.2 names the [inspect. limits"
has "$s62" 'deadline_ms' "6.2 names deadline_ms"
has "$s62" 'max_concurrent' "6.2 names max_concurrent"
has "$s62" 'Catalogue::for_protocol' "6.2 names Catalogue::for_protocol"
has "$s62" 'v2-cutover' "6.2 states the inspectors' placement before v2-cutover"
has "$s62" '/inspect/<tool>' "6.2 names /inspect/<tool>"

[ -n "$v47" ] && pass "Phase 4 table has a V4.7 row" || fail "Phase 4 table has no V4.7 row"
has "$v47" 'v2-inspectors' "V4.7 row names v2-inspectors as built"

exit "$fails"
