// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// v2-cutover C12: a posted form first empties every element marked `data-stream-reset`, so a
// refused job (one `inspect-result` fragment) leaves no stale back-link and a started job's
// `inspect-backlink` fragment re-fills it.

function fragment(id: string, html: string): string {
  return `event: fragment\ndata: ${JSON.stringify({ id, html })}\n\n`;
}

function stubFetch(sse: string): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => {
      const body = new ReadableStream<Uint8Array>({
        start(c) {
          c.enqueue(new TextEncoder().encode(sse + "event: end\ndata: {}\n\n"));
          c.close();
        },
      });
      return Promise.resolve(
        new Response(body, { status: 200, headers: { "content-type": "text/event-stream" } }),
      );
    }),
  );
}

function page(): HTMLFormElement {
  document.body.innerHTML = `
    <form data-island="stream" data-stream-url="/inspect/tls/run" action="/inspect/tls">
      <input name="h" value="example.org">
    </form>
    <p id="inspect-backlink" data-stream-reset><a href="/example.com">Back to example.com</a></p>
    <div id="inspect-result"></div>`;
  mount(document.body);
  return document.querySelector("form") as HTMLFormElement;
}

const backlink = (): Element => document.getElementById("inspect-backlink") as Element;

beforeEach(() => {
  document.body.innerHTML = "";
});

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("stream reset (C12)", () => {
  it("a refusal answering only inspect-result leaves the back-link empty", async () => {
    stubFetch(fragment("inspect-result", '<div id="inspect-result">Too many requests</div>'));
    const f = page();
    f.requestSubmit();
    await vi.waitFor(() =>
      expect(document.getElementById("inspect-result")?.textContent).toBe("Too many requests"),
    );
    expect(backlink().textContent).toBe("");
    expect(backlink().querySelector("a")).toBeNull();
  });

  it("a started job's inspect-backlink fragment re-fills the back-link", async () => {
    stubFetch(
      fragment(
        "inspect-backlink",
        '<p id="inspect-backlink" data-stream-reset><a href="/example.org">Back to example.org</a></p>',
      ) + fragment("inspect-result", '<div id="inspect-result">done</div>'),
    );
    const f = page();
    f.requestSubmit();
    await vi.waitFor(() =>
      expect(document.getElementById("inspect-result")?.textContent).toBe("done"),
    );
    expect(backlink().textContent).toBe("Back to example.org");
    expect(backlink().querySelector("a")?.getAttribute("href")).toBe("/example.org");
  });
});
