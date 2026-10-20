// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Spec v2-inspector-shell, requirements 15 and 16: C5/C18 history, C19 storage failure,
// C6/C20 DNS autocomplete (island half), C21 no autocomplete without data-suggest.
function markup(root: string, field: string, tool: string): string {
  return (
    `<div class="inspector-shell" data-island="shell" ${root}>` +
    `<form class="query-input query-input--card" action="/inspect/${tool}" method="get">` +
    `<div class="query-input__box">` +
    `<input class="query-input__field" id="query-input-${field}" name="${field}" aria-controls="inspector-suggest" aria-autocomplete="list" autocomplete="off">` +
    `<ul class="inspector-suggest" id="inspector-suggest" role="listbox" hidden></ul></div>` +
    `<button type="button" class="query-input__clear" aria-label="Clear input" hidden>x</button>` +
    `<button class="query-input__submit" type="submit">Inspect</button></form>` +
    `<div id="inspect-result"></div></div>`
  );
}
const TLS = markup('data-tool="tls" data-field="h"', "h", "tls");
const DNS = markup(
  'data-tool="dns" data-field="q" data-suggest="dns" data-suggest-types="A AAAA CAA" data-suggest-flags="+trace +dnssec" data-suggest-servers="cloudflare google quad9"',
  "q",
  "dns",
);

function memoryStorage(): Storage {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => (m.has(k) ? (m.get(k) as string) : null),
    setItem: (k: string, v: string) => void m.set(k, String(v)),
    removeItem: (k: string) => void m.delete(k),
    clear: () => m.clear(),
    key: (i: number) => [...m.keys()][i] ?? null,
    get length() {
      return m.size;
    },
  } as Storage;
}
function useStorage(s: Storage) {
  vi.stubGlobal("localStorage", s);
  Object.defineProperty(window, "localStorage", { value: s, configurable: true });
}

const input = (): HTMLInputElement => document.querySelector(".query-input__field") as HTMLInputElement;
const form = (): HTMLFormElement => document.querySelector("form") as HTMLFormElement;
const list = (): HTMLElement => document.getElementById("inspector-suggest") as HTMLElement;
const options = (): HTMLElement[] =>
  [...list().querySelectorAll<HTMLElement>('[role="option"]')].filter((o) => o.dataset.action !== "clear");
const optionTexts = (): string[] => options().map((o) => (o.textContent ?? "").trim());
const key = (k: string, extra: KeyboardEventInit = {}): KeyboardEvent => {
  const ev = new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...extra });
  input().dispatchEvent(ev);
  return ev;
};

let submitted: string[];
function submit(q: string): void {
  input().value = q;
  form().requestSubmit();
}
function openHistory(): void {
  input().value = "";
  input().dispatchEvent(new Event("input", { bubbles: true }));
  input().focus();
  input().dispatchEvent(new FocusEvent("focus"));
}
function typeAtEnd(text: string): void {
  input().value = text;
  input().setSelectionRange(text.length, text.length);
  input().dispatchEvent(new Event("input", { bubbles: true }));
}

function setup(html: string): void {
  document.body.innerHTML = html;
  submitted = [];
  form().addEventListener("submit", (e) => {
    e.preventDefault();
    submitted.push(input().value);
  });
  mount(document);
}

beforeEach(() => {
  useStorage(memoryStorage());
  vi.stubGlobal("scrollTo", () => {});
});
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("shell history", () => {
  it("keeps the newest 20 distinct queries, newest first, and drives the list by keyboard", () => {
    setup(TLS);
    for (let i = 0; i <= 20; i++) submit(`q${i}`);
    openHistory();
    expect(list().hasAttribute("hidden")).toBe(false);
    const expected = Array.from({ length: 20 }, (_, i) => `q${20 - i}`);
    expect(optionTexts()).toEqual(expected);
    expect(JSON.parse(localStorage.getItem("netray.history.tls") as string)).toEqual(expected);

    submit("q5");
    openHistory();
    expect(optionTexts()).toEqual(["q5", ...expected.filter((q) => q !== "q5")]);

    // ArrowDown then Enter takes the first entry and submits.
    key("ArrowDown");
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
    const before = submitted.length;
    key("Enter");
    expect(input().value).toBe("q5");
    expect(submitted.length).toBe(before + 1);

    // Esc closes.
    openHistory();
    expect(list().hasAttribute("hidden")).toBe(false);
    key("Escape");
    expect(list().hasAttribute("hidden")).toBe(true);
  });

  it("empties the storage and closes the list on Clear history", () => {
    setup(TLS);
    submit("a");
    submit("b");
    openHistory();
    const clear = list().querySelector('[data-action="clear"]') as HTMLElement;
    expect(clear.textContent).toContain("Clear history");
    key("ArrowDown");
    key("ArrowDown");
    key("ArrowDown");
    key("Enter");
    expect(localStorage.getItem("netray.history.tls")).toBeNull();
    expect(list().hasAttribute("hidden")).toBe(true);
  });

  it("survives a storage that throws", () => {
    const boom = () => {
      throw new Error("storage denied");
    };
    useStorage({ getItem: boom, setItem: boom, removeItem: boom, clear: boom, key: boom, length: 0 } as unknown as Storage);
    const errors: unknown[] = [];
    window.addEventListener("error", (e) => errors.push(e));
    expect(() => {
      setup(TLS);
      submit("example.com");
      openHistory();
    }).not.toThrow();
    expect(submitted).toEqual(["example.com"]);
    expect(errors).toEqual([]);
  });
});

describe("shell autocomplete", () => {
  it("suggests and accepts a resolver, a flag and a record type for DNS", () => {
    setup(DNS);
    typeAtEnd("example.com @cl");
    expect(list().hasAttribute("hidden")).toBe(false);
    expect(optionTexts()).toEqual(["@cloudflare"]);
    const tab = key("Tab");
    expect(tab.defaultPrevented).toBe(true);
    expect(input().value).toBe("example.com @cloudflare");

    typeAtEnd("example.com +dn");
    expect(optionTexts()).toEqual(["+dnssec"]);

    typeAtEnd("example.com ca");
    expect(optionTexts()).toEqual(["CAA"]);
  });

  it("offers no suggestion without data-suggest", () => {
    setup(TLS);
    typeAtEnd("example.com @cl");
    expect(options()).toEqual([]);
  });
});

// Final review: the autocomplete hook looks the provider up by the name the view gives; a name
// with no provider offers nothing.
describe("autocomplete provider hook", () => {
  it("offers nothing for a provider name it does not know", () => {
    document.body.innerHTML = markup(
      'data-tool="http" data-field="url" data-suggest="http" data-suggest-types="A AAAA CAA" data-suggest-flags="+trace" data-suggest-servers="cloudflare"',
      "url",
      "http",
    );
    mount(document);
    const input = document.querySelector(".query-input__field") as HTMLInputElement;
    input.value = "example.com @cl";
    input.setSelectionRange(input.value.length, input.value.length);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    const list = document.getElementById("inspector-suggest") as HTMLElement;
    expect(list.hidden || list.children.length === 0).toBe(true);
  });
});
