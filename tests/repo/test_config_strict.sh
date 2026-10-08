#!/usr/bin/env bash
# Every config struct rejects unknown keys (specs/features/monorepo-p3, Phase 1, R2).
# Scope: (a) every struct deriving Deserialize in crates/*/src/config.rs, (b) every struct
# named *Config deriving Deserialize anywhere under crates/*/src/ (netray-common's shared
# BackendConfig, EcosystemConfig, TelemetryConfig). Enums are out of scope; #[cfg(test)]
# modules are skipped.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fails=0
fail() { echo "FAIL: $1"; fails=1; }

command -v python3 >/dev/null || { echo "FAIL: python3 missing"; exit 1; }
ls crates/*/src/config.rs >/dev/null 2>&1 || { echo "FAIL: no crates/*/src/config.rs found"; exit 1; }

offenders=$(python3 -I - <<'PY'
import glob, os, re

def scan(path, in_scope_all):
    lines = open(path, encoding="utf-8").read().split("\n")
    out = []
    i = 0
    while i < len(lines):
        s = lines[i].strip()
        if s.startswith("#[cfg(test)]"):
            break  # test module(s) trail the file
        if s.startswith("#[derive(") :
            j = i
            derive = s
            while ")]" not in derive and j + 1 < len(lines):
                j += 1
                derive += lines[j].strip()
            if "Deserialize" in derive:
                attrs = [derive]
                k = j + 1
                while k < len(lines):
                    t = lines[k].strip()
                    m = re.match(r"(?:pub(?:\([^)]*\))?\s+)?(struct|enum)\s+(\w+)", t)
                    if m:
                        kind, name = m.groups()
                        if kind == "struct" and (in_scope_all or name.endswith("Config")):
                            if not any("deny_unknown_fields" in a for a in attrs):
                                out.append(f"{path}:{i+1} {name}")
                        break
                    attrs.append(t)
                    k += 1
            i = j
        i += 1
    return out

res = []
for p in sorted(glob.glob("crates/*/src/**/*.rs", recursive=True)):
    res += scan(p, os.path.basename(p) == "config.rs" and os.path.dirname(p).endswith("/src"))
print("\n".join(res))
PY
)

if [ -n "$offenders" ]; then
    while IFS= read -r line; do
        fail "missing #[serde(deny_unknown_fields)]: $line"
    done <<< "$offenders"
fi

[ "$fails" -eq 0 ] || exit 1
echo "PASS: every config struct rejects unknown keys"
