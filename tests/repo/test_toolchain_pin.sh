#!/usr/bin/env bash
# One Rust toolchain version everywhere (spec release-toolchain-pin, requirements 1-6):
# rust-toolchain.toml channel, the Dockerfile builder tag, its RUSTUP_TOOLCHAIN, the CI
# `toolchain:` values and the two docs that name the version must agree. Offline, no docker.
# Prints one line per failed check; exit 1 if any failed.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=()
fail() { fails+=("FAIL: $1"); }

# --- channel (rust-toolchain.toml) ---
channel=""
if [ ! -f rust-toolchain.toml ]; then
    fail "rust-toolchain.toml: file is missing"
else
    raw=$(grep -E '^[[:space:]]*channel[[:space:]]*=' rust-toolchain.toml | head -n1 \
        | sed -E 's/^[^=]*=[[:space:]]*//; s/[[:space:]]*#.*$//; s/[[:space:]]+$//; s/^"(.*)"$/\1/; s/^'"'"'(.*)'"'"'$/\1/')
    if printf '%s\n' "$raw" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
        channel=$raw
    else
        fail "rust-toolchain.toml: channel '${raw}' is not an exact X.Y.Z version"
    fi
    grep -Eq '^[[:space:]]*components[[:space:]]*=[[:space:]]*\[[[:space:]]*"rustfmt"[[:space:]]*,[[:space:]]*"clippy"[[:space:]]*\]' rust-toolchain.toml \
        || fail "rust-toolchain.toml: components are not [\"rustfmt\", \"clippy\"]"
fi

# --- Dockerfile builder stage ---
if [ ! -f Dockerfile ]; then
    fail "Dockerfile: file is missing"
else
    from=$(awk 'tolower($0) ~ /^[[:space:]]*from[[:space:]].*[[:space:]]as[[:space:]]+builder[[:space:]]*$/ { print; exit }' Dockerfile)
    tag=$(printf '%s\n' "$from" | sed -nE 's/.*clux\/muslrust:([0-9]+\.[0-9]+\.[0-9]+).*/\1/p')
    if [ -z "$from" ]; then
        fail "Dockerfile: no FROM line ending in AS builder (channel ${channel:-unknown} in rust-toolchain.toml)"
    elif [ -z "$tag" ]; then
        fail "Dockerfile: builder FROM '$from' has no clux/muslrust:X.Y.Z version (rust-toolchain.toml channel ${channel:-unknown})"
    elif [ -n "$channel" ] && [ "$tag" != "$channel" ]; then
        fail "Dockerfile: builder tag version $tag differs from rust-toolchain.toml channel $channel"
    fi

    # ENV RUSTUP_TOOLCHAIN=stable inside the builder stage, before its first cargo line.
    env_ok=$(awk '
        BEGIN { in_b = 0; ok = 0 }
        { l = tolower($0) }
        l ~ /^[[:space:]]*from[[:space:]]/ { if (in_b) exit; if (l ~ /[[:space:]]as[[:space:]]+builder[[:space:]]*$/) in_b = 1; next }
        in_b {
            if ($0 ~ /^[[:space:]]*[Ee][Nn][Vv][[:space:]]+RUSTUP_TOOLCHAIN(=|[[:space:]]+)["'"'"']?stable["'"'"']?[[:space:]]*$/) { ok = 1; exit }
            if ($0 ~ /cargo/) exit
        }
        END { print ok }' Dockerfile)
    [ "$env_ok" = 1 ] || fail "Dockerfile: builder stage has no ENV RUSTUP_TOOLCHAIN=stable before its first cargo line (rust-toolchain.toml channel ${channel:-unknown} would be installed instead)"
fi

# --- CI toolchain values ---
if [ ! -f .github/workflows/ci.yml ]; then
    fail ".github/workflows/ci.yml: file is missing"
else
    vals=$(sed -nE 's/^[[:space:]]*(-[[:space:]]*)?toolchain:[[:space:]]*//p' .github/workflows/ci.yml \
        | sed -E 's/[[:space:]]*#.*$//; s/[[:space:]]+$//; s/^"(.*)"$/\1/; s/^'"'"'(.*)'"'"'$/\1/')
    if [ -z "$vals" ]; then
        fail ".github/workflows/ci.yml: no toolchain: value found (rust-toolchain.toml channel ${channel:-unknown})"
    else
        while IFS= read -r v; do
            [ "$v" = "$channel" ] || fail ".github/workflows/ci.yml: toolchain '$v' differs from rust-toolchain.toml channel ${channel:-unknown}"
        done <<<"$vals"
    fi
fi

# --- docs name the version ---
if [ -n "$channel" ]; then
    r=$(grep -F 'R-A2' specs/rules/workflow-rules.md 2>/dev/null | head -n1)
    case "$r" in *"$channel"*) ;; *) fail "specs/rules/workflow-rules.md: R-A2 line does not name channel $channel" ;; esac
    case "$r" in *'tests/repo/test_toolchain_pin.sh'*) ;; *) fail "specs/rules/workflow-rules.md: R-A2 line does not name tests/repo/test_toolchain_pin.sh" ;; esac
    case "$r" in *'`stable`'*) fail "specs/rules/workflow-rules.md: R-A2 line still says the toolchain is \`stable\` instead of $channel" ;; esac
    c=$(cat crates/common/CLAUDE.md 2>/dev/null)
    case "$c" in *"$channel"*) ;; *) fail "crates/common/CLAUDE.md: does not name channel $channel" ;; esac
    case "$c" in *'the toolchain is `stable`'*) fail "crates/common/CLAUDE.md: still says the toolchain is \`stable\` instead of $channel" ;; esac
else
    fail "specs/rules/workflow-rules.md, crates/common/CLAUDE.md: cannot check the named version without a valid rust-toolchain.toml channel"
fi

if [ "${#fails[@]}" -gt 0 ]; then
    printf '%s\n' "${fails[@]}"
    exit 1
fi
echo "PASS: toolchain pinned to $channel in rust-toolchain.toml, Dockerfile, ci.yml and docs"
