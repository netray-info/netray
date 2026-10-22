#!/usr/bin/env bash
# The one npm workspace package, crates/web/islands, declares a typescript major and no vite
# (the Solid apps that used vite are gone, v2-cutover Phase 7).
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

node -e '
const fs = require("fs");
const f = "crates/web/islands/package.json";
if (!fs.existsSync(f)) { console.log(`FAIL: ${f} missing`); process.exit(1); }
const p = JSON.parse(fs.readFileSync(f, "utf8"));
const deps = { ...(p.dependencies || {}), ...(p.devDependencies || {}) };
if (!/(\d+)/.test(String(deps.typescript || ""))) { console.log(`FAIL: ${f}: no typescript major`); process.exit(1); }
if (deps.vite !== undefined) { console.log(`FAIL: ${f}: names vite`); process.exit(1); }
console.log("PASS: islands toolchain");
'
