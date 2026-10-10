#!/usr/bin/env bash
# One npm workspace at the repo root: shared lockfile, no GitHub Packages auth,
# common-frontend linked from the workspace.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail() { echo "FAIL: $1"; exit 1; }

[ -f package.json ] || fail "root package.json missing"

# 1. workspaces cover all six frontends and common-frontend (globs resolved)
node -e '
const fs = require("fs"), path = require("path");
const pkg = JSON.parse(fs.readFileSync("package.json", "utf8"));
let ws = pkg.workspaces;
if (ws && !Array.isArray(ws)) ws = ws.packages;
if (!Array.isArray(ws) || ws.length === 0) { console.log("root package.json has no workspaces"); process.exit(1); }
function expand(pattern) {
  let bases = ["."];
  for (const seg of pattern.replace(/\/$/, "").split("/")) {
    const next = [];
    for (const b of bases) {
      if (seg === "*") {
        if (!fs.existsSync(b)) continue;
        for (const e of fs.readdirSync(b, { withFileTypes: true }))
          if (e.isDirectory() && e.name !== "node_modules") next.push(path.join(b, e.name));
      } else next.push(path.join(b, seg));
    }
    bases = next;
  }
  return bases.map(p => path.normalize(p));
}
const resolved = new Set(ws.flatMap(expand));
const want = ["ip","mhost-prism","tls","http","email","lens"].map(c => `crates/${c}/frontend`)
  .concat(["packages/common-frontend"]);
const missing = want.filter(w => !resolved.has(w));
if (missing.length) { console.log("workspaces miss: " + missing.join(", ")); process.exit(1); }
' || fail "workspaces do not cover all frontends and packages/common-frontend"

# 2. no GitHub Packages registry or auth token in tracked files
while IFS= read -r f; do
  [ -f "$f" ] || continue
  grep -qE 'npm\.pkg\.github\.com|_authToken' "$f" && fail "$f references npm.pkg.github.com or _authToken"
done < <(git ls-files '.npmrc' '*/.npmrc')
while IFS= read -r f; do
  [ -f "$f" ] || continue
  grep -q 'npm\.pkg\.github\.com' "$f" && fail "$f mentions npm.pkg.github.com"
done < <(git ls-files 'package.json' '*/package.json')

# 3. a single lockfile for the workspace
[ -f package-lock.json ] || fail "root package-lock.json missing"
extra=$(git ls-files 'crates/*package-lock.json' 'packages/*package-lock.json')
[ -z "$extra" ] || fail "per-package lockfile(s) still tracked: $(echo "$extra" | tr '\n' ' ')"

# 4. common-frontend resolves to the workspace link in the lockfile
node -e '
const lock = JSON.parse(require("fs").readFileSync("package-lock.json", "utf8"));
const e = (lock.packages || {})["node_modules/@netray-info/common-frontend"];
if (!e) { console.log("lockfile has no node_modules/@netray-info/common-frontend"); process.exit(1); }
if (e.link !== true || e.resolved !== "packages/common-frontend") {
  console.log("common-frontend not linked to packages/common-frontend: " + JSON.stringify(e)); process.exit(1);
}
' || fail "lockfile does not link @netray-info/common-frontend to the workspace"

echo "PASS: npm workspaces"
