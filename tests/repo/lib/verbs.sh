# Sourced by tests/repo/test_verbs*.sh.
#   recipe_deps <justfile> <recipe>      -> the recipe's dependencies, one per line
#   check_reaches_verify <justfile>      -> exit 0 when every adlc-verify dependency is reached from check;
#                                           else prints the first unreached one and exits 1
#
# check may run test-rust in place of test-rust-offline (one compile of the workspace tests,
# v2-ci-speed R1-R3) only while test-rust is a superset: its data branch runs cargo test --workspace
# with no --exclude and its no-data branch delegates to `just test-rust-offline`.

recipe_deps() {
    just --justfile "$1" --show "$2" 2>/dev/null | grep -m1 -E "^$2[^:=]*:" | sed -E 's/^[^:]*://' | tr ' ' '\n' | grep -v '^$'
}

_test_rust_is_superset() {
    local body
    body=$(just --justfile "$1" --show test-rust 2>/dev/null) || return 1
    # The shape: `if [ -f <...>.mmdb ]; then` / cargo test --workspace / `else` / just test-rust-offline.
    # Read as consecutive non-comment lines, so swapped branches or a dead `if false` do not pass.
    local shape
    shape=$(grep -vE '^[[:space:]]*(#|echo )' <<<"$body" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//' \
        | grep -A3 -E '^if \[ -f [^]]*\.mmdb \]; then$' | head -4 | tr '\n' '|')
    case "$shape" in
        if*'; then|cargo test --workspace|else|just '*'test-rust-offline|') ;;
        *) echo "test-rust is not: if [ -f <data>.mmdb ]; then cargo test --workspace; else just test-rust-offline (got: $shape)"; return 1 ;;
    esac
    if grep -E 'cargo test' <<<"$body" | grep -q -- '--exclude'; then
        echo "test-rust excludes a crate: $(grep -E 'cargo test' <<<"$body" | grep -- '--exclude' | head -1 | sed 's/^ *//')"
        return 1
    fi
}

check_reaches_verify() {
    local jf=$1 check_deps verify_deps d why
    check_deps=$(recipe_deps "$jf" check) || true
    [ -n "$check_deps" ] || { echo "recipe 'check' has no dependencies in $jf"; return 1; }
    grep -qx 'adlc-verify' <<<"$check_deps" && return 0
    verify_deps=$(recipe_deps "$jf" adlc-verify) || true
    for d in $verify_deps; do
        grep -qx -- "$d" <<<"$check_deps" && continue
        if [ "$d" = test-rust-offline ] && grep -qx 'test-rust' <<<"$check_deps"; then
            why=$(_test_rust_is_superset "$jf") && continue
            echo "check runs test-rust in place of test-rust-offline, but $why"
            return 1
        fi
        echo "adlc-verify dependency '$d' is not reached from check"
        return 1
    done
}
