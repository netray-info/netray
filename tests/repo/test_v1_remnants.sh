#!/usr/bin/env bash
# specs/features/v2-v1-remnants Phase 3 (requirements 10 to 12): the dead V1 code is gone.
# netray-common has no `ecosystem` or `metrics` module and needs serde_json only for its tests;
# no module crate re-exports `RequestId` or another middleware item, or enables the `middleware`
# feature; the TLS inspection lives in `inspection.rs`, not in a
# `routes.rs` that holds no router.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=()

for f in crates/common/src/ecosystem.rs crates/common/src/metrics.rs crates/tls/src/routes.rs; do
    git ls-files --error-unmatch "$f" >/dev/null 2>&1 && fails+=("$f is still tracked")
done
grep -qE '^pub mod (ecosystem|metrics);' crates/common/src/lib.rs \
    && fails+=("crates/common/src/lib.rs still declares ecosystem or metrics")

meta=$(cargo metadata --format-version 1 --no-deps 2>/dev/null)
if [ -z "$meta" ]; then
    fails+=("cargo metadata produced nothing: the dependency checks did not run")
else
    deps=$(printf '%s' "$meta" | python3 -I -c '
import json, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "netray-common":
        print("common-serde_json" if any(d["name"] == "serde_json" and d["kind"] is None for d in p["dependencies"]) else "common-ok")
    if p["name"] in ("netray-dns", "netray-tls", "netray-http", "netray-email"):
        for d in p["dependencies"]:
            if d["name"] == "netray-common" and "middleware" in d["features"]:
                print(p["name"] + "-middleware")
')
    grep -q '^common-' <<<"$deps" || fails+=("cargo metadata lists no netray-common: the dependency checks did not run")
    grep -q '^common-serde_json$' <<<"$deps" && fails+=("netray-common has serde_json as a normal dependency")
    mw=$(grep -- '-middleware$' <<<"$deps" | tr '\n' ' ')
    [ -z "$mw" ] || fails+=("module crates enable netray-common's middleware feature: $mw")
fi

reexports=$(git ls-files 'crates/*/src/lib.rs' | xargs grep -nE 'pub use (netray_common::middleware|.*RequestId)' 2>/dev/null)
[ -z "$reexports" ] || fails+=("a crate re-exports netray_common::middleware items: $reexports")

git ls-files --error-unmatch crates/tls/src/inspection.rs >/dev/null 2>&1 \
    || fails+=("crates/tls/src/inspection.rs is not tracked")
grep -qxF 'pub mod inspection;' crates/tls/src/lib.rs \
    || fails+=("crates/tls/src/lib.rs does not declare pub mod inspection;")
grep -nE '#\[path|mod routes' crates/tls/src/lib.rs >/dev/null \
    && fails+=("crates/tls/src/lib.rs still declares a routes module or a #[path] attribute")
grep -qF '`src/inspection.rs`' crates/tls/CLAUDE.md || fails+=("crates/tls/CLAUDE.md does not name src/inspection.rs")
users=$(git ls-files 'crates/tls/*.rs' 'crates/tls/*.md' 'crates/web/*.rs' | grep -v '/CHANGELOG.md$' \
    | xargs grep -nE 'netray_tls::routes|crate::routes::|src/routes\.rs|`routes::plan' 2>/dev/null | head -3)
[ -z "$users" ] || fails+=("the old TLS module path is still named: $users")

if [ ${#fails[@]} -gt 0 ]; then
    printf 'FAIL: %s\n' "${fails[@]}"
    exit 1
fi
echo "PASS: test_v1_remnants"
