// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { spectrum } from "../src/spectrum";

// Phase 6 review: a tapped line writes "<label> <code> · <history>" into the stepper hint.
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
const hint = (): HTMLElement => document.querySelector(".spectrum-stepper__hint") as HTMLElement;
const target = (code: string): HTMLElement =>
  figure().querySelector(`[data-target="${code}"]`) as HTMLElement;
const sentence = (code: string): HTMLElement =>
  document.querySelector(`#spectrum-sentences li[data-code="${code}"]`) as HTMLElement;
const lineText = (code: string): string =>
  `${sentence(code).getAttribute("data-name")} · ${sentence(code).getAttribute("data-sentence")}`;

function mountIsland(phone: boolean): void {
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

describe("spectrum island phone hint (review finding 6)", () => {
  it("a tapped line names itself in the hint and marks it active", () => {
    mountIsland(true);
    const rest = hint().textContent;
    expect(rest).toContain("Tap a line");
    target("dns.ns").click();
    expect(hint().textContent).toBe(lineText("dns.ns"));
    expect(hint().textContent).toMatch(/^NS DNS·05 · /);
    expect(hint().hasAttribute("data-active")).toBe(true);
    expect(hint().getAttribute("data-rest")).toBe(rest);
  });

  it("another tap replaces the text, a second tap on the same line restores the rest text", () => {
    mountIsland(true);
    const rest = hint().textContent;
    target("dns.ns").click();
    target("dns.caa").click();
    expect(hint().textContent).toBe(lineText("dns.caa"));
    target("dns.caa").click();
    expect(hint().textContent).toBe(rest);
    expect(hint().hasAttribute("data-active")).toBe(false);
  });

  it("stepping back to the latest run releases the line and restores the rest text", () => {
    mountIsland(true);
    const rest = hint().textContent;
    (document.querySelector('#spectrum-stepper [data-step="1"]') as HTMLElement).click();
    target("dns.ns").click();
    expect(hint().textContent).toBe(lineText("dns.ns"));
    (document.querySelector('#spectrum-stepper [data-step="-1"]') as HTMLElement).click();
    expect(hint().textContent).toBe(rest);
    expect(hint().hasAttribute("data-active")).toBe(false);
  });

  it("above phone width the hint is left alone", () => {
    mountIsland(false);
    const rest = hint().textContent;
    target("dns.ns").click();
    expect(hint().textContent).toBe(rest);
    expect(hint().hasAttribute("data-active")).toBe(false);
  });
});
