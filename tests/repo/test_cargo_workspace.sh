#!/usr/bin/env bash
# Cargo workspace layout: C1 (workspace, path dep on common, workspace deps, edition 2024),
# C6 (cargo metadata members), C7 (no per-crate lock/toolchain/justfile/Makefile).
# Run from the repo root. Prints one line per failed check; exit 1 if any failed.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=()
fail() { fails+=("$1"); }

# --- C1: root manifest is a workspace ---
if [ ! -f Cargo.toml ]; then
    fail "C1: root Cargo.toml is missing"
else
    grep -q '^\[workspace\]' Cargo.toml || fail "C1: root Cargo.toml has no [workspace] table"
    grep -q '^\[workspace.dependencies\]' Cargo.toml || fail "C1: root Cargo.toml has no [workspace.dependencies] table"
    if ! grep -Eq '^edition *= *"2024"' Cargo.toml; then
        for m in crates/*/Cargo.toml; do
            grep -Eq '^edition *= *"2024"' "$m" || fail "C1: $m is not edition 2024 and the root sets no workspace edition"
        done
    fi
fi
[ -f Cargo.lock ] || fail "C1: root Cargo.lock is missing"
[ -f rust-toolchain.toml ] || fail "C1: root rust-toolchain.toml is missing"

for m in crates/*/Cargo.toml; do
    # The V2 core crates depend on no workspace crate (model) or on model only (engine).
    case "$m" in crates/common/Cargo.toml|crates/model/Cargo.toml|crates/engine/Cargo.toml) continue ;; esac
    grep -Eq '^netray-common *=.*path *= *"[./]*crates/common"|^netray-common *=.*path *= *"\.\./common"|^netray-common *= *\{ *workspace *= *true' "$m" \
        || fail "C1: $m does not depend on netray-common by path"
    grep -Eq '^netray-common *=.*version *=' "$m" && fail "C1: $m has a version dependency on netray-common"
done
if [ -f Cargo.toml ] && grep -Eq '^netray-common *= *\{ *workspace' crates/*/Cargo.toml 2>/dev/null; then
    grep -Eq '^netray-common *=.*path *= *"crates/common"' Cargo.toml || fail "C1: workspace netray-common is not a path dep on crates/common"
fi
for dep in axum tokio serde; do
    for m in crates/*/Cargo.toml; do
        # Only crates that use the dependency at all.
        grep -Eq "^$dep *=|^$dep\.workspace|^\[dependencies\.$dep\]" "$m" || continue
        grep -Eq "^$dep *=.*workspace *= *true|^$dep\.workspace *= *true" "$m" \
            || fail "C1: $m does not use workspace = true for $dep"
    done
done

# --- C6: cargo metadata ---
meta=$(cargo metadata --format-version 1 --no-deps --offline 2>&1)
if [ $? -ne 0 ]; then
    fail "C6: cargo metadata failed at the root: $(printf '%s' "$meta" | head -n1)"
else
    got=$(printf '%s' "$meta" | python3 -I -c '
import json, sys, os
m = json.load(sys.stdin)
root = m["workspace_root"]
ws = set(m["workspace_members"])
print("\n".join(sorted(os.path.relpath(p["manifest_path"], root) for p in m["packages"] if p["id"] in ws)))
')
    want=$(ls crates/*/Cargo.toml 2>/dev/null | sort)
    [ "$got" = "$want" ] || fail "C6: workspace members differ from crates/*/Cargo.toml"
    printf '%s\n' "$got" | grep -qx 'crates/common/Cargo.toml' || fail "C6: netray-common manifest is not crates/common/Cargo.toml"
    printf '%s' "$meta" | python3 -I -c '
import json, sys, os
m = json.load(sys.stdin)
r = m["workspace_root"]
sys.exit(0 if any(p["name"] == "netray-common" and os.path.relpath(p["manifest_path"], r) == "crates/common/Cargo.toml" for p in m["packages"]) else 1)
' || fail "C6: package netray-common not at crates/common/Cargo.toml"
fi

# --- C7: nothing outside the root ---
stray=$(git ls-files 2>/dev/null | grep -E '(^|/)(Cargo\.lock|rust-toolchain\.toml|justfile|Makefile)$' | grep -Ev '^(Cargo\.lock|rust-toolchain\.toml|justfile|Makefile)$')
if [ -n "$stray" ]; then
    fail "C7: $(printf '%s\n' "$stray" | wc -l | tr -d ' ') stray Cargo.lock/rust-toolchain.toml/justfile/Makefile outside root, e.g. $(printf '%s\n' "$stray" | head -n1)"
fi

if [ ${#fails[@]} -gt 0 ]; then
    printf 'FAIL: %s\n' "${fails[@]}" >&2
    exit 1
fi
echo "cargo workspace layout ok"
