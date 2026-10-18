// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { issues } from "../src/issues";

// Markup is written by the Rust renderer (crates/web/tests/result_islands_*_fixture.rs).
const page = readFileSync(resolve(process.cwd(), "test/fixtures/result-issues.html"), "utf8");

const root = (): HTMLElement => document.getElementById("issues") as HTMLElement;
const option = (g: string): HTMLElement =>
  root().querySelector(`.grouping-switch__option[data-group="${g}"]`) as HTMLElement;
const rowOf = (order: string): HTMLElement =>
  root().querySelector(`.issue[data-order="${order}"]`) as HTMLElement;
const groups = (): HTMLElement[] => [...root().querySelectorAll<HTMLElement>("section.issue-group")];
const keys = (): string[] => groups().map((g) => g.getAttribute("data-group") as string);
const orders = (g: HTMLElement): string[] =>
  [...g.querySelectorAll(".issue")].map((r) => r.getAttribute("data-order") as string);
const pressed = (): string[] =>
  [...root().querySelectorAll(".grouping-switch__option")].map(
    (b) => `${b.getAttribute("data-group")}=${b.getAttribute("aria-pressed")}`,
  );

/** Render the fixture; the owner label stays on each row as production will write it. */
function mount(): void {
  document.body.innerHTML = page;
  for (const g of document.querySelectorAll("#issues section.issue-group")) {
    const label = g.querySelector("h3")?.textContent?.trim() ?? "";
    for (const r of g.querySelectorAll(".issue")) {
      if (!r.hasAttribute("data-owner-label")) r.setAttribute("data-owner-label", label);
    }
  }
  issues(root());
}

let fetchSpy: ReturnType<typeof vi.fn>;
beforeEach(() => {
  localStorage.clear();
  fetchSpy = vi.fn();
  vi.stubGlobal("fetch", fetchSpy);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  localStorage.clear();
  document.body.innerHTML = "";
});

describe("issues island (requirement 10)", () => {
  it("reveals the switch and regroups the same rows in place", () => {
    mount();
    expect(option("owner").closest("[hidden]")).toBeNull();
    const rows = ["4", "44", "11", "32", "53"].map(rowOf);

    option("protocol").click();
    expect(keys()).toEqual(["dns", "tls", "http", "email", "ip"]);
    expect(groups().map((g) => g.querySelector("h3")?.textContent?.trim())).toEqual([
      "DNS", "TLS", "HTTP", "email", "IP",
    ]);
    expect(rowOf("4")).toBe(rows[0]);
    expect(rowOf("53")).toBe(rows[4]);
    expect(rows.every((r) => root().contains(r))).toBe(true);
    expect(pressed()).toEqual(["owner=false", "severity=false", "protocol=true"]);

    option("severity").click();
    expect(keys()).toEqual(["critical", "high", "medium"]);
    expect(groups().map((g) => g.querySelector("h3")?.textContent?.trim())).toEqual([
      "critical", "high", "medium",
    ]);
    expect(groups().map(orders)).toEqual([["11"], ["53", "32"], ["44", "4"]]);
    expect(pressed()).toEqual(["owner=false", "severity=true", "protocol=false"]);
    expect(rowOf("44")).toBe(rows[1]);

    option("owner").click();
    expect(keys()).toEqual(["dns_provider", "web_server", "hosting"]);
    expect(groups().map((g) => g.querySelector("h3")?.textContent?.trim())).toEqual([
      "your DNS provider", "your web server configuration", "your hosting provider",
    ]);
    expect(groups().map(orders)).toEqual([["44", "4"], ["11", "32"], ["53"]]);
    expect(pressed()).toEqual(["owner=true", "severity=false", "protocol=false"]);
    expect(rowOf("11")).toBe(rows[2]);
  });

  it("severity regroups without a request, is stored, and applies after a reload", () => {
    mount();
    expect(keys()).toEqual(["dns_provider", "web_server", "hosting"]);
    option("severity").click();
    expect(keys()).toEqual(["critical", "high", "medium"]);
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(localStorage.getItem("netray.grouping")).toBe("severity");

    document.body.innerHTML = "";
    mount();
    expect(keys()).toEqual(["critical", "high", "medium"]);
    expect(pressed()).toEqual(["owner=false", "severity=true", "protocol=false"]);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("keeps the default grouping and never throws when storage throws", () => {
    const boom = () => {
      throw new Error("storage denied");
    };
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(boom);
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(boom);

    expect(() => mount()).not.toThrow();
    expect(keys()).toEqual(["dns_provider", "web_server", "hosting"]);
    expect(() => option("severity").click()).not.toThrow();
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});
