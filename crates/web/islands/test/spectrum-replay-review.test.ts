// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { spectrum } from "../src/spectrum";

// spec v2-result-fidelity Phase 3, review findings 1 and 5: the hint follows pin and hover,
// the caption names a line only while a run is hovered or pinned.
const CAPTION = "Each line carries its trace: 3 runs, newest on top.";
const HINT = "hover a run to replay the spectrum · tap to pin";

const rows = [3, 2, 1]
  .map(
    (k, i) =>
      `<li class="trace__row" data-run="${i}" data-run-no="${k}" data-date="2027-01-1${k}" data-grade="B"></li>`,
  )
  .join("");
const unnumbered = `<li class="trace__row" data-run="0" data-date="2027-01-20" data-grade="B"></li><li class="trace__row" data-run="1" data-run-no="1" data-date="2027-01-11" data-grade="B"></li>`;

const html = (list: string): string => `<main>
<figure id="spectrum">
<span class="spectrum__target" data-target="http.hsts"></span>
<p class="trace__caption" data-rest="${CAPTION}">${CAPTION}</p>
<p class="trace__hint" data-rest="${HINT}">${HINT}</p>
<ol class="trace">${list}</ol>
<ul id="spectrum-sentences" hidden>
<li data-code="http.hsts" data-name="HSTS HTTP·21" data-sentence="Failing since 2027-01-11."></li>
<li data-code="http.csp" data-sentence="Passing."></li>
</ul>
</figure></main>`;

const figure = (): HTMLElement => document.getElementById("spectrum") as HTMLElement;
const row = (i: number): HTMLElement => document.querySelectorAll<HTMLElement>("li.trace__row")[i];
const target = (): HTMLElement => document.querySelector("[data-target]") as HTMLElement;
const text = (sel: string): string => document.querySelector(sel)?.textContent?.trim() ?? "";

function fire(el: Element, types: string[]): void {
  for (const t of types) el.dispatchEvent(new Event(t, { bubbles: t !== "pointerenter" && t !== "mouseenter" }));
}
const enter = (el: Element) => fire(el, ["pointerenter", "mouseenter", "pointerover", "mouseover"]);
const leave = (el: Element) => fire(el, ["pointerleave", "mouseleave", "pointerout", "mouseout"]);
const click = (el: Element) => el.dispatchEvent(new Event("click", { bubbles: true }));

function mount(list = rows): void {
  document.body.innerHTML = html(list);
  spectrum(figure());
}

beforeEach(() => {
  vi.stubGlobal("matchMedia", (q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} }));
  (window as unknown as { matchMedia: unknown }).matchMedia = (globalThis as { matchMedia: unknown }).matchMedia;
});
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("hint (finding 1)", () => {
  it("reads pinned after hover then click on the same row", () => {
    mount();
    enter(row(1));
    expect(text(".trace__hint")).toBe("replaying run 2 · move away to return");
    click(row(1));
    expect(text(".trace__hint")).toBe("pinned · tap again to release");
  });

  it("reads pinned once the pointer leaves the pinned row", () => {
    mount();
    enter(row(1));
    click(row(1));
    leave(row(1));
    expect(text(".trace__hint")).toBe("pinned · tap again to release");
  });

  it("names the other row while one is hovered over a pin, then returns to pinned", () => {
    mount();
    click(row(1));
    enter(row(2));
    expect(text(".trace__hint")).toBe("replaying run 1 · move away to return");
    leave(row(2));
    expect(text(".trace__hint")).toBe("pinned · tap again to release");
  });

  it("names the newest row when it is hovered over a pin", () => {
    mount();
    click(row(1));
    enter(row(0));
    expect(text(".trace__hint")).toBe("replaying run 3 · move away to return");
  });

  it("is back to its rest text after the pin is released and the pointer left", () => {
    mount();
    enter(row(1));
    click(row(1));
    click(row(1));
    leave(row(1));
    expect(text(".trace__hint")).toBe(HINT);
  });

  it("never prints an empty run number", () => {
    mount(unnumbered);
    click(row(1));
    enter(row(0));
    expect(text(".trace__hint")).toBe("replaying this run · move away to return");
  });
});

describe("caption (finding 5)", () => {
  it("rests while no run is hovered or pinned, whatever line is selected", () => {
    mount();
    enter(target());
    expect(text(".trace__caption")).toBe(CAPTION);
    expect(document.querySelector(".trace__caption")?.hasAttribute("data-active")).toBe(false);
  });

  it("names the line by data-name with its sentence while a run is hovered", () => {
    mount();
    enter(row(1));
    enter(target());
    expect(text(".trace__caption")).toBe("HSTS HTTP·21 · Failing since 2027-01-11.");
    expect(document.querySelector(".trace__caption")?.hasAttribute("data-active")).toBe(true);
  });

  it("falls back to the code without a data-name", () => {
    document.body.innerHTML = html(rows).replace('data-target="http.hsts"', 'data-target="http.csp"');
    spectrum(figure());
    enter(row(1));
    enter(target());
    expect(text(".trace__caption")).toBe("http.csp · Passing.");
  });

  it("forgets the line when the pointer leaves its target", () => {
    mount();
    enter(row(1));
    enter(target());
    leave(target());
    expect(text(".trace__caption")).toBe(CAPTION);
    expect(document.querySelector(".trace__caption")?.hasAttribute("data-active")).toBe(false);
  });

  it("does not name a once-hovered line after the run is left", () => {
    mount();
    enter(row(1));
    enter(target());
    leave(target());
    leave(row(1));
    enter(row(2));
    expect(text(".trace__caption")).toBe(CAPTION);
  });

  it("keeps a clicked line under a pin until the pin is released", () => {
    mount();
    click(row(1));
    click(target());
    leave(target());
    expect(text(".trace__caption")).toBe("HSTS HTTP·21 · Failing since 2027-01-11.");
    click(row(1));
    enter(row(1));
    expect(text(".trace__caption")).toBe(CAPTION);
  });

  it("shows the rest text again when the pin is released", () => {
    mount();
    click(row(1));
    click(target());
    click(row(1));
    expect(text(".trace__caption")).toBe(CAPTION);
  });
});
