// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { spectrum } from "../src/spectrum";

// Markup is written by the Rust renderer (crates/web/tests/it/result_islands_trace_fixture.rs).
const trace = readFileSync(resolve(process.cwd(), "test/fixtures/result-trace.html"), "utf8");

function stubMatchMedia(phone: boolean): void {
  vi.stubGlobal("matchMedia", (q: string) => ({
    matches: phone && q === "(max-width: 480px)",
    media: q,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
  }));
  (window as unknown as { matchMedia: unknown }).matchMedia = (globalThis as { matchMedia: unknown }).matchMedia;
}

const figure = (): HTMLElement => document.getElementById("spectrum") as HTMLElement;
const row = (k: number): HTMLElement =>
  document.querySelector(`li.trace__row[data-run="${k}"]`) as HTMLElement;
const text = (el: Element | null): string => (el?.textContent ?? "").replace(/\s+/g, " ").trim();

function fire(el: Element, types: string[]): void {
  for (const t of types) el.dispatchEvent(new Event(t, { bubbles: t !== "pointerenter" && t !== "mouseenter" }));
}
const enter = (el: Element) => fire(el, ["pointerenter", "mouseenter", "pointerover", "mouseover"]);
const leave = (el: Element) => fire(el, ["pointerleave", "mouseleave", "pointerout", "mouseout"]);
const click = (el: Element) => (el as HTMLElement).click();

/** The runs whose groups or label sets carry data-shown, plus a check that no other run does. */
function shownRuns(): string[] {
  const all = [...figure().querySelectorAll(".spectrum__run[data-run], .spectrum__labels[data-run]")];
  const runs = new Set(all.map((e) => e.getAttribute("data-run") as string));
  const shown = new Set<string>();
  for (const r of runs) {
    const group = all.filter((e) => e.getAttribute("data-run") === r);
    const flags = group.map((e) => e.hasAttribute("data-shown"));
    expect(new Set(flags).size, `run ${r}: groups and labels agree`).toBe(1);
    if (flags[0]) shown.add(r);
  }
  return [...shown];
}

function mountIsland(phone = false): void {
  stubMatchMedia(phone);
  document.body.innerHTML = `<main>${trace}</main>`;
  spectrum(figure());
}

beforeEach(() => {
  document.body.innerHTML = "";
});
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("spectrum island (requirement 17)", () => {
  it("c2: hovering a trace row shows that run and the gutter; leaving restores run 0", () => {
    mountIsland();
    expect(shownRuns()).toEqual(["0"]);
    enter(row(1));
    expect(shownRuns()).toEqual(["1"]);
    const labels = figure().querySelectorAll('.spectrum__labels[data-run="1"] .spectrum__line-label');
    expect(labels.length).toBeGreaterThan(0);
    const gutter = text(document.getElementById("spectrum-gutter"));
    expect(gutter).toContain("Run 2");
    expect(gutter).toContain("2025-10-10");
    expect(gutter).toContain("grade B");
    leave(row(1));
    expect(shownRuns()).toEqual(["0"]);
  });

  it("c3: a click pins a run, hover is temporary over it, a second click releases", () => {
    mountIsland();
    click(row(1));
    expect(row(1).getAttribute("aria-pressed")).toBe("true");
    expect(shownRuns()).toEqual(["1"]);
    enter(row(2));
    expect(shownRuns()).toEqual(["2"]);
    leave(row(2));
    expect(shownRuns()).toEqual(["1"]);
    click(row(1));
    expect(row(1).getAttribute("aria-pressed")).toBe("false");
    expect(shownRuns()).toEqual(["0"]);
  });

  it("c4: at the phone width the stepper walks older runs and a tapped line sets the sentence", () => {
    mountIsland(true);
    const older = document.querySelector('#spectrum-stepper [data-step="1"]') as HTMLElement;
    const newer = document.querySelector('#spectrum-stepper [data-step="-1"]') as HTMLElement;
    click(older);
    expect(shownRuns()).toEqual(["1"]);
    click(older);
    expect(shownRuns()).toEqual(["2"]);
    click(older);
    expect(shownRuns()).toEqual(["2"]);
    click(newer);
    expect(shownRuns()).toEqual(["1"]);

    const target = figure().querySelector("[data-target]") as Element;
    const code = target.getAttribute("data-target") as string;
    const item = document.querySelector(`#spectrum-sentences li[data-code="${code}"]`) as HTMLElement;
    expect(item.getAttribute("data-sentence")).toBeTruthy();
    fire(target, ["pointerdown"]);
    click(target);
    expect(text(document.getElementById("spectrum-sentence"))).toBe(
      (item.getAttribute("data-sentence") as string).trim(),
    );
  });

  it("c5: interactions write only class, aria-* and data-* attributes and the two text slots", () => {
    mountIsland(true);
    const muts: MutationRecord[] = [];
    const observer = new MutationObserver((r) => muts.push(...r));
    observer.observe(figure(), {
      attributes: true,
      attributeOldValue: true,
      childList: true,
      characterData: true,
      subtree: true,
    });
    const older = document.querySelector('#spectrum-stepper [data-step="1"]') as HTMLElement;
    const target = figure().querySelector("[data-target]") as Element;
    click(older);
    click(target);
    enter(row(1));
    leave(row(1));
    click(row(2));
    click(row(2));
    muts.push(...observer.takeRecords());
    observer.disconnect();

    expect(muts.length).toBeGreaterThan(0);
    const slot = (n: Node): boolean => {
      const el = n instanceof Element ? n : n.parentElement;
      return !!el?.closest("#spectrum-gutter, #spectrum-sentence, .trace__caption, .trace__hint, #run-line");
    };
    for (const m of muts) {
      if (m.type === "attributes") {
        const name = m.attributeName as string;
        expect(name === "class" || name === "hidden" || name.startsWith("aria-") || name.startsWith("data-"), name).toBe(true);
      } else {
        expect(slot(m.target), `${m.type} on ${(m.target as Element).nodeName}`).toBe(true);
      }
    }
  });
});
