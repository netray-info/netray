#!/usr/bin/env bash
# specs/features/v2-cutover Phase 7 (requirements 6, 7, 8, 23): the V1 tree is gone. Nothing is
# tracked under crates/lens/, crates/*/frontend/, packages/ or site/; crates/netray/src/site.rs
# and v1.rs are absent; no manifest names solid-js or @netray-info/common-frontend; the root
# workspaces name only crates/web/islands; no module crate exposes `pub async fn run`; no tracked
# file outside specs/ and CHANGELOG.md names snapshots.db; the Dockerfile builds no Solid app,
# copies netray.example.toml and no site/, and runs `netray serve`; `just acceptance-local`
# starts one `netray serve` and no V1 subcommand.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=()

left=$(git ls-files 'crates/lens/' 'packages/' 'site/' | head -n 5)
[ -z "$left" ] || fails+=("still tracked: $left")
fe=$(git ls-files | grep -E '^crates/[^/]+/frontend/' | head -n 3)
[ -z "$fe" ] || fails+=("a crate frontend is still tracked: $fe")
for f in crates/netray/src/site.rs crates/netray/src/v1.rs; do
    git ls-files --error-unmatch "$f" >/dev/null 2>&1 && fails+=("$f is still tracked")
done

manifests=$(git ls-files | grep -E '(^|/)(package\.json|Cargo\.toml)$')
named=$(printf '%s\n' "$manifests" | xargs grep -lE 'solid-js|@netray-info/common-frontend' 2>/dev/null)
[ -z "$named" ] || fails+=("manifests name solid-js or common-frontend: $named")

ws=$(python3 -I -c 'import json; print(" ".join(json.load(open("package.json")).get("workspaces", [])))')
[ "$ws" = "crates/web/islands" ] || fails+=("root package.json workspaces are '$ws', want crates/web/islands")

runs=$(git ls-files 'crates/dns/src' 'crates/tls/src' 'crates/http/src' 'crates/email/src' 'crates/ip/src' \
    | xargs grep -nE '^\s*pub async fn run\b' 2>/dev/null)
[ -z "$runs" ] || fails+=("a module crate exposes pub async fn run: $runs")

snap=$(git ls-files | grep -vE '^(specs/|CHANGELOG\.md$)' | grep -vE '^(crates/netray/src/legacy\.rs|tests/repo/test_v1_deleted\.sh)$' \
    | xargs grep -lF 'snapshots.db' 2>/dev/null)
[ -z "$snap" ] || fails+=("files name snapshots.db: $snap")

df=Dockerfile
grep -q 'frontend/dist' "$df" && fails+=("Dockerfile still copies a frontend/dist")
grep -qE 'common-frontend|build:types' "$df" && fails+=("Dockerfile still builds common-frontend")
grep -qE '^COPY +site' "$df" && fails+=("Dockerfile still copies site/")
grep -q 'netray.example.toml' "$df" || fails+=("Dockerfile does not copy netray.example.toml")
grep -qE '^ENTRYPOINT \["netray", *"serve"' "$df" || fails+=("Dockerfile entrypoint does not run netray serve")

recipe=$(just --show acceptance-local 2>/dev/null)
serves=$(printf '%s\n' "$recipe" | grep -cE '"?\$?\{?NETRAY[A-Z_]*\}?"? serve|netray serve|[/ ]netray"? serve')
[ "$serves" -eq 1 ] || fails+=("acceptance-local starts netray serve $serves times, want 1")
printf '%s\n' "$recipe" | grep -qE '(netray|NETRAY[A-Z_]*\}?"?) (lens|dns|tls|http|email|ip|site)( |$)' \
    && fails+=("acceptance-local still starts a V1 subcommand")

if [ ${#fails[@]} -gt 0 ]; then
    printf 'FAIL: %s\n' "${fails[@]}"
    exit 1
fi
echo "PASS: test_v1_deleted"
