#!/usr/bin/env bash
# specs/features/v2-spectrum, requirements 1 and 9: label placement lives once, in the pure module
# crates/web/src/spectrum.rs (declared in lib.rs; no askama, no I/O, no global state), no other
# tracked Rust file defines `fn label_width`, and SDD section 5.5 carries the amended wording:
# "every warn, fail and fixed line", and in point 2 left alignment, half-open intervals and the
# over-wide box at 0. A self-test runs the same check on scratch trees it must refuse.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

# rs_files <root>: Rust files under <root>/crates (tracked ones for the real tree).
rs_files() {
    if [ "$1" = "." ]; then git ls-files 'crates/*.rs'; else (cd "$1" && find crates -name '*.rs'); fi
}

# problems <root>: print one line per violation under <root>.
problems() {
    local r=$1 spec=$1/crates/web/src/spectrum.rs
    if [ ! -f "$spec" ]; then
        echo "crates/web/src/spectrum.rs is missing"
    else
        grep -Eq '^pub mod spectrum;' "$r/crates/web/src/lib.rs" 2>/dev/null ||
            echo "crates/web/src/lib.rs does not declare pub mod spectrum;"
        # Purity by allowlist, over the code with comments stripped and lines joined: `use` only
        # from crate, super, netray_model, netray_engine and std::collections; no other std, core
        # or alloc path and no grouped std import; only the macros below; no static, atomic,
        # unsafe, extern or include. A blocklist of I/O names misses print macros and std::process.
        local flat u m
        flat=$(sed 's://.*$::' "$spec" | tr '\n' ' ')
        ! grep -q 'askama' <<< "$flat" || echo "spectrum.rs is not pure: it contains askama"
        while IFS= read -r u; do
            [ -n "$u" ] || continue
            case "$u" in
            'use crate::'* | 'use super::'* | 'use netray_model::'* | 'use netray_engine::'* | 'use std::collections::'*) ;;
            *) echo "spectrum.rs is not pure: it imports outside the allowlist: $u" ;;
            esac
        done < <(grep -oE '(^|[^A-Za-z0-9_])use[[:space:]]+[^;]+' <<< "$flat" |
            sed -E 's/^[^u]*use[[:space:]]+(::)?//; s/[[:space:]]//g; s/^/use /')
        while IFS= read -r u; do
            [ -n "$u" ] || continue
            case "$u" in
            std::collections::*) ;;
            *) echo "spectrum.rs is not pure: it names $u" ;;
            esac
        done < <(grep -oE '(^|[^A-Za-z0-9_])(::)?(std|core|alloc)::([A-Za-z_][A-Za-z0-9_]*|[{])(::)?' <<< "$flat" |
            sed -E 's/^[^a-z]*(::)?//')
        while IFS= read -r m; do
            [ -n "$m" ] || continue
            case "$m" in
            panic | matches | vec | format | assert | assert_eq | assert_ne | debug_assert | unreachable) ;;
            *) echo "spectrum.rs is not pure: it calls the macro $m!" ;;
            esac
        done < <(grep -oE '[A-Za-z_][A-Za-z0-9_]*![[:space:]]*[([{]' <<< "$flat" | sed -E 's/!.*$//')
        ! grep -Eq "(^|[^'A-Za-z0-9_])static[[:space:]]" <<< "$flat" ||
            echo "spectrum.rs is not pure: it holds a static"
        local kw
        for kw in Atomic unsafe extern include; do
            ! grep -Eq "(^|[^A-Za-z0-9_])$kw" <<< "$flat" || echo "spectrum.rs is not pure: it contains $kw"
        done
    fi
    local f
    while IFS= read -r f; do
        [ "$f" = "crates/web/src/spectrum.rs" ] && continue
        case "$f" in */tests/*) continue ;; esac
        ! grep -q 'fn label_width' "$r/$f" || echo "$f defines fn label_width (placement lives in spectrum.rs)"
    done < <(rs_files "$r")

    local sdd=$r/specs/sdd/v2.md sec p2
    sec=$(awk '/^### 5\.5/ { on = 1; next } /^### 5\.6/ { on = 0 } on' "$sdd" 2>/dev/null)
    if [ -z "$sec" ]; then
        echo "specs/sdd/v2.md has no section 5.5"
        return 0
    fi
    ! grep -q 'every non-passing line' <<< "$sec" || echo "SDD 5.5 still says 'every non-passing line'"
    grep -q 'every warn, fail and fixed line' <<< "$sec" || echo "SDD 5.5 does not say 'every warn, fail and fixed line'"
    p2=$(awk '/^2\. / { on = 1 } /^3\. / { on = 0 } on' <<< "$sec")
    grep -qi 'left' <<< "$p2" || echo "SDD 5.5 point 2 does not state left alignment"
    grep -q 'half-open' <<< "$p2" || echo "SDD 5.5 point 2 does not say half-open"
    grep -q 'wider than the plate' <<< "$p2" || echo "SDD 5.5 point 2 does not cover a box wider than the plate"
    return 0
}

# good <dir>: a scratch tree that violates nothing.
good() {
    mkdir -p "$1/crates/web/src" "$1/crates/dns/src" "$1/specs/sdd"
    printf '%s\n' 'pub mod spectrum;' > "$1/crates/web/src/lib.rs"
    printf '%s\n' 'pub fn label_width(n: usize) -> f64 { n as f64 }' > "$1/crates/web/src/spectrum.rs"
    echo 'fn other() {}' > "$1/crates/dns/src/lib.rs"
    printf '%s\n' '### 5.5 Label placement' '1. a' \
        '2. Labels are left-aligned at the leader; intervals are half-open. A box wider than the plate sits at 0.' \
        '3. Every warn, fail and fixed line keeps its label.' '### 5.6 Trace' \
        'every non-passing line (outside 5.5)' > "$1/specs/sdd/v2.md"
    sed -i.bak 's/Every warn/every warn/' "$1/specs/sdd/v2.md" && rm "$1/specs/sdd/v2.md.bak"
}

fails=0
fail() { echo "FAIL: $1"; fails=1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

good "$tmp/good"
[ -z "$(problems "$tmp/good")" ] || fail "self-test: the good tree reported '$(problems "$tmp/good")'"

expect() { # expect <name> <want>: the tree $tmp/<name> must report <want>
    printf '%s\n' "$(problems "$tmp/$1")" | grep -q "$2" || fail "self-test: $1 did not report '$2'"
}

good "$tmp/nospec"; rm "$tmp/nospec/crates/web/src/spectrum.rs"
expect nospec "spectrum.rs is missing"

good "$tmp/nodecl"; echo 'pub mod gate;' > "$tmp/nodecl/crates/web/src/lib.rs"
expect nodecl "does not declare pub mod spectrum"

impure() { # impure <name> <line> <want>: spectrum.rs plus <line> must report <want>
    good "$tmp/$1"; printf '%s\n' "$2" >> "$tmp/$1/crates/web/src/spectrum.rs"
    expect "$1" "$3"
}
impure askama 'use askama::Template;' 'imports outside the allowlist: use askama::'
impure fs 'use std::fs;' 'names std::fs'
impure fullpath 'fn f() { let _ = ::std::fs::read("x"); }' 'names std::fs'
impure process 'fn f() { std::process::exit(1) }' 'names std::process'
impure time 'fn f() { let _ = std::time::SystemTime::now(); }' 'names std::time'
impure corepath 'use core::cell::Cell;' 'outside the allowlist'
impure firstgroup 'use std::{env as e, fmt};' 'names std::{'
impure dbg 'fn f(x: f64) -> f64 { dbg!(x) }' 'macro dbg!'
impure println 'fn f() { println!("x") }' 'macro println!'
impure eprintln 'fn f() { eprintln! ("x") }' 'macro eprintln!'
impure tl 'thread_local! { }' 'macro thread_local!'
impure lazy 'lazy_static! { }' 'macro lazy_static!'
impure unsafe 'fn f() { unsafe { } }' 'contains unsafe'
impure include 'const S: &str = include_str!("x");' 'macro include_str!'
impure otherdep 'use once_cell::sync::Lazy;' 'outside the allowlist'
good "$tmp/allowed"; printf '%s\n' 'use std::collections::BTreeMap;' 'use netray_model::Status;' \
    'use netray_engine::Catalogue;' '// std::fs and println!("x") in a comment' \
    'fn f() { let _ = matches!(1, 1); let _ = vec![1]; panic!("x") }' >> "$tmp/allowed/crates/web/src/spectrum.rs"
[ -z "$(problems "$tmp/allowed")" ] || fail "self-test: the allowlist refused: $(problems "$tmp/allowed")"

good "$tmp/grouped"; printf '%s\n' 'use std::{' '    env,' '    fs,' '};' >> "$tmp/grouped/crates/web/src/spectrum.rs"
expect grouped "names std::{"
good "$tmp/static"; echo 'static N: usize = 0;' >> "$tmp/static/crates/web/src/spectrum.rs"
expect static "holds a static"
good "$tmp/atomic"; echo 'const A: AtomicUsize = AtomicUsize::new(0);' >> "$tmp/atomic/crates/web/src/spectrum.rs"
expect atomic "contains Atomic"
good "$tmp/lifetime"; echo "pub const S: &'static str = \"8px\";" >> "$tmp/lifetime/crates/web/src/spectrum.rs"
[ -z "$(problems "$tmp/lifetime")" ] || fail "self-test: a 'static lifetime was reported: $(problems "$tmp/lifetime")"

good "$tmp/dup"; echo 'fn label_width() {}' > "$tmp/dup/crates/dns/src/lib.rs"
expect dup "crates/dns/src/lib.rs defines fn label_width"

good "$tmp/old"; echo 'every non-passing line' >> "$tmp/old/specs/sdd/v2.md"
sed -i.bak '/^### 5.6/d' "$tmp/old/specs/sdd/v2.md"; rm "$tmp/old/specs/sdd/v2.md.bak"
expect old "still says 'every non-passing line'"

good "$tmp/nofixed"; sed -i.bak 's/every warn, fail and fixed line/every warn line/' "$tmp/nofixed/specs/sdd/v2.md"
expect nofixed "does not say 'every warn, fail and fixed line'"

good "$tmp/noleft"; sed -i.bak 's/left-aligned/centred/' "$tmp/noleft/specs/sdd/v2.md"
expect noleft "left alignment"

good "$tmp/nohalf"; sed -i.bak 's/half-open/closed/' "$tmp/nohalf/specs/sdd/v2.md"
expect nohalf "half-open"

good "$tmp/nowide"; sed -i.bak 's/wider than the plate/big/' "$tmp/nowide/specs/sdd/v2.md"
expect nowide "wider than the plate"

good "$tmp/nosec"; echo '# SDD' > "$tmp/nosec/specs/sdd/v2.md"
expect nosec "no section 5.5"

out=$(problems .)
if [ -n "$out" ]; then
    while IFS= read -r line; do fail "$line"; done <<< "$out"
fi

[ "$fails" -eq 0 ] && echo "ok: spectrum module is pure and sole owner of label placement; SDD 5.5 amended"
exit "$fails"
