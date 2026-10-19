// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { spectrum } from "../src/spectrum";

// spec v2-result-fidelity Phase 3 (requirements 21-23): replaying a run from the trace. The
// markup mirrors the server's; the island copies data attributes and writes no styles.
const TITLE = "Run 12 · 2027-01-26 08:00 UTC · 1 checks";
const CAPTION = "Each line carries its trace: 12 runs, newest on top.";
const HINT = "hover a run to replay the spectrum · tap to pin";

const rowHtml = (k: number): string => {
  const day = String(14 + k).padStart(2, "0");
  return `<li class="trace__row" data-run="${12 - k}" data-run-no="${k}" data-date="2027-01-${day}" data-grade="${k === 5 ? "C" : "B"}"><span class="trace__date">2027-01-${day}</span><span class="trace__grade">${k === 5 ? "C" : "B"}</span></li>`;
};
const rows = Array.from({ length: 12 }, (_, i) => rowHtml(12 - i)).join("");

const HTML = `<main>
<div id="run-line"><p class="run-line run-line--title" data-rest="${TITLE}">${TITLE}</p></div>
<figure id="spectrum">
<p class="trace__caption" data-rest="${CAPTION}">${CAPTION}</p>
<p class="trace__hint" data-rest="${HINT}">${HINT}</p>
<div id="spectrum-gutter" class="spectrum-gutter"><span class="spectrum-gutter__run">Run 12</span><span class="spectrum-gutter__date">2027-01-26</span><span class="spectrum-gutter__grade">grade <strong>B</strong></span></div>
<ol class="trace">${rows}</ol>
</figure></main>`;

const figure = (): HTMLElement => document.getElementById("spectrum") as HTMLElement;
const row = (k: number): HTMLElement =>
  document.querySelector(`li.trace__row[data-run-no="${k}"]`) as HTMLElement;
const text = (sel: string): string => document.querySelector(sel)?.textContent?.trim() ?? "";
const gutter = (): HTMLElement => document.getElementById("spectrum-gutter") as HTMLElement;

function fire(el: Element, types: string[]): void {
  for (const t of types) el.dispatchEvent(new Event(t, { bubbles: t !== "pointerenter" && t !== "mouseenter" }));
}
const enter = (el: Element) => fire(el, ["pointerenter", "mouseenter", "pointerover", "mouseover"]);
const leave = (el: Element) => fire(el, ["pointerleave", "mouseleave", "pointerout", "mouseout"]);

beforeEach(() => {
  vi.stubGlobal("matchMedia", (q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} }));
  (window as unknown as { matchMedia: unknown }).matchMedia = (globalThis as { matchMedia: unknown }).matchMedia;
  document.body.innerHTML = HTML;
  spectrum(figure());
});
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("spectrum island replay", () => {
  it("C12 caption and hint show their rest text before any interaction", () => {
    expect(text(".trace__caption")).toBe(CAPTION);
    expect(text(".trace__hint")).toBe(HINT);
  });

  it("C13 hovering run 5 dims the others, fills the gutter, retitles the run and names the replay", () => {
    enter(row(5));
    expect(row(5).hasAttribute("data-hovered")).toBe(true);
    expect(row(5).hasAttribute("data-dimmed")).toBe(false);
    for (const k of [1, 2, 3, 4, 6, 7, 8, 9, 10, 11, 12]) {
      expect(row(k).hasAttribute("data-dimmed"), `row ${k}`).toBe(true);
      expect(row(k).hasAttribute("data-hovered"), `row ${k}`).toBe(false);
    }
    expect(text(".spectrum-gutter__run")).toBe("Run 5");
    expect(text(".spectrum-gutter__date")).toBe("2027-01-19");
    expect(text(".spectrum-gutter__grade")).toBe("grade C");
    expect(gutter().hasAttribute("data-replaying")).toBe(true);
    expect(text("#run-line .run-line")).toBe("Run 5 · 2027-01-19 · replaying history");
    expect(text(".trace__hint")).toBe("replaying run 5 · move away to return");
  });

  it("C13 leaving restores caption, hint, title, gutter and row marks", () => {
    enter(row(5));
    leave(row(5));
    expect(text(".trace__caption")).toBe(CAPTION);
    expect(text(".trace__hint")).toBe(HINT);
    expect(text("#run-line .run-line")).toBe(TITLE);
    expect(text(".spectrum-gutter__run")).toBe("Run 12");
    expect(text(".spectrum-gutter__date")).toBe("2027-01-26");
    expect(text(".spectrum-gutter__grade")).toBe("grade B");
    expect(gutter().hasAttribute("data-replaying")).toBe(false);
    expect(document.querySelectorAll("[data-dimmed], [data-hovered]").length).toBe(0);
  });

  it("C14 a click pins the run and a second click releases it", () => {
    row(5).dispatchEvent(new Event("click", { bubbles: true }));
    expect(text(".trace__hint")).toBe("pinned · tap again to release");
    row(5).dispatchEvent(new Event("click", { bubbles: true }));
    expect(text(".trace__hint")).toBe(HINT);
  });

  it("C4 the island writes no style attribute", () => {
    enter(row(5));
    row(5).dispatchEvent(new Event("click", { bubbles: true }));
    leave(row(5));
    expect(document.querySelectorAll("[style]").length).toBe(0);
    // the replay must have happened for the absence to mean anything
    expect(row(5).hasAttribute("data-hovered") || text(".trace__hint").startsWith("pinned")).toBe(true);
  });
});
