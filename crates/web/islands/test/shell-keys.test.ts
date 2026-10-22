// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Spec v2-inspector-shell, requirements 17 (help dialog) and 18 (keyboard shortcuts).
const MARKUP =
  '<div class="inspector-shell" data-island="shell" data-tool="tls" data-field="h">' +
  '<nav class="inspector-tabs">' +
  '<a class="inspector-tabs__tab" href="/inspect/dns">DNS</a>' +
  '<a class="inspector-tabs__tab" href="/inspect/tls" aria-current="page">TLS</a>' +
  '<a class="inspector-tabs__tab" href="/inspect/http?url=x">HTTP</a>' +
  '<a class="inspector-tabs__tab" href="/inspect/email">EMAIL</a>' +
  '<a class="inspector-tabs__tab" href="/inspect/ip">IP</a>' +
  '<button type="button" class="inspector-help__open" aria-haspopup="dialog" aria-controls="inspector-help" aria-label="Help" hidden>?</button>' +
  "</nav>" +
  '<form class="query-input query-input--card" action="/inspect/tls" method="get">' +
  '<input class="query-input__field" id="query-input-h" name="h">' +
  '<button class="query-input__submit" type="submit">Inspect</button></form>' +
  '<dialog class="inspector-help" id="inspector-help"><h2>TLS inspector</h2>' +
  '<form method="dialog"><button class="inspector-help__close" value="close">Close</button></form></dialog>' +
  '<div id="inspect-result">' +
  '<article class="check-row" id="r1"><a class="check-row__link" href="#ev-x">evidence →</a><a class="check-row__link" href="/guide/a">guide →</a></article>' +
  '<article class="check-row" id="r2"><a class="check-row__link" href="#ev-x">evidence →</a><a class="check-row__link" href="/guide/b">guide →</a></article>' +
  '<section class="evidence-block" id="ev-x"></section>' +
  "</div></div>";

const input = (): HTMLInputElement => document.querySelector(".query-input__field") as HTMLInputElement;
const form = (): HTMLFormElement => document.querySelector("form.query-input") as HTMLFormElement;
const help = (): HTMLButtonElement => document.querySelector(".inspector-help__open") as HTMLButtonElement;
const dialog = (): HTMLDialogElement => document.querySelector("dialog#inspector-help") as HTMLDialogElement;

function key(target: Element, k: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const ev = new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(ev);
  return ev;
}

describe("shell keyboard and help", () => {
  const assign = vi.fn();

  beforeEach(() => {
    const proto = HTMLDialogElement.prototype as any;
    proto.showModal = function (this: HTMLDialogElement) {
      this.setAttribute("open", "");
    };
    proto.close = function (this: HTMLDialogElement) {
      this.removeAttribute("open");
      this.dispatchEvent(new Event("close"));
    };
    assign.mockReset();
    vi.stubGlobal("location", { assign, href: "http://localhost/inspect/tls" });
    document.body.innerHTML = MARKUP;
    (document.activeElement as HTMLElement | null)?.blur?.();
    mount(document);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("unhides the help button and opens the dialog on ?", () => {
    expect(help().hasAttribute("hidden")).toBe(false);
    key(document.body, "?");
    expect(dialog().hasAttribute("open")).toBe(true);
  });

  it("closes the dialog on Escape and returns focus to the help button", () => {
    help().click();
    expect(dialog().hasAttribute("open")).toBe(true);
    key(dialog(), "Escape");
    expect(dialog().hasAttribute("open")).toBe(false);
    expect(document.activeElement).toBe(help());
  });

  it("focuses the input on /, and focuses and selects it on Ctrl+L and Meta+L", () => {
    const ev = key(document.body, "/");
    expect(ev.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(input());

    for (const mod of [{ ctrlKey: true }, { metaKey: true }]) {
      input().value = "example.com";
      input().blur();
      const e = key(document.body, "l", mod);
      expect(e.defaultPrevented).toBe(true);
      expect(document.activeElement).toBe(input());
      expect(input().selectionStart).toBe(0);
      expect(input().selectionEnd).toBe("example.com".length);
    }
  });

  it("moves focus through results with j and opens the guide with Enter", () => {
    key(document.body, "j");
    key(document.activeElement ?? document.body, "j");
    const second = document.getElementById("r2") as HTMLElement;
    expect(document.activeElement).toBe(second);
    key(second, "Enter");
    expect(assign).toHaveBeenCalledTimes(1);
    expect(assign.mock.calls[0][0]).toBe("/guide/b");
  });

  it("follows the next and previous tab with l and h", () => {
    key(document.body, "l");
    expect(assign).toHaveBeenLastCalledWith("/inspect/http?url=x");
    key(document.body, "h");
    expect(assign).toHaveBeenLastCalledWith("/inspect/dns");
  });

  it("leaves r, j and / to the input while typing", () => {
    const submit = vi.fn();
    form().requestSubmit = submit;
    input().value = "example.com";
    input().focus();
    for (const k of ["r", "j", "/"]) {
      const ev = key(input(), k);
      expect(ev.defaultPrevented).toBe(false);
      expect(document.activeElement).toBe(input());
    }
    expect(submit).not.toHaveBeenCalled();
  });

  it("re-runs on r outside the input when it holds text", () => {
    const submit = vi.fn();
    form().requestSubmit = submit;
    input().value = "example.com";
    key(document.body, "r");
    expect(submit).toHaveBeenCalledTimes(1);
  });
});
