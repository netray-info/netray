#!/usr/bin/env bash
# specs/features/v2-result-page, R30: SDD Phase 4 rows V4.5, V4.6 and V4.13 name v2-result-page as
# their builder; 5.1 says the result routes mount on lens's router until v2-cutover; 5.2 says a stale
# page's visit posts trigger=visit and a fresh visit starts nothing; 4.6 names the spectrum card.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
sdd=specs/sdd/v2.md

fails=0
fail() { echo "FAIL: $1"; fails=1; }

# section_text <from-heading-prefix> <to-heading-prefix>
section_text() {
    awk -v from="$1" -v to="$2" 'index($0, from) == 1 { on = 1; next } (index($0, to) == 1 || /^## /) { on = 0 } on' "$sdd"
}

for id in V4.5 V4.6 V4.13; do
    row=$(grep -F -- "| $id |" "$sdd")
    [ -n "$row" ] || { fail "SDD has no row $id"; continue; }
    grep -qF 'built by `v2-result-page`' <<< "$row" || fail "SDD row $id does not say: built by \`v2-result-page\`"
done

s51=$(section_text '### 5.1' '### 5.2')
[ -n "$s51" ] || fail "SDD has no section 5.1"
grep -qE "lens's router" <<< "$s51" || fail "SDD 5.1 does not say: the result routes mount on lens's router"
grep -qF 'v2-cutover' <<< "$s51" || fail "SDD 5.1 does not say: until \`v2-cutover\`"

s52=$(section_text '### 5.2' '### 5.3')
[ -n "$s52" ] || fail "SDD has no section 5.2"
grep -qF 'trigger=visit' <<< "$s52" || fail "SDD 5.2 does not say: a stale page's visit posts trigger=visit"
grep -qiE 'fresh visit starts nothing' <<< "$s52" || fail "SDD 5.2 does not say: a fresh visit starts nothing"

s46=$(section_text '### 4.6' '### 4.7')
[ -n "$s46" ] || fail "SDD has no section 4.6"
grep -qiE 'spectrum card|spectrum::layout' <<< "$s46" || fail "SDD 4.6 does not name the spectrum card"
grep -qF 'v2-result-page' <<< "$s46" || fail "SDD 4.6 does not name v2-result-page"

[ "$fails" -eq 0 ] && echo "PASS: test_result_page_sdd"
exit "$fails"
