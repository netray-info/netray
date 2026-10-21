// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// v2-dns-ui requirement 16: the DNS provider asks the parse endpoint 150 ms after the last
// keystroke and shows its completions; a failed request changes nothing.
const PARSE = "/api/v1/dns/parse";

function markup(): string {
  return (
    `<div class="inspector-shell" data-island="shell" data-tool="dns" data-field="q" data-suggest="dns" ` +
    `data-suggest-types="A MX" data-suggest-flags="+trace" data-suggest-servers="cloudflare google" ` +
    `data-suggest-parse="${PARSE}">` +
    `<form class="query-input query-input--card" action="/inspect/dns" method="get">` +
    `<div class="query-input__box">` +
    `<input class="query-input__field" name="q" aria-controls="inspector-suggest" aria-autocomplete="list" autocomplete="off">` +
    `<ul class="inspector-suggest" id="inspector-suggest" role="listbox" hidden></ul></div>` +
    `<button class="query-input__submit" type="submit">Inspect</button></form>` +
    `<p class="inspector-shell__hint" id="inspector-parse-hint" hidden></p>` +
    `<div id="inspect-result"></div></div>`
  );
}

const input = (): HTMLInputElement => document.querySelector(".query-input__field") as HTMLInputElement;
const hint = (): HTMLElement => document.querySelector("#inspector-parse-hint") as HTMLElement;
const labels = (): string[] =>
  [...document.querySelectorAll("#inspector-suggest li")].map((li) => li.textContent ?? "");

function type(value: string): void {
  const i = input();
  i.focus();
  i.value = value;
  i.setSelectionRange(value.length, value.length);
  i.dispatchEvent(new Event("input", { bubbles: true }));
}

const answer = (body: unknown, status = 200): Response =>
  ({ ok: status >= 200 && status < 300, status, json: async () => body }) as Response;

const completions = {
  completions: [
    { label: "@cloudflare", detail: "", category: "server" },
    { label: "@cloudflare-tls", detail: "", category: "server" },
    { label: "@google", detail: "", category: "server" },
  ],
  error: null,
  tokens: [],
  query: null,
};

let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.useFakeTimers();
  history.replaceState(null, "", "/inspect/dns");
  document.body.innerHTML = markup();
  fetchMock = vi.fn(async () => answer(completions));
  vi.stubGlobal("fetch", fetchMock);
  mount(document);
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("shell island, DNS parse autocomplete", () => {
  it("posts the input and caret byte offset after 150 ms and lists the completions", async () => {
    type("example.com @cl");
    await vi.advanceTimersByTimeAsync(150);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(PARSE);
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual({ input: "example.com @cl", cursor_pos: 15 });
    expect(labels()).toContain("@cloudflare");
    expect(labels()).toContain("@cloudflare-tls");
    expect(labels()).not.toContain("@google");
  });

  it("counts the caret in UTF-8 bytes", async () => {
    type("exämple.com @cl");
    await vi.advanceTimersByTimeAsync(150);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(init.body as string)).toEqual({ input: "exämple.com @cl", cursor_pos: 16 });
  });

  it("debounces: two inputs 100 ms apart make one fetch with the second value", async () => {
    type("example.com @c");
    await vi.advanceTimersByTimeAsync(100);
    type("example.com @cl");
    await vi.advanceTimersByTimeAsync(150);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(init.body as string).input).toBe("example.com @cl");
  });

  it("keeps the table suggestions and the hint hidden on a 429", async () => {
    fetchMock.mockImplementation(async () => answer({}, 429));
    type("example.com @cl");
    await vi.advanceTimersByTimeAsync(150);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(labels()).toEqual(["@cloudflare"]);
    expect(hint().hidden).toBe(true);
    expect(hint().textContent).toBe("");
  });

  it("keeps the table suggestions and the hint hidden on a network error", async () => {
    fetchMock.mockImplementation(async () => {
      throw new TypeError("network error");
    });
    type("example.com @cl");
    await vi.advanceTimersByTimeAsync(150);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(labels()).toEqual(["@cloudflare"]);
    expect(hint().hidden).toBe(true);
    expect(hint().textContent).toBe("");
  });

  it("shows the parse error as the hint and hides it on a later null error", async () => {
    fetchMock.mockImplementation(async () =>
      answer({ completions: [], error: "unknown record type: ANY", tokens: [], query: null }),
    );
    type("example.com ANY");
    await vi.advanceTimersByTimeAsync(150);
    expect(hint().hidden).toBe(false);
    expect(hint().textContent).toBe("unknown record type: ANY");

    fetchMock.mockImplementation(async () => answer({ completions: [], error: null, tokens: [], query: null }));
    type("example.com A");
    await vi.advanceTimersByTimeAsync(150);
    expect(hint().hidden).toBe(true);
  });
});
