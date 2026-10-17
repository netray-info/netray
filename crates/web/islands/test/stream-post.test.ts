// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

type Call = { url: string; init: RequestInit };

let calls: Call[];

function sseResponse(): Response {
  // The body stays open: a never-ending stream.
  const body = new ReadableStream<Uint8Array>({ start() {} });
  return new Response(body, {
    status: 200,
    headers: { "content-type": "text/event-stream" },
  });
}

function stubFetch(): void {
  calls = [];
  vi.stubGlobal(
    "fetch",
    vi.fn((url: unknown, init?: RequestInit) => {
      calls.push({ url: String(url), init: init ?? {} });
      return Promise.resolve(sseResponse());
    }),
  );
}

function form(attrs: Record<string, string>, inner: string): HTMLFormElement {
  const f = document.createElement("form");
  f.setAttribute("data-island", "stream");
  for (const [k, v] of Object.entries(attrs)) f.setAttribute(k, v);
  f.innerHTML = inner;
  document.body.appendChild(f);
  return f;
}

function bodyParams(call: Call): URLSearchParams {
  const b = call.init.body as unknown;
  if (b instanceof URLSearchParams) return b;
  return new URLSearchParams(String(b));
}

function header(call: Call, name: string): string | null {
  return new Headers(call.init.headers).get(name);
}

const INPUT = '<input name="d" value="example.com">';

async function settle(): Promise<void> {
  await new Promise((r) => setTimeout(r, 20));
}

beforeEach(() => {
  document.body.innerHTML = "";
  stubFetch();
});

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
  delete (document as unknown as Record<string, unknown>).prerendering;
});

describe("stream island posting (requirement 28)", () => {
  it("autostart posts once on mount with the form fields and trigger=visit", async () => {
    form(
      {
        "data-stream-url": "/example.com/run",
        action: "/x",
        "data-stream-autostart": "visit",
      },
      INPUT,
    );
    mount(document.body);
    await vi.waitFor(() => expect(calls.length).toBe(1));
    await settle();
    expect(calls.length).toBe(1);
    const c = calls[0];
    expect(c.url).toBe("/example.com/run");
    expect(c.init.method).toBe("POST");
    expect(header(c, "accept")).toBe("text/event-stream");
    expect(c.init.credentials).toBe("same-origin");
    const p = bodyParams(c);
    expect(p.get("d")).toBe("example.com");
    expect(p.get("trigger")).toBe("visit");
  });

  it("without autostart posts only on submit, to data-stream-url", async () => {
    const f = form({ "data-stream-url": "/example.com/run", action: "/x" }, INPUT);
    mount(document.body);
    await settle();
    expect(calls.length).toBe(0);
    f.requestSubmit();
    await vi.waitFor(() => expect(calls.length).toBe(1));
    expect(calls[0].url).toBe("/example.com/run");
    expect(calls[0].init.method).toBe("POST");
  });

  it("without data-stream-url posts to the form action", async () => {
    const f = form({ action: "/x" }, INPUT);
    mount(document.body);
    f.requestSubmit();
    await vi.waitFor(() => expect(calls.length).toBe(1));
    expect(calls[0].url).toBe("/x");
  });

  it("waits for prerenderingchange before autostarting", async () => {
    Object.defineProperty(document, "prerendering", {
      value: true,
      configurable: true,
      writable: true,
    });
    form({ "data-stream-url": "/example.com/run", "data-stream-autostart": "visit" }, INPUT);
    mount(document.body);
    await settle();
    expect(calls.length).toBe(0);
    Object.defineProperty(document, "prerendering", {
      value: false,
      configurable: true,
      writable: true,
    });
    document.dispatchEvent(new Event("prerenderingchange"));
    await vi.waitFor(() => expect(calls.length).toBe(1));
    await settle();
    expect(calls.length).toBe(1);
  });

  it("a trigger-named submit button sets the trigger field", async () => {
    const f = form(
      { "data-stream-url": "/example.com/run", "data-stream-autostart": "visit" },
      `${INPUT}<button type="submit" name="trigger" value="recheck">Re-check</button>`,
    );
    mount(document.body);
    await vi.waitFor(() => expect(calls.length).toBe(1));
    const button = f.querySelector("button") as HTMLButtonElement;
    f.requestSubmit(button);
    await vi.waitFor(() => expect(calls.length).toBe(2));
    const p = bodyParams(calls[1]);
    expect(p.get("trigger")).toBe("recheck");
    expect(p.getAll("trigger")).toEqual(["recheck"]);
    expect(p.get("d")).toBe("example.com");
  });

  it("a second submit aborts the running post and posts again", async () => {
    const f = form(
      { "data-stream-url": "/example.com/run" },
      `${INPUT}<button type="submit" name="trigger" value="recheck">Re-check</button>`,
    );
    mount(document.body);
    f.requestSubmit(f.querySelector("button") as HTMLButtonElement);
    await vi.waitFor(() => expect(calls.length).toBe(1));
    expect(bodyParams(calls[0]).get("trigger")).toBe("recheck");
    const first = calls[0].init.signal as AbortSignal;
    expect(first.aborted).toBe(false);
    f.requestSubmit(f.querySelector("button") as HTMLButtonElement);
    await vi.waitFor(() => expect(calls.length).toBe(2));
    expect(first.aborted).toBe(true);
    expect((calls[1].init.signal as AbortSignal).aborted).toBe(false);
  });
});
