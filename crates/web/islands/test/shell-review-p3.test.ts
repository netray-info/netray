// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Review findings on v2-inspector-shell Phase 3: a popstate that changes only the fragment (an
// in-page evidence link) restores nothing; going back to the idle entry reloads the idle page
// instead of submitting an empty query; Enter on a typed DNS query submits even while suggestions
// show, Tab still completes.
function markup(root: string, field: string, tool: string): string {
  return (
    `<div class="inspector-shell" data-island="shell" ${root}>` +
    `<form class="query-input query-input--card" action="/inspect/${tool}" method="get">` +
    `<div class="query-input__box">` +
    `<input class="query-input__field" name="${field}" aria-controls="inspector-suggest" aria-autocomplete="list" autocomplete="off">` +
    `<ul class="inspector-suggest" id="inspector-suggest" role="listbox" hidden></ul></div>` +
    `<button class="query-input__submit" type="submit">Inspect</button></form>` +
    `<div id="inspect-result"></div></div>`
  );
}

let submits = 0;
document.addEventListener("submit", (e) => {
  submits += 1;
  e.preventDefault();
});

const input = (): HTMLInputElement => document.querySelector(".query-input__field") as HTMLInputElement;
const form = (): HTMLFormElement => document.querySelector("form") as HTMLFormElement;

beforeEach(() => {
  submits = 0;
  vi.stubGlobal("fetch", vi.fn(() => new Promise(() => {})));
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("shell island, phase 3 review", () => {
  it("ignores a popstate that only changes the fragment", () => {
    history.replaceState(null, "", "/inspect/ip");
    document.body.innerHTML = markup('data-tool="ip" data-field="ip"', "ip", "ip");
    mount(document);
    history.replaceState(null, "", "/inspect/ip#ev-ip.addresses");
    window.dispatchEvent(new PopStateEvent("popstate"));
    expect(submits).toBe(0);
  });

  it("reloads instead of submitting an empty query when back reaches the idle entry", () => {
    history.replaceState(null, "", "/inspect/tls");
    document.body.innerHTML = markup('data-tool="tls" data-field="h"', "h", "tls");
    mount(document);
    const reload = vi.fn();
    vi.stubGlobal("location", { ...window.location, reload, pathname: "/inspect/tls", search: "", hash: "" });
    input().value = "example.com:443";
    form().requestSubmit();
    const after = submits;
    vi.stubGlobal("location", { ...window.location, reload, pathname: "/inspect/tls", search: "", hash: "" });
    window.dispatchEvent(new PopStateEvent("popstate"));
    expect(submits).toBe(after);
    expect(reload).toHaveBeenCalled();
  });

  it("submits on Enter while suggestions show, and Tab still completes", () => {
    history.replaceState(null, "", "/inspect/dns");
    document.body.innerHTML = markup(
      'data-tool="dns" data-field="q" data-suggest="dns" data-suggest-types="A AAAA MX" data-suggest-flags="+trace" data-suggest-servers="cloudflare"',
      "q",
      "dns",
    );
    mount(document);
    const i = input();
    i.focus();
    i.value = "example.com MX";
    i.setSelectionRange(i.value.length, i.value.length);
    i.dispatchEvent(new Event("input", { bubbles: true }));
    const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    i.dispatchEvent(enter);
    expect(enter.defaultPrevented).toBe(false);

    i.value = "example.com @cl";
    i.setSelectionRange(i.value.length, i.value.length);
    i.dispatchEvent(new Event("input", { bubbles: true }));
    const tab = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
    i.dispatchEvent(tab);
    expect(tab.defaultPrevented).toBe(true);
    expect(i.value).toBe("example.com @cloudflare");
  });
});
