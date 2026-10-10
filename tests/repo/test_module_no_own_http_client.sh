#!/usr/bin/env bash
# specs/features/v2-modules (Phase 6, R19): the module crates (dns, tls, http, email, ip) build no
# reqwest client of their own; outbound HTTP goes through netray_common::fetch. The scanner strips
# `//` comments, then flags `reqwest::Client::new|builder`, `reqwest::ClientBuilder`, and the same
# through a `use reqwest::Client [as X]` / `use reqwest::{Client [as X]}` alias (`X::new(`,
# `X::builder(`, `ClientBuilder::new(`). Test-only files (`*_tests.rs`, `tests/`) are excluded. A
# self-test runs the scanner on fixtures: two it must flag, one it must pass.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

# scan <file>...: print "file:line: text" for each client construction; exit 0 when any found.
scan() {
    python3 -I -c '
import re, sys
found = False
direct = re.compile(r"reqwest::(Client::(new|builder)\b|ClientBuilder\b)")
imp = re.compile(r"use\s+reqwest::(?:\{([^}]*)\}|(Client|ClientBuilder)(?:\s+as\s+(\w+))?)\s*;")
for path in sys.argv[1:]:
    src = open(path, encoding="utf-8").read()
    lines = [re.sub(r"//.*", "", l) for l in src.splitlines()]
    text = "\n".join(lines)
    names = set()
    for m in imp.finditer(text):
        items = m.group(1).split(",") if m.group(1) else [m.group(2) + (" as " + m.group(3) if m.group(3) else "")]
        for it in items:
            parts = it.split()
            if not parts or parts[0] not in ("Client", "ClientBuilder"):
                continue
            alias = parts[2] if len(parts) == 3 and parts[1] == "as" else parts[0]
            names.add((parts[0], alias))
    pats = []
    for orig, alias in names:
        pats.append(re.compile(r"\b%s::(new|builder)\s*\(" % re.escape(alias)) if orig == "Client"
                    else re.compile(r"\b%s::(new|default)\s*\(" % re.escape(alias)))
    for i, l in enumerate(lines, 1):
        if direct.search(l) or any(p.search(l) for p in pats):
            print("%s:%d: %s" % (path, i, l.strip()))
            found = True
sys.exit(0 if found else 1)
' "$@"
}

fixdir=tests/repo/fixtures/http-client
for c in alias builder; do
    [ -f "$fixdir/$c.rs" ] || { fail "$fixdir/$c.rs missing"; continue; }
    scan "$fixdir/$c.rs" > /dev/null || fail "self-test: scanner does not flag $fixdir/$c.rs"
done
if [ -f "$fixdir/clean.rs" ]; then
    scan "$fixdir/clean.rs" > /dev/null && fail "self-test: scanner flags $fixdir/clean.rs"
else
    fail "$fixdir/clean.rs missing"
fi

files=()
while IFS= read -r f; do
    [[ "$f" =~ _tests\.rs$ || "$f" =~ /tests/ ]] && continue
    files+=("$f")
done < <(git ls-files 'crates/dns/src/*.rs' 'crates/tls/src/*.rs' 'crates/http/src/*.rs' 'crates/email/src/*.rs' 'crates/ip/src/*.rs')
echo "scanned ${#files[@]} files"
[ "${#files[@]}" -gt 0 ] || fail "no module source files scanned"

if [ "${#files[@]}" -gt 0 ] && hits=$(scan "${files[@]}"); then
    fail "module builds its own reqwest client (use netray_common::fetch):"
    echo "$hits"
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_module_no_own_http_client"
