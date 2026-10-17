#!/usr/bin/env bash
# crates/web/islands is a workspace package with a pinned, toolchain-aligned build, and the gate
# refuses a checkout whose islands bundle was not built.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
fail=0
bad() { echo "FAIL: $*"; fail=1; }

node -e '
const fs = require("fs");
const out = [];
const bad = m => out.push("FAIL: " + m);
const rd = f => fs.existsSync(f) ? JSON.parse(fs.readFileSync(f, "utf8")) : null;

const root = rd("package.json");
if (!(root.workspaces || []).includes("crates/web/islands")) bad("root package.json workspaces lacks crates/web/islands");

const tc = fs.readFileSync("tests/repo/test_frontend_toolchain_majors.sh", "utf8");
if (!tc.includes("crates/web/islands")) bad("test_frontend_toolchain_majors.sh dir list lacks crates/web/islands");

const p = rd("crates/web/islands/package.json");
if (!p) { bad("crates/web/islands/package.json missing"); }
else {
  const dev = p.devDependencies || {};
  if (p.private !== true) bad("islands package is not private: true");
  if (!/^\d+\.\d+\.\d+$/.test(dev.esbuild || "")) bad(`esbuild must be one exact version, got ${dev.esbuild}`);
  const ref = rd("crates/lens/frontend/package.json");
  const refRange = (ref.dependencies || {}).typescript ?? (ref.devDependencies || {}).typescript;
  const refMajor = String(refRange).match(/(\d+)/)[1];
  const m = String(dev.typescript || "").match(/(\d+)/);
  if (!m || m[1] !== refMajor) bad(`typescript major ${m && m[1]} != workspace major ${refMajor}`);
  if (!/^\^4(\.|$)/.test(dev.vitest || "")) bad(`vitest must be ^4, got ${dev.vitest}`);
  if (!/^\^20(\.|$)/.test(dev["happy-dom"] || "")) bad(`happy-dom must be ^20, got ${dev["happy-dom"]}`);
  const s = p.scripts || {};
  const b = s.build || "";
  if (!(/esbuild/.test(b) && /src\/main\.ts/.test(b) && /dist\/islands\.js/.test(b) && /--bundle/.test(b) && /--minify/.test(b) && /--format=esm/.test(b)))
    bad(`build script must run esbuild src/main.ts --bundle --minify --format=esm to dist/islands.js, got: ${b}`);
  const t = (s.test || "").split("&&").map(x => x.trim());
  const i = t.findIndex(x => /^tsc\s+--noEmit/.test(x)), j = t.findIndex(x => /^vitest\s+run/.test(x));
  if (i < 0 || j < 0 || i > j) bad(`test script must run tsc --noEmit && vitest run, got: ${s.test}`);
}
const vc = "crates/web/islands/vitest.config.ts";
if (!fs.existsSync(vc)) bad(vc + " missing");
else if (!/happy-dom/.test(fs.readFileSync(vc, "utf8"))) bad(vc + " does not name happy-dom");
out.forEach(l => console.log(l));
process.exit(out.length ? 1 : 0);
' || fail=1

if [ -n "$(git ls-files crates/web/islands/dist)" ]; then bad "crates/web/islands/dist is tracked"; fi
git check-ignore -q crates/web/islands/dist/islands.js || bad "crates/web/islands/dist/islands.js is not ignored"

# C10: check-frontend-dist names the islands bundle.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp justfile "$tmp/justfile"
for c in dns email http ip lens tls; do mkdir -p "$tmp/crates/$c/frontend/dist"; done
out=$(just --justfile "$tmp/justfile" --working-directory "$tmp" check-frontend-dist 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then bad "check-frontend-dist passes without crates/web/islands/dist"
elif ! grep -Eq 'crates/web/islands/dist|islands\.js' <<<"$out" || ! grep -q 'just adlc-setup' <<<"$out"; then
  bad "check-frontend-dist output must name crates/web/islands/dist and 'just adlc-setup': $out"
fi
mkdir -p "$tmp/crates/web/islands/dist"; : > "$tmp/crates/web/islands/dist/islands.js"
just --justfile "$tmp/justfile" --working-directory "$tmp" check-frontend-dist >/dev/null 2>&1 \
  || bad "check-frontend-dist fails although islands.js exists"

if [ "$fail" -ne 0 ]; then exit 1; fi
echo "PASS: islands package and its dist check"
