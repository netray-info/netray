#!/usr/bin/env bash
# specs/features/v2-v1-remnants Phase 3 (requirements 10 to 12): the dead V1 code is gone.
# netray-common has no `ecosystem` or `metrics` module and needs serde_json only for its tests;
# no module crate re-exports `RequestId`; the TLS inspection lives in `inspection.rs`, not in a
# `routes.rs` that holds no router.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=()

for f in crates/common/src/ecosystem.rs crates/common/src/metrics.rs crates/tls/src/routes.rs; do
    git ls-files --error-unmatch "$f" >/dev/null 2>&1 && fails+=("$f is still tracked")
done
grep -qE '^pub mod (ecosystem|metrics);' crates/common/src/lib.rs \
    && fails+=("crates/common/src/lib.rs still declares ecosystem or metrics")

deps=$(cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -I -c '
import json, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "netray-common":
        print(" ".join(d["name"] for d in p["dependencies"] if d["name"] == "serde_json" and d["kind"] is None))
')
[ -z "$deps" ] || fails+=("netray-common has serde_json as a normal dependency")

reexports=$(git ls-files 'crates/*/src/lib.rs' | xargs grep -nE 'pub use .*RequestId' 2>/dev/null)
[ -z "$reexports" ] || fails+=("a crate re-exports RequestId: $reexports")

git ls-files --error-unmatch crates/tls/src/inspection.rs >/dev/null 2>&1 \
    || fails+=("crates/tls/src/inspection.rs is not tracked")

if [ ${#fails[@]} -gt 0 ]; then
    printf 'FAIL: %s\n' "${fails[@]}"
    exit 1
fi
echo "PASS: test_v1_remnants"
