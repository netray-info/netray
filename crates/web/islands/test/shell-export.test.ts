// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Spec v2-inspector-shell, requirement 20: export controls (Copy Markdown, CSV, JSON), island half.
const MARKDOWN = "# example.com:443\n\n## Checks\n";
const CSV = "section,item,field,value\nchecks,tls.chain,status,pass\n";
const JSON_TEXT = '{"target":"example.com:443"}';

const slot = (disabled: boolean, templates: boolean): string =>
  '<div class="inspector-export" id="inspect-export" data-filename="netray-tls-example.com-443">' +
  `<button type="button" class="inspector-export__copy" data-export-action="markdown"${disabled ? " disabled" : ""} hidden>Copy Markdown</button>` +
  `<button type="button" class="inspector-export__download" data-export-action="csv"${disabled ? " disabled" : ""} hidden>CSV</button>` +
  `<button type="button" class="inspector-export__download" data-export-action="json"${disabled ? " disabled" : ""} hidden>JSON</button>` +
  '<span class="inspector-export__status" role="status" aria-live="polite"></span>' +
  (templates
    ? `<template data-export="markdown">${MARKDOWN.replace(/\n/g, "&#10;")}</template>` +
      `<template data-export="csv">${CSV.replace(/\n/g, "&#10;")}</template>` +
      `<template data-export="json">${JSON_TEXT.replace(/"/g, "&quot;")}</template>`
    : "") +
  "</div>";

const MARKUP =
  '<div class="inspector-shell" data-island="shell" data-tool="tls" data-field="h">' +
  '<form class="query-input query-input--card" action="/inspect/tls" method="get">' +
  '<label class="query-input__label" for="query-input-h">Host</label>' +
  '<input class="query-input__field" id="query-input-h" name="h">' +
  '<button class="query-input__submit" type="submit">Inspect</button></form>' +
  `<div id="inspect-result"><div class="inspector-run__head">${slot(true, false)}</div></div></div>`;

const btn = (kind: string): HTMLButtonElement =>
  document.querySelector(`[data-export-action="${kind}"]`) as HTMLButtonElement;
const status = (): string => (document.querySelector(".inspector-export__status") as HTMLElement).textContent ?? "";
const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

function finalSlot(): void {
  const head = document.querySelector(".inspector-run__head") as HTMLElement;
  head.innerHTML = slot(false, true);
  head.firstElementChild!.dispatchEvent(new CustomEvent("netray:stream-fragment", { bubbles: true }));
}

let writeText: ReturnType<typeof vi.fn>;
let createObjectURL: ReturnType<typeof vi.fn>;
let downloads: string[];

beforeEach(() => {
  history.replaceState(null, "", "/inspect/tls");
  document.body.innerHTML = MARKUP;
  vi.stubGlobal("fetch", vi.fn(() => new Promise(() => {})));
  writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  createObjectURL = vi.fn(() => "blob:test");
  (URL as unknown as Record<string, unknown>).createObjectURL = createObjectURL;
  (URL as unknown as Record<string, unknown>).revokeObjectURL = vi.fn();
  downloads = [];
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) {
    downloads.push(this.download);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("shell export", () => {
  it("unhides the buttons at mount and ignores clicks while disabled", async () => {
    mount(document);
    for (const k of ["markdown", "csv", "json"]) {
      expect(btn(k).hidden).toBe(false);
      expect(btn(k).disabled).toBe(true);
    }
    btn("markdown").click();
    await flush();
    expect(writeText).not.toHaveBeenCalled();
  });

  it("copies the markdown template's text and announces it", async () => {
    mount(document);
    finalSlot();
    expect(btn("markdown").hidden).toBe(false);
    btn("markdown").click();
    await flush();
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText.mock.calls[0][0]).toBe(MARKDOWN);
    expect(status()).toContain("Copied Markdown");
  });

  it.each([
    ["json", "netray-tls-example.com-443.json", "application/json"],
    ["csv", "netray-tls-example.com-443.csv", "text/csv"],
  ])("downloads %s as a named Blob and announces it", async (kind, name, type) => {
    mount(document);
    finalSlot();
    btn(kind).click();
    await flush();
    expect(createObjectURL).toHaveBeenCalledTimes(1);
    const blob = createObjectURL.mock.calls[0][0] as Blob;
    expect(blob.type).toBe(type);
    expect(await blob.text()).toBe(kind === "json" ? JSON_TEXT : CSV);
    expect(downloads).toEqual([name]);
    expect(status()).not.toBe("");
  });
});
