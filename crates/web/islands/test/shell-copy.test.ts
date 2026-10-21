// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Spec v2-ip-ui, requirement 6: per-value copy buttons (.inspector-copy), island half.
const COPY = '<button type="button" class="inspector-copy" data-copy-text="192.0.2.10" hidden>Copy</button>';

const MARKUP =
  '<div class="inspector-shell" data-island="shell" data-tool="ip" data-field="ip">' +
  '<form class="query-input query-input--card" action="/inspect/ip" method="get">' +
  '<label class="query-input__label" for="query-input-ip">Address</label>' +
  '<input class="query-input__field" id="query-input-ip" name="ip">' +
  '<button class="query-input__submit" type="submit">Inspect</button></form>' +
  `<div id="inspect-result">${COPY}</div></div>`;

const copies = (): HTMLButtonElement[] =>
  Array.from(document.querySelectorAll<HTMLButtonElement>(".inspector-copy"));
const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

let writeText: ReturnType<typeof vi.fn>;

beforeEach(() => {
  history.replaceState(null, "", "/inspect/ip");
  document.body.innerHTML = MARKUP;
  vi.stubGlobal("fetch", vi.fn(() => new Promise(() => {})));
  writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("shell copy buttons", () => {
  it("unhides a copy button at mount and writes its data-copy-text on click", async () => {
    expect(copies()[0].hidden).toBe(true);
    mount(document);
    expect(copies()[0].hidden).toBe(false);
    copies()[0].click();
    await flush();
    expect(writeText).toHaveBeenCalledWith("192.0.2.10");
  });

  it("unhides a copy button that arrives in a later fragment", async () => {
    mount(document);
    const result = document.getElementById("inspect-result") as HTMLElement;
    result.insertAdjacentHTML(
      "beforeend",
      '<button type="button" class="inspector-copy" data-copy-text="198.51.100.7" hidden>Copy</button>',
    );
    const late = copies()[1];
    expect(late.hidden).toBe(true);
    (document.querySelector(".inspector-shell") as HTMLElement).dispatchEvent(
      new CustomEvent("netray:stream-fragment", { bubbles: true }),
    );
    expect(late.hidden).toBe(false);
    late.click();
    await flush();
    expect(writeText).toHaveBeenCalledWith("198.51.100.7");
  });
});
