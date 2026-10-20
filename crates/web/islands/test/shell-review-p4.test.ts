// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Review findings on v2-inspector-shell Phase 4: j steps from the item that holds focus, also when
// focus sits on a link inside it; r re-runs a shown run of an empty IP input; without
// HTMLDialogElement the help still opens, closes and gives focus back.
function build(tool: string, field: string, value: string, run: boolean): void {
  document.body.innerHTML =
    `<div class="inspector-shell" data-island="shell" data-tool="${tool}" data-field="${field}">` +
    `<nav class="inspector-tabs"><a class="inspector-tabs__tab" href="/inspect/${tool}" aria-current="page">X</a>` +
    `<button type="button" class="inspector-help__open" hidden>?</button></nav>` +
    `<form class="query-input query-input--card" action="/inspect/${tool}" method="get">` +
    `<input class="query-input__field" name="${field}" value="${value}"><button type="submit">Inspect</button></form>` +
    `<div id="inspect-result">${run ? '<div class="inspector-run__head"></div>' : ""}` +
    `<article class="check-row"><a class="check-row__link" href="/guide/a">guide →</a></article>` +
    `<article class="check-row"><a class="check-row__link" href="/guide/b">guide →</a></article>` +
    `<article class="check-row"><a class="check-row__link" href="/guide/c">guide →</a></article></div>` +
    `<dialog class="inspector-help" id="inspector-help"><form method="dialog"><button class="inspector-help__close">Close</button></form></dialog></div>`;
  mount(document);
}
const key = (k: string): void => {
  document.body.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
};

beforeEach(() => vi.stubGlobal("fetch", vi.fn(() => new Promise(() => {}))));
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("shell keys, phase 4 review", () => {
  it("steps from the row that holds the focused link", () => {
    build("tls", "h", "", false);
    const links = document.querySelectorAll<HTMLElement>(".check-row__link");
    links[1].focus();
    key("j");
    expect(document.activeElement).toBe(document.querySelectorAll(".check-row")[2]);
  });

  it("re-runs a shown run of an empty IP input on r", () => {
    build("ip", "ip", "", true);
    const form = document.querySelector("form") as HTMLFormElement;
    const submit = vi.fn();
    form.requestSubmit = submit;
    key("r");
    expect(submit).toHaveBeenCalledTimes(1);
  });

  it("opens and closes the help without HTMLDialogElement support", () => {
    build("tls", "h", "", false);
    const dialog = document.querySelector("dialog") as HTMLElement & { showModal?: unknown; close?: unknown };
    Object.defineProperty(dialog, "showModal", { value: undefined, configurable: true });
    Object.defineProperty(dialog, "close", { value: undefined, configurable: true });
    Object.defineProperty(dialog, "open", { value: undefined, configurable: true });
    key("?");
    expect(dialog.hasAttribute("open")).toBe(true);
    key("Escape");
    expect(dialog.hasAttribute("open")).toBe(false);
    expect(document.activeElement).toBe(document.querySelector(".inspector-help__open"));
  });
});
