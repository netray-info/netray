// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { dnsview } from "../src/dnsview";

// Spec v2-dns-ui, Phase 3, requirements 11 and 19 (C1, C6, C7, C8, C9, C13).
const KEY = "netray.dns.view";

function markup(): string {
  const toggle = (n: string) =>
    `<button type="button" class="dns-view__toggle" data-toggle="${n}" aria-pressed="false">${n}</button>`;
  const card = (order: number, rank: number) =>
    `<article class="dns-type" data-order="${order}" data-rank="${rank}"><details class="dns-record"><summary>r${order}</summary></details></article>`;
  return `<section class="dns-view" data-island="dnsview"><div class="dns-view__toolbar" role="toolbar" hidden>${[
    "hide-empty",
    "compact",
    "only-divergent",
    "sort-slowest",
    "explain",
  ]
    .map(toggle)
    .join("")}<button type="button" class="dns-view__action" data-action="expand">Expand all</button><button type="button" class="dns-view__action" data-action="collapse">Collapse all</button></div><div id="dns-results">${card(1, 3)}${card(2, 1)}${card(3, 2)}</div></section>`;
}

const root = (): HTMLElement => document.querySelector(".dns-view") as HTMLElement;
const btn = (sel: string): HTMLElement => document.querySelector(sel) as HTMLElement;
const toggle = (n: string): HTMLElement => btn(`[data-toggle="${n}"]`);
const action = (n: string): HTMLElement => btn(`[data-action="${n}"]`);
const click = (e: HTMLElement): boolean =>
  e.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
const orders = (): string[] =>
  [...document.querySelectorAll<HTMLElement>("#dns-results .dns-type")].map((c) => c.dataset.order ?? "");

function useStorage(s: unknown) {
  vi.stubGlobal("localStorage", s);
  Object.defineProperty(window, "localStorage", { value: s, configurable: true });
}

function memoryStorage(initial: Record<string, string> = {}): Storage {
  const m = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, String(v)),
    removeItem: (k: string) => void m.delete(k),
    clear: () => m.clear(),
    key: () => null,
    get length() {
      return m.size;
    },
  } as Storage;
}

describe("dnsview toolbar", () => {
  beforeEach(() => {
    useStorage(memoryStorage());
    document.body.innerHTML = markup();
  });

  it("C1/C6: unhides the toolbar and toggles hide-empty on and off without a style attribute", () => {
    dnsview(root());
    expect(btn(".dns-view__toolbar").hasAttribute("hidden")).toBe(false);

    click(toggle("hide-empty"));
    expect(root().hasAttribute("data-hide-empty")).toBe(true);
    expect(toggle("hide-empty").getAttribute("aria-pressed")).toBe("true");

    click(toggle("hide-empty"));
    expect(root().hasAttribute("data-hide-empty")).toBe(false);
    expect(toggle("hide-empty").getAttribute("aria-pressed")).toBe("false");
    expect(document.querySelectorAll("[style]").length).toBe(0);
  });

  it("C7: only-divergent sets data-only-divergent on the root", () => {
    dnsview(root());
    click(toggle("only-divergent"));
    expect(root().hasAttribute("data-only-divergent")).toBe(true);
  });

  it("C8: sort-slowest reorders the cards by rank and back by order", () => {
    dnsview(root());
    expect(orders()).toEqual(["1", "2", "3"]);
    click(toggle("sort-slowest"));
    expect(orders()).toEqual(["2", "3", "1"]);
    click(toggle("sort-slowest"));
    expect(orders()).toEqual(["1", "2", "3"]);
  });

  it("C9: restores stored state on mount and writes the JSON back on a click", () => {
    const s = memoryStorage({ [KEY]: '{"hide-empty":true,"explain":true}' });
    useStorage(s);
    dnsview(root());
    expect(root().hasAttribute("data-hide-empty")).toBe(true);
    expect(root().hasAttribute("data-explain")).toBe(true);
    expect(toggle("hide-empty").getAttribute("aria-pressed")).toBe("true");
    expect(toggle("explain").getAttribute("aria-pressed")).toBe("true");

    click(toggle("compact"));
    const saved = JSON.parse(s.getItem(KEY) ?? "{}");
    expect(saved["compact"]).toBe(true);
    expect(saved["hide-empty"]).toBe(true);
    expect(saved["explain"]).toBe(true);
  });

  it("C9: a storage that throws leaves defaults and clicks do not throw", () => {
    useStorage({
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    });
    expect(() => dnsview(root())).not.toThrow();
    expect(root().hasAttribute("data-hide-empty")).toBe(false);
    expect(btn(".dns-view__toolbar").hasAttribute("hidden")).toBe(false);
    expect(() => click(toggle("hide-empty"))).not.toThrow();
    expect(root().hasAttribute("data-hide-empty")).toBe(true);
  });

  it("C13: expand opens every record and collapse closes them", () => {
    dnsview(root());
    const recs = [...document.querySelectorAll<HTMLElement>("details.dns-record")];
    expect(recs.length).toBe(3);
    click(action("expand"));
    for (const r of recs) expect(r.hasAttribute("open")).toBe(true);
    click(action("collapse"));
    for (const r of recs) expect(r.hasAttribute("open")).toBe(false);
  });
});
