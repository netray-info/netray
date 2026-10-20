// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Spec v2-inspector-shell, requirement 25: the page island (result page and idle page).
const MD = "# example.com\n\nGrade A · score 97 / 100\n";

const RESULT =
  '<div id="result" data-island="page">' +
  '<div class="result-hero__actions">' +
  '<form id="recheck" method="post" action="/example.com/recheck"><button type="submit" name="trigger" value="recheck">Re-check</button></form>' +
  '<button type="button" class="inspector-help__open" aria-haspopup="dialog" aria-controls="inspector-help" aria-label="Help" hidden>?</button>' +
  "</div>" +
  '<section id="issues">' +
  '<article class="check-row" id="c1"><a href="#e1">one</a></article>' +
  '<article class="check-row" id="c2"><a href="#e2">two</a></article>' +
  '<article class="check-row" id="c3"><a href="#e3">three</a></article>' +
  "</section>" +
  '<section id="evidence"><div class="inspector-export" id="inspect-export" data-filename="netray-example.com-abcd1234">' +
  '<button type="button" class="inspector-export__copy" data-export-action="markdown" hidden>Copy Markdown</button>' +
  '<button type="button" class="inspector-export__download" data-export-action="csv" hidden>CSV</button>' +
  '<button type="button" class="inspector-export__download" data-export-action="json" hidden>JSON</button>' +
  '<span class="inspector-export__status" role="status"></span>' +
  `<template data-export="markdown">${MD.replace(/\n/g, "\n")}</template>` +
  '<template data-export="csv">section,item,field,value\n</template>' +
  '<template data-export="json">{}</template>' +
  "</div></section>" +
  '<dialog class="inspector-help" id="inspector-help"><h2>Result page</h2>' +
  '<form method="dialog"><button class="inspector-help__close" value="close">Close</button></form></dialog>' +
  "</div>";

const IDLE =
  '<div data-island="page"><form class="query-input"><input class="query-input__field" name="d"></form></div>';

const q = <T extends Element>(s: string): T => document.querySelector(s) as T;

function key(target: Element, k: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const ev = new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(ev);
  return ev;
}

beforeEach(() => {
  const proto = HTMLDialogElement.prototype as any;
  proto.showModal = function (this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
  proto.close = function (this: HTMLDialogElement) {
    this.removeAttribute("open");
    this.dispatchEvent(new Event("close"));
  };
  (document.activeElement as HTMLElement | null)?.blur?.();
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("page island", () => {
  describe("result page", () => {
    beforeEach(() => {
      document.body.innerHTML = RESULT;
      mount(document);
    });

    it("C2: unhides ?, opens help on ?, Escape closes and returns focus to ?", () => {
      const help = q<HTMLButtonElement>(".inspector-help__open");
      expect(help.hasAttribute("hidden")).toBe(false);
      key(document.body, "?");
      expect(q<HTMLDialogElement>("dialog#inspector-help").hasAttribute("open")).toBe(true);
      key(q("dialog#inspector-help"), "Escape");
      expect(q<HTMLDialogElement>("dialog#inspector-help").hasAttribute("open")).toBe(false);
      expect(document.activeElement).toBe(help);
    });

    it("C6: r submits #recheck with the trigger button as submitter", () => {
      const spy = vi.fn();
      q<HTMLFormElement>("form#recheck").requestSubmit = spy;
      key(document.body, "r");
      expect(spy).toHaveBeenCalledTimes(1);
      expect(spy.mock.calls[0][0]).toBe(q("form#recheck button[name=trigger]"));
    });

    it("j twice focuses the second check row", () => {
      key(document.body, "j");
      key(document.body, "j");
      expect(document.activeElement).toBe(document.querySelectorAll("#issues .check-row")[1]);
    });

    it("C3: Copy Markdown writes the markdown template to the clipboard", () => {
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
      q<HTMLButtonElement>('[data-export-action="markdown"]').click();
      expect(writeText).toHaveBeenCalledWith(MD);
    });
  });

  describe("idle page", () => {
    beforeEach(() => {
      document.body.innerHTML = IDLE;
      mount(document);
    });

    it("C5: / focuses the domain input; r typed inside it does nothing", () => {
      const ev = key(document.body, "/");
      expect(ev.defaultPrevented).toBe(true);
      const field = q<HTMLInputElement>(".query-input__field");
      expect(document.activeElement).toBe(field);
      const r = key(field, "r");
      expect(r.defaultPrevented).toBe(false);
      expect(document.activeElement).toBe(field);
    });
  });
});

// Phase 8 review: the export stays until a run's own export fragment replaces it, and `/` on a
// page without a domain input is left to the browser.
describe("page island, review", () => {
  it("keeps the export slot when a stream fragment arrives", () => {
    document.body.innerHTML =
      '<div data-island="page"><div id="inspect-export" class="inspector-export"></div><p id="run-line"></p></div>';
    mount(document);
    document
      .getElementById("run-line")
      ?.dispatchEvent(new CustomEvent("netray:stream-fragment", { bubbles: true, detail: "run-line" }));
    expect(document.getElementById("inspect-export")).not.toBeNull();
  });

  it("leaves / alone where the page has no domain input", () => {
    document.body.innerHTML = '<div data-island="page"><p>result</p></div>';
    mount(document);
    const ev = new KeyboardEvent("keydown", { key: "/", bubbles: true, cancelable: true });
    document.body.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
  });
});
