// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { spectrum } from "../src/spectrum";

// Phase 5 review: the trace marker (aria-current) follows the run on display.
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
const current = (): string[] =>
  [...document.querySelectorAll("li.trace__row")]
    .filter((r) => r.hasAttribute("aria-current"))
    .map((r) => r.getAttribute("data-run") as string);

function fire(el: Element, types: string[]): void {
  for (const t of types) el.dispatchEvent(new Event(t, { bubbles: t !== "pointerenter" && t !== "mouseenter" }));
}
const enter = (el: Element) => fire(el, ["pointerenter", "mouseenter", "pointerover", "mouseover"]);
const leave = (el: Element) => fire(el, ["pointerleave", "mouseleave", "pointerout", "mouseout"]);

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

describe("spectrum island trace marker", () => {
  it("hover moves aria-current to the hovered row and leaving restores row 0", () => {
    mountIsland();
    expect(current()).toEqual(["0"]);
    enter(row(1));
    expect(current()).toEqual(["1"]);
    expect(row(0).hasAttribute("aria-current")).toBe(false);
    leave(row(1));
    expect(current()).toEqual(["0"]);
  });

  it("a pinned row carries the marker alone once the pointer leaves", () => {
    mountIsland();
    enter(row(2));
    row(2).click();
    leave(row(2));
    expect(current()).toEqual(["2"]);
  });

  it("at the phone width stepping once marks row 1 only", () => {
    mountIsland(true);
    (document.querySelector('#spectrum-stepper [data-step="1"]') as HTMLElement).click();
    expect(current()).toEqual(["1"]);
  });
});
