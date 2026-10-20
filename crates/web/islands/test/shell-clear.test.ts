// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Spec v2-inspector-shell, C15 (clear button) and C5 island half (example chips).
const MARKUP =
  '<div class="inspector-shell" data-island="shell" data-tool="tls" data-field="h">' +
  '<form class="query-input query-input--card" action="/inspect/tls" method="get">' +
  '<label class="query-input__label" for="query-input-h">Host</label>' +
  '<input class="query-input__field" id="query-input-h" name="h">' +
  '<button type="button" class="query-input__clear" aria-label="Clear input" hidden>×</button>' +
  '<button class="query-input__submit" type="submit">Inspect</button></form>' +
  '<ul class="inspector-examples"><li><a class="inspector-examples__chip" href="/inspect/tls?h=example.com%3A443" data-example="example.com:443">example.com:443</a></li></ul>' +
  '<div id="inspect-result"></div></div>';

const input = (): HTMLInputElement => document.querySelector(".query-input__field") as HTMLInputElement;
const clear = (): HTMLElement => document.querySelector(".query-input__clear") as HTMLElement;
const form = (): HTMLFormElement => document.querySelector("form") as HTMLFormElement;
const chip = (): HTMLElement => document.querySelector("a.inspector-examples__chip[data-example]") as HTMLElement;

function type(text: string): void {
  input().value = text;
  input().dispatchEvent(new Event("input", { bubbles: true }));
}

describe("shell island", () => {
  beforeEach(() => {
    document.body.innerHTML = MARKUP;
    mount(document);
  });

  it("shows the clear button only while the input holds text", () => {
    expect(clear().hasAttribute("hidden")).toBe(true);
    type("example.com");
    expect(clear().hasAttribute("hidden")).toBe(false);
    clear().click();
    expect(clear().hasAttribute("hidden")).toBe(true);
  });

  it("empties and focuses the input when the clear button is activated", () => {
    type("example.com");
    clear().click();
    expect(input().value).toBe("");
    expect(document.activeElement).toBe(input());
  });

  it("fills the input from an example chip, submits the form and prevents navigation", () => {
    const submit = vi.fn();
    form().requestSubmit = submit;
    const ev = new MouseEvent("click", { bubbles: true, cancelable: true });
    chip().dispatchEvent(ev);
    expect(input().value).toBe("example.com:443");
    expect(ev.defaultPrevented).toBe(true);
    expect(submit).toHaveBeenCalled();
  });
});
