// @vitest-environment node
import { execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { describe, expect, it } from "vitest";

const BUDGET = 30_720;
const pkgDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// The one budget check: gzip (default level) of a bundle must stay under BUDGET.
function checkBudget(bundlePath: string): void {
  const size = gzipSync(readFileSync(bundlePath)).length;
  if (size >= BUDGET) {
    throw new Error(`gzipped bundle is ${size} bytes, budget is under ${BUDGET} bytes`);
  }
}

describe("islands bundle budget", () => {
  it("the built dist/islands.js is under 30720 gzipped bytes", () => {
    execFileSync("npm", ["run", "build"], { cwd: pkgDir, stdio: "pipe" });
    expect(() => checkBudget(join(pkgDir, "dist/islands.js"))).not.toThrow();
  }, 30_000);

  it("a planted 40 KB incompressible module fails the budget naming its size", async () => {
    const { build } = await import("esbuild");
    const dir = mkdtempSync(join(tmpdir(), "islands-budget-"));
    const entry = join(dir, "entry.js");
    const out = join(dir, "out.js");
    const blob = randomBytes(40_000).toString("base64");
    writeFileSync(entry, `export const blob = ${JSON.stringify(blob)};\n`);
    await build({ entryPoints: [entry], bundle: true, minify: true, format: "esm", outfile: out, write: true });
    expect(() => checkBudget(out)).toThrow(/\d+ bytes/);
  }, 30_000);
});
