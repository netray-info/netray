// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Spec v2-inspector-shell, requirements 11 and 12: the URL follows the query (C1, C7, C8),
// a finished run's share URL replaces it (C2), and the copy link button (C11).
const SHARE_URL = "/inspect/tls?r=0123456789ab";
const MARKUP =
  '<div class="inspector-shell" data-island="shell" data-tool="tls" data-field="h">' +
  '<form class="query-input query-input--card" action="/inspect/tls" method="get">' +
  '<label class="query-input__label" for="query-input-h">Host</label>' +
  '<input class="query-input__field" id="query-input-h" name="h">' +
  '<button class="query-input__submit" type="submit">Inspect</button></form>' +
  '<div id="inspect-result"><div class="inspector-run__head">' +
  '<div class="inspector-share" id="inspect-share"><button type="button" class="inspector-share__copy" hidden>Copy link</button></div>' +
  "</div></div></div>";

const input = (): HTMLInputElement => document.querySelector(".query-input__field") as HTMLInputElement;
const form = (): HTMLFormElement => document.querySelector("form") as HTMLFormElement;
const copyBtn = (): HTMLElement => document.querySelector("button.inspector-share__copy") as HTMLElement;
const share = (): HTMLElement => document.getElementById("inspect-share") as HTMLElement;
const here = (): string => location.pathname + location.search;

let submits: number;

function submit(value: string): void {
  input().value = value;
  form().requestSubmit();
}

// One listener for the whole file: a listener per test would count every submit once per test so far.
document.addEventListener("submit", (e) => {
  submits += 1;
  e.preventDefault();
});

beforeEach(() => {
  history.replaceState(null, "", "/inspect/tls");
  document.body.innerHTML = MARKUP;
  submits = 0;
  vi.stubGlobal("fetch", vi.fn(() => new Promise(() => {})));
  try {
    localStorage.clear();
  } catch {
    /* no storage */
  }
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("shell URL", () => {
  it("pushes /inspect/<tool>?<field>=<query> once on submit", () => {
    mount(document);
    const push = vi.spyOn(history, "pushState");
    submit("example.com:443");
    expect(here()).toBe("/inspect/tls?h=example.com%3A443");
    expect(push).toHaveBeenCalledTimes(1);
  });

  it("restores the earlier query and resubmits on popstate", () => {
    mount(document);
    submit("a.example.com");
    const first = here();
    submit("b.example.com");
    expect(here()).toBe("/inspect/tls?h=b.example.com");
    const before = submits;
    history.replaceState(null, "", first);
    window.dispatchEvent(new PopStateEvent("popstate"));
    expect(input().value).toBe("a.example.com");
    expect(submits).toBe(before + 1);
  });

  it("replaces the URL with the share URL when the stream ends", () => {
    mount(document);
    share().setAttribute("data-share-url", SHARE_URL);
    form().dispatchEvent(new CustomEvent("netray:stream-end", { bubbles: true }));
    expect(here()).toBe(SHARE_URL);
  });
});

describe("copy link", () => {
  let writeText: ReturnType<typeof vi.fn>;
  let shareFn: ReturnType<typeof vi.fn>;

  function env(coarse: boolean, withShare: boolean): void {
    writeText = vi.fn().mockResolvedValue(undefined);
    shareFn = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    Object.defineProperty(navigator, "share", {
      value: withShare ? shareFn : undefined,
      configurable: true,
    });
    vi.stubGlobal(
      "matchMedia",
      vi.fn((q: string) => ({
        matches: coarse && q.includes("coarse"),
        media: q,
        addEventListener: () => {},
        removeEventListener: () => {},
      })),
    );
  }

  it("unhides the button at mount", () => {
    env(false, false);
    mount(document);
    expect(copyBtn().hasAttribute("hidden")).toBe(false);
  });

  it("copies origin plus data-share-url", () => {
    env(false, false);
    share().setAttribute("data-share-url", SHARE_URL);
    mount(document);
    copyBtn().click();
    expect(writeText).toHaveBeenCalledWith(location.origin + SHARE_URL);
  });

  it("copies location.href without data-share-url", () => {
    env(false, false);
    mount(document);
    copyBtn().click();
    expect(writeText).toHaveBeenCalledWith(location.href);
  });

  it("uses navigator.share on a coarse pointer", () => {
    env(true, true);
    share().setAttribute("data-share-url", SHARE_URL);
    mount(document);
    copyBtn().click();
    expect(shareFn).toHaveBeenCalledWith({ url: location.origin + SHARE_URL });
    expect(writeText).not.toHaveBeenCalled();
  });
});
