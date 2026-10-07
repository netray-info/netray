#!/usr/bin/env bash
# All workspace packages declare the same major of vite and of typescript.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

node -e '
const fs = require("fs");
const dirs = ["ifconfig-rs","mhost-prism","tlsight","spectra","beacon","lens"].map(c => `crates/${c}/frontend`)
  .concat(["packages/common-frontend"]);
let bad = false;
for (const tool of ["vite", "typescript"]) {
  const majors = {};
  for (const d of dirs) {
    const f = `${d}/package.json`;
    if (!fs.existsSync(f)) { console.log(`FAIL: ${f} missing`); process.exit(1); }
    const p = JSON.parse(fs.readFileSync(f, "utf8"));
    const range = (p.dependencies || {})[tool] ?? (p.devDependencies || {})[tool];
    if (range === undefined) continue;
    const m = String(range).match(/(\d+)/);
    if (!m) { console.log(`FAIL: ${f}: unparsable ${tool} range ${range}`); process.exit(1); }
    (majors[m[1]] ||= []).push(d);
  }
  if (Object.keys(majors).length > 1) {
    bad = true;
    console.log(`FAIL: ${tool} majors differ: ` + Object.entries(majors).map(([k, v]) => `${k} (${v.join(", ")})`).join("; "));
  }
}
if (bad) process.exit(1);
console.log("PASS: vite and typescript majors agree");
'
