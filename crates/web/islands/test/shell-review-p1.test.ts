// @vitest-environment happy-dom
import { describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Review findings on v2-inspector-shell Phase 1: the clear button follows a value present when the
// island mounts (prefilled or restored by the browser), and a modified click on a chip stays a link.
function build(value: string): void {
  document.body.innerHTML =
    '<div class="inspector-shell" data-island="shell" data-tool="tls" data-field="h">' +
    '<form class="query-input query-input--card" action="/inspect/tls" method="get">' +
    `<input class="query-input__field" name="h" value="${value}">` +
    '<button type="button" class="query-input__clear" aria-label="Clear input" hidden>×</button>' +
    '<button class="query-input__submit" type="submit">Inspect</button></form>' +
    '<div id="inspect-result"><ul class="inspector-examples"><li><a class="inspector-examples__chip" href="/inspect/tls?h=example.com" data-example="example.com">example.com</a></li></ul></div></div>';
  mount(document);
}

const clear = (): HTMLElement => document.querySelector(".query-input__clear") as HTMLElement;

describe("shell island, review", () => {
  it("shows the clear button at mount when the input already holds text", () => {
    build("example.com");
    expect(clear().hasAttribute("hidden")).toBe(false);
  });

  it("keeps the clear button hidden at mount for an empty input", () => {
    build("");
    expect(clear().hasAttribute("hidden")).toBe(true);
  });

  it("lets a modified click on a chip through as a link", () => {
    build("");
    const form = document.querySelector("form") as HTMLFormElement;
    const submit = vi.fn();
    form.requestSubmit = submit;
    const a = document.querySelector("a.inspector-examples__chip") as HTMLElement;
    const ev = new MouseEvent("click", { bubbles: true, cancelable: true, metaKey: true });
    a.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
    expect(submit).not.toHaveBeenCalled();
  });
});
