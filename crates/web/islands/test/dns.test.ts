// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { beforeEach, describe, expect, it } from "vitest";
import { buildQuery, dns } from "../src/dns";

const here = dirname(fileURLToPath(import.meta.url));

describe("buildQuery", () => {
  it.each([
    ["example.com", ["A", "MX"], null, "dnssec", "example.com A MX +dnssec"],
    ["example.com", [], "cloudflare", "trace", "example.com @cloudflare +trace"],
    ["example.com", ["TXT"], null, "query", "example.com TXT"],
    ["example.com", [], null, null, "example.com"],
  ])("buildQuery(%s, %j, %s, %s) = %s", (domain, types, resolver, job, want) => {
    expect(buildQuery(domain, types as string[], resolver as string | null, job as string | null)).toBe(want);
  });
});

function page(): { picker: HTMLElement; q: HTMLInputElement } {
  document.body.innerHTML = `
    <form id="f">
      <input name="q" value="example.com">
      <div data-island="dns" id="picker">
        <input type="checkbox" name="dns-type" value="A">
        <input type="checkbox" name="dns-type" value="MX">
        <input type="checkbox" name="dns-type" value="TXT">
        <input type="radio" name="dns-job" value="query" checked>
        <input type="radio" name="dns-job" value="trace">
        <input type="radio" name="dns-job" value="dnssec">
        <input type="radio" name="dns-job" value="compare">
        <input type="radio" name="dns-job" value="auth">
        <select name="dns-resolver"><option value="" selected></option><option value="cloudflare">cloudflare</option></select>
      </div>
    </form>`;
  return {
    picker: document.getElementById("picker") as HTMLElement,
    q: document.querySelector("input[name=q]") as HTMLInputElement,
  };
}

describe("dns island", () => {
  beforeEach(() => {
    document.body.replaceChildren();
  });

  it("writes the built query into q on change, without style writes or other changes", () => {
    const { picker, q } = page();
    dns(picker);
    const before = document.body.innerHTML;

    for (const v of ["A", "MX"]) {
      const box = picker.querySelector(`input[name=dns-type][value=${v}]`) as HTMLInputElement;
      box.checked = true;
      box.dispatchEvent(new Event("change", { bubbles: true }));
    }
    const job = picker.querySelector("input[name=dns-job][value=dnssec]") as HTMLInputElement;
    job.checked = true;
    job.dispatchEvent(new Event("change", { bubbles: true }));

    expect(q.value).toBe("example.com A MX +dnssec");
    for (const e of document.querySelectorAll("*")) expect(e.getAttribute("style")).toBeNull();

    // nothing but the q value moved: markup other than live properties is identical
    expect(document.body.innerHTML).toBe(before);
    expect((picker.querySelector("select") as HTMLSelectElement).value).toBe("");
  });

  it("is registered in main.ts", () => {
    const src = readFileSync(resolve(here, "../src/main.ts"), "utf8");
    expect(src).toContain('register("dns"');
  });
});
