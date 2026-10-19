// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { recent } from "../src/recent";

const KEY = "netray.recent";
let fetchSpy: ReturnType<typeof vi.fn>;
let beaconSpy: ReturnType<typeof vi.fn>;
let added: [string, EventListenerOrEventListenerObject][] = [];
const realAdd = document.addEventListener.bind(document);

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

function useStorage(s: Storage) {
  vi.stubGlobal("localStorage", s);
  Object.defineProperty(window, "localStorage", { value: s, configurable: true });
}

function throwingStorage(): Storage {
  const boom = () => {
    throw new Error("storage denied");
  };
  return { getItem: boom, setItem: boom, removeItem: boom, clear: boom, key: boom, length: 0 } as unknown as Storage;
}

beforeEach(() => {
  useStorage(memoryStorage());
  fetchSpy = vi.fn();
  beaconSpy = vi.fn();
  vi.stubGlobal("fetch", fetchSpy);
  Object.defineProperty(navigator, "sendBeacon", { value: beaconSpy, configurable: true });
  added = [];
  document.addEventListener = ((type: string, fn: EventListenerOrEventListenerObject, o?: any) => {
    added.push([type, fn]);
    realAdd(type, fn, o);
  }) as typeof document.addEventListener;
});

afterEach(() => {
  for (const [t, f] of added) document.removeEventListener(t, f);
  document.addEventListener = realAdd;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

const page = (domain: string, grade = "B", date = "2026-10-18", state = "fresh") => {
  document.body.innerHTML =
    `<div id="result" class="result" data-state="${state}" data-domain="${domain}" data-island="recent">` +
    `<div id="result-grade" data-grade="${grade}" data-date="${date}">${grade}</div></div>`;
  const el = document.getElementById("result") as HTMLElement;
  recent(el);
  return el;
};
const stored = () => JSON.parse(localStorage.getItem(KEY) ?? "[]");
const silent = () => {
  expect(fetchSpy).not.toHaveBeenCalled();
  expect(beaconSpy).not.toHaveBeenCalled();
  expect(document.cookie).toBe("");
};

describe("recent island", () => {
  it("c2: a fresh page writes its entry", () => {
    page("d.example.com");
    expect(stored()).toEqual([{ domain: "d.example.com", grade: "B", date: "2026-10-18" }]);
    silent();
  });

  it("c2: a stale page writes only after netray:stream-end", () => {
    const el = page("d.example.com", "B", "2026-10-18", "stale");
    expect(localStorage.getItem(KEY)).toBeNull();
    const g = el.querySelector("#result-grade") as HTMLElement;
    g.setAttribute("data-grade", "A");
    document.dispatchEvent(new Event("netray:stream-end"));
    expect(stored()).toEqual([{ domain: "d.example.com", grade: "A", date: "2026-10-18" }]);
    silent();
  });

  it("c2: an incomplete grade is not written", () => {
    page("d.example.com", "incomplete");
    expect(localStorage.getItem(KEY)).toBeNull();
  });

  it("c2: the same domain moves to the front without duplication", () => {
    page("a.example.com", "A", "2026-10-16");
    page("b.example.com", "C", "2026-10-17");
    page("a.example.com", "B", "2026-10-18");
    expect(stored()).toEqual([
      { domain: "a.example.com", grade: "B", date: "2026-10-18" },
      { domain: "b.example.com", grade: "C", date: "2026-10-17" },
    ]);
  });

  it("c2: throwing storage neither throws nor shows the list", () => {
    useStorage(throwingStorage());
    expect(() => page("d.example.com")).not.toThrow();
    document.body.innerHTML = `<ul id="recent" data-island="recent" hidden></ul>`;
    const ul = document.getElementById("recent") as HTMLElement;
    expect(() => recent(ul)).not.toThrow();
    expect(ul.hasAttribute("hidden")).toBe(true);
    expect(ul.querySelectorAll("li").length).toBe(0);
    silent();
  });

  it("c5: the idle page lists the last eight domains, newest first", () => {
    for (let i = 1; i <= 9; i++) page(`s${i}.example.com`, "B", `2026-10-${String(9 + i).padStart(2, "0")}`);
    page("s2.example.com", "A", "2026-10-19");
    document.body.innerHTML = `<ul id="recent" data-island="recent" hidden></ul>`;
    const ul = document.getElementById("recent") as HTMLElement;
    recent(ul);
    const items = [...ul.querySelectorAll("li")];
    expect(items.length).toBe(8);
    expect(items.map((li) => li.querySelector("a")?.getAttribute("href"))).toEqual(
      ["s2", "s9", "s8", "s7", "s6", "s5", "s4", "s3"].map((s) => `/${s}.example.com`),
    );
    expect(items[0].textContent).toContain("s2.example.com");
    expect(items[0].textContent).toContain("A");
    expect(items[0].textContent).toContain("2026-10-19");
    expect(items[1].textContent).toContain("2026-10-18");
    expect(ul.hasAttribute("hidden")).toBe(false);
    silent();
  });
});
