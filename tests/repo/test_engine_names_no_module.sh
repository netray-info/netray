#!/usr/bin/env bash
# specs/features/v2-model: crates/engine names no module crate. Its Cargo.toml lists no module
# crate in [dependencies], [dev-dependencies], [build-dependencies] or their [target.*]
# variants; netray-model and external crates only. The scan is a text scan of the TOML. A
# self-test runs it on fixtures: three bad manifests it must flag, one good manifest (with
# lookalikes) it must pass.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

modules='beacon tlsight spectra prism ifconfig-rs lens netray-dns netray-tls netray-http netray-email netray-ip'

# scan <manifest>: print each module crate listed as a dependency, one per line.
scan() {
    awk -v mods="$modules" '
        BEGIN { n = split(mods, a, " "); for (i = 1; i <= n; i++) mod[a[i]] = 1 }
        function strip(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); gsub(/^["\047]|["\047]$/, "", s); return s }
        /^[ \t]*\[/ {
            h = $0; sub(/^[ \t]*\[/, "", h); sub(/\][ \t]*(#.*)?$/, "", h); h = strip(h)
            mode = 0
            if (h ~ /(^|\.)(dev-|build-)?dependencies$/) mode = 1
            else if (match(h, /(^|\.)(dev-|build-)?dependencies\.[^.]+$/)) {
                name = substr(h, RSTART, RLENGTH); sub(/^.*dependencies\./, "", name)
                name = strip(name); if (name in mod) print name
            }
            next
        }
        mode == 1 && /^[ \t]*[A-Za-z0-9_"\047-]+[ \t]*(=|\.)/ {
            k = $0; sub(/^[ \t]+/, "", k); sub(/[ \t]*(=|\.).*$/, "", k); k = strip(k)
            if (k in mod) print k
        }
    ' "$1" | sort -u
}

fixdir=tests/repo/fixtures/engine-deps
for f in bad-deps bad-dev bad-build; do
    p=$fixdir/$f.toml
    if [ -f "$p" ]; then
        scan "$p" | grep -qx beacon || fail "scanner does not report beacon in $p"
    else
        fail "$p missing"
    fi
done
if [ -f "$fixdir/good.toml" ]; then
    [ -z "$(scan "$fixdir/good.toml")" ] || fail "scanner flags $fixdir/good.toml"
else
    fail "$fixdir/good.toml missing"
fi

manifest=crates/engine/Cargo.toml
if [ -f "$manifest" ]; then
    while IFS= read -r name; do
        [ -n "$name" ] && fail "$manifest depends on module crate $name"
    done < <(scan "$manifest")
else
    fail "$manifest missing"
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: test_engine_names_no_module"
