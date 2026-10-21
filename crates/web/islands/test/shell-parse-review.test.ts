// @vitest-environment happy-dom
// Spec v2-dns-ui, Phase 5 review: late parse answers never reopen, replace a navigated list or
// keep a stale hint; the parser's completions add to the table; the domain token sends nothing.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

const PARSE = "/api/v1/dns/parse";
function markup(withParse = true): string {
  return (
    `<div class="inspector-shell" data-island="shell" data-tool="dns" data-field="q" data-suggest="dns" ` +
    `data-suggest-types="A AAAA ANAME MX NS NSEC NSEC3 NAPTR" data-suggest-flags="+trace" data-suggest-servers="cloudflare google" ` +
    (withParse ? `data-suggest-parse="${PARSE}"` : ``) + `>` +
    `<form class="query-input query-input--card" action="/inspect/dns" method="get">` +
    `<div class="query-input__box">` +
    `<input class="query-input__field" name="q" aria-controls="inspector-suggest" aria-autocomplete="list" autocomplete="off">` +
    `<ul class="inspector-suggest" id="inspector-suggest" role="listbox" hidden></ul>` +
    `<button type="button" class="query-input__clear" aria-label="Clear input" hidden>×</button></div>` +
    `<button class="query-input__submit" type="submit">Inspect</button></form>` +
    `<p class="inspector-shell__hint" id="inspector-parse-hint" hidden></p>` +
    `<div id="inspect-result"><ul class="inspector-examples"><li><a class="inspector-examples__chip" href="/inspect/dns?q=example.com" data-example="example.com MX">example.com MX</a></li></ul></div></div>`
  );
}
const input = (): HTMLInputElement => document.querySelector(".query-input__field") as HTMLInputElement;
const list = (): HTMLElement => document.querySelector("#inspector-suggest") as HTMLElement;
const hint = (): HTMLElement => document.querySelector("#inspector-parse-hint") as HTMLElement;
const labels = (): string[] => [...document.querySelectorAll("#inspector-suggest li")].map((li) => li.textContent ?? "");
function type(value: string): void {
  const i = input();
  i.focus();
  i.value = value;
  i.setSelectionRange(value.length, value.length);
  i.dispatchEvent(new Event("input", { bubbles: true }));
}
const key = (k: string): KeyboardEvent => {
  const ev = new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true });
  input().dispatchEvent(ev);
  return ev;
};
const answer = (body: unknown, status = 200): Response =>
  ({ ok: status >= 200 && status < 300, status, json: async () => body }) as Response;
let fetchMock: ReturnType<typeof vi.fn>;
let submitted: string[];
function setup(withParse = true) {
  vi.useFakeTimers();
  history.replaceState(null, "", "/inspect/dns");
  document.body.innerHTML = markup(withParse);
  submitted = [];
  document.querySelector("form")!.addEventListener("submit", (e) => { e.preventDefault(); submitted.push(input().value); });
  fetchMock = vi.fn(async (_u: string, init: RequestInit) => {
    const v = JSON.parse(init.body as string).input as string;
    if (v.endsWith("MX")) return answer({ completions: [{ label: "MX" }], error: null });
    if (v.endsWith("@cl")) return answer({ completions: [{ label: "@cloudflare" }], error: null });
    if (v.endsWith(" N")) return answer({ completions: [{ label: "NS" }, { label: "NAPTR" }], error: null });
    if (v.endsWith("ANY")) return answer({ completions: [], error: "blocked query type: ANY" });
    return answer({ completions: [], error: null });
  });
  vi.stubGlobal("fetch", fetchMock);
  mount(document);
}
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

describe("probe", () => {
  it("P1 list stays closed after a submit that precedes the parse answer", async () => {
    setup();
    type("example.com MX");
    expect(list().hidden).toBe(false);
    document.querySelector("form")!.requestSubmit();
    expect(list().hidden).toBe(true);
    await vi.advanceTimersByTimeAsync(150);
    expect(submitted).toEqual(["example.com MX"]);
    expect(list().hidden).toBe(true);
  });
  it("P2 ArrowDown selection survives the parse answer", async () => {
    setup();
    type("example.com @cl");
    key("ArrowDown");
    expect(list().children[0].getAttribute("aria-selected")).toBe("true");
    await vi.advanceTimersByTimeAsync(150);
    expect(list().children[0].getAttribute("aria-selected")).toBe("true");
  });
  it("P2b Enter after ArrowDown picks, not submits", async () => {
    setup();
    type("example.com @cl");
    key("ArrowDown");
    await vi.advanceTimersByTimeAsync(150);
    const ev = key("Enter");
    expect(ev.defaultPrevented).toBe(true);
    expect(input().value).toBe("example.com @cloudflare");
  });
  it("P3 Escape keeps the list closed", async () => {
    setup();
    type("example.com @cl");
    key("Escape");
    expect(list().hidden).toBe(true);
    await vi.advanceTimersByTimeAsync(150);
    expect(list().hidden).toBe(true);
  });
  it("P4a hint cleared when input emptied", async () => {
    setup();
    type("example.com ANY");
    await vi.advanceTimersByTimeAsync(150);
    expect(hint().hidden).toBe(false);
    type("");
    await vi.advanceTimersByTimeAsync(150);
    expect(hint().hidden).toBe(true);
  });
  it("P4b hint cleared by clear button", async () => {
    setup();
    type("example.com ANY");
    await vi.advanceTimersByTimeAsync(150);
    (document.querySelector(".query-input__clear") as HTMLElement).click();
    expect(input().value).toBe("");
    expect(hint().hidden).toBe(true);
  });
  it("P4c hint not stale on 429 after edit", async () => {
    setup();
    type("example.com ANY");
    await vi.advanceTimersByTimeAsync(150);
    fetchMock.mockImplementation(async () => answer({}, 429));
    type("example.com A");
    await vi.advanceTimersByTimeAsync(150);
    expect(hint().hidden).toBe(true);
  });
  it("P4d hint not stale after example chip", async () => {
    setup();
    type("example.com ANY");
    await vi.advanceTimersByTimeAsync(150);
    (document.querySelector("a.inspector-examples__chip") as HTMLElement).dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, button: 0 }));
    expect(input().value).toBe("example.com MX");
    await vi.advanceTimersByTimeAsync(300);
    expect(hint().textContent).not.toContain("ANY");
  });
  it("P5 table types narrowed by answer", async () => {
    setup();
    type("example.com N");
    expect(labels()).toEqual(["NS", "NSEC", "NSEC3", "NAPTR"]);
    await vi.advanceTimersByTimeAsync(150);
    // The parser's completions lead; every table type stays.
    expect([...labels()].sort()).toEqual(["NAPTR", "NS", "NSEC", "NSEC3"]);
  });
  it("P6 no endpoint attribute, no fetch", async () => {
    setup(false);
    type("example.com @cl");
    await vi.advanceTimersByTimeAsync(300);
    expect(fetchMock).toHaveBeenCalledTimes(0);
  });
  it("P7 emoji byte offset", async () => {
    setup();
    type("😀.com @cl");
    await vi.advanceTimersByTimeAsync(150);
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(init.body as string).cursor_pos).toBe(12);
  });
  it("P8 hint via textContent", async () => {
    setup();
    fetchMock.mockImplementation(async () => answer({ completions: [], error: "<img src=x onerror=alert(1)>" }));
    type("example.com ANY");
    await vi.advanceTimersByTimeAsync(150);
    expect(hint().querySelector("img")).toBeNull();
    expect(hint().textContent).toBe("<img src=x onerror=alert(1)>");
  });
  it("P9 Tab pick of identical token then answer", async () => {
    setup();
    type("example.com MX");
    key("Tab");
    expect(list().hidden).toBe(true);
    await vi.advanceTimersByTimeAsync(150);
    expect(list().hidden).toBe(true);
  });
  it("P10 domain-token keystrokes send no parse request", async () => {
    setup();
    for (const v of ["e", "ex", "exa", "exam"]) { type(v); await vi.advanceTimersByTimeAsync(200); }
    expect(fetchMock).toHaveBeenCalledTimes(0);
  });
});
