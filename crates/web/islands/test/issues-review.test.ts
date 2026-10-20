// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { issues } from "../src/issues";

const row = (order: number, severity: string, status: string, owner: string, label: string, protocol: string) =>
  `<div class="issue" data-owner="${owner}" data-owner-label="${label}" data-severity="${severity}" data-protocol="${protocol}" data-status="${status}" data-order="${order}"></div>`;

const page = `<section id="issues" data-island="issues">
<div class="grouping-switch">
<button type="button" class="grouping-switch__option" data-group="owner" aria-pressed="true">Owner</button>
<button type="button" class="grouping-switch__option" data-group="severity" aria-pressed="false">Severity</button>
<button type="button" class="grouping-switch__option" data-group="protocol" aria-pressed="false">Protocol</button>
</div>
<section class="issue-group" data-group="dns_provider">${row(1, "low", "fail", "dns_provider", "Your DNS provider", "dns")}</section>
<section class="issue-group" data-group="web_server">${row(2, "critical", "warn", "web_server", "Your web server configuration", "http")}${row(3, "high", "fail", "web_server", "Your web server configuration", "email")}</section>
</section>`;

const root = (): HTMLElement => document.getElementById("issues") as HTMLElement;
const option = (g: string): HTMLElement =>
  root().querySelector(`.grouping-switch__option[data-group="${g}"]`) as HTMLElement;
const groups = (): HTMLElement[] => [...root().querySelectorAll<HTMLElement>("section.issue-group")];
const keys = (): string[] => groups().map((g) => g.getAttribute("data-group") as string);
const labels = (): string[] => groups().map((g) => g.querySelector("h3")?.textContent ?? "");
const orders = (g: HTMLElement): string[] =>
  [...g.querySelectorAll(".issue")].map((r) => r.getAttribute("data-order") as string);

function memoryStorage(): Storage {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => (m.has(k) ? (m.get(k) as string) : null),
    setItem: (k: string, v: string) => void m.set(k, String(v)),
    removeItem: (k: string) => void m.delete(k),
    clear: () => m.clear(),
    key: (i: number) => [...m.keys()][i] ?? null,
    get length() {
      return m.size;
    },
  } as Storage;
}

beforeEach(() => {
  const s = memoryStorage();
  vi.stubGlobal("localStorage", s);
  Object.defineProperty(window, "localStorage", { value: s, configurable: true });
  document.body.innerHTML = page;
});
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("issues island regrouping follows the board", () => {
  it("lists fail rows first and orders groups by their first row in every mode", () => {
    issues(root());

    option("owner").click();
    expect(keys()).toEqual(["web_server", "dns_provider"]);
    expect(groups().map(orders)).toEqual([["3", "2"], ["1"]]);
    expect(labels()).toEqual(["Your web server configuration", "Your DNS provider"]);

    option("severity").click();
    expect(keys()).toEqual(["high", "low", "critical"]);
    expect(groups().map(orders)).toEqual([["3"], ["1"], ["2"]]);

    option("protocol").click();
    expect(keys()).toEqual(["email", "dns", "http"]);
    expect(groups().map(orders)).toEqual([["3"], ["1"], ["2"]]);
  });

  it("labels the email protocol group Email", () => {
    issues(root());
    option("protocol").click();
    expect(labels()).toEqual(["Email", "DNS", "HTTP"]);
  });

  it("marks the section ready on mount so the switch shows", () => {
    expect(root().hasAttribute("data-ready")).toBe(false);
    issues(root());
    expect(root().hasAttribute("data-ready")).toBe(true);
  });
});
