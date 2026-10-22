// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { recent } from "../src/recent";

const KEY = "netray.recent";
const A = "xn--bcher-kva.example";

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
});

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

const page = (domain: string, display?: string) => {
  const d = display === undefined ? "" : ` data-display="${display}"`;
  document.body.innerHTML =
    `<div id="result" class="result" data-state="fresh" data-domain="${domain}"${d} data-island="recent">` +
    `<div id="result-grade" data-grade="B" data-date="2026-10-18">B</div></div>`;
  recent(document.getElementById("result") as HTMLElement);
};
const stored = () => JSON.parse(localStorage.getItem(KEY) ?? "[]");
const mountList = () => {
  document.body.innerHTML = `<ul id="recent" data-island="recent" hidden></ul>`;
  const ul = document.getElementById("recent") as HTMLElement;
  recent(ul);
  return ul;
};

describe("recent island, IDN display", () => {
  it("c2: the result page stores the A-label as domain and the display form beside it", () => {
    page(A, "bücher.example");
    expect(stored()).toEqual([{ domain: A, display: "bücher.example", grade: "B", date: "2026-10-18" }]);
  });

  it("c6: the home list links the A-label and shows both forms", () => {
    page(A, "bücher.example");
    const a = mountList().querySelector("li a") as HTMLAnchorElement;
    expect(a.getAttribute("href")).toBe(`/${A}`);
    expect(a.textContent).toBe(`bücher.example (${A})`);
  });

  it("c6: an item without a display form lists the domain alone", () => {
    page("d.example.com");
    const a = mountList().querySelector("li a") as HTMLAnchorElement;
    expect(a.getAttribute("href")).toBe("/d.example.com");
    expect(a.textContent).toBe("d.example.com");
  });
});
