// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount, register } from "../src/main";

const enc = new TextEncoder();

function frag(id: string, html: string): string {
  return `event: fragment\ndata: ${JSON.stringify({ id, html })}\n\n`;
}
const END = "event: end\ndata: \n\n";

function splitBytes(bytes: Uint8Array, size: number): Uint8Array[] {
  const out: Uint8Array[] = [];
  for (let i = 0; i < bytes.length; i += size) out.push(bytes.slice(i, i + size));
  return out;
}

function sseResponse(chunks: Uint8Array[], status = 200, type = "text/event-stream"): Response {
  const body = new ReadableStream<Uint8Array>({
    start(c) {
      for (const ch of chunks) c.enqueue(ch);
      c.close();
    },
  });
  return new Response(body, { status, headers: { "content-type": type } });
}

function stubFetch(res: () => Response | Promise<Response>) {
  vi.stubGlobal("fetch", vi.fn(async () => res()));
}

function build(targets: string): HTMLFormElement {
  document.body.innerHTML =
    `<form data-island="stream" data-stream-url="/run" data-stream-autostart="visit">` +
    `<p data-stream-error hidden>failed</p></form>${targets}`;
  return document.querySelector("form") as HTMLFormElement;
}

const settle = () => new Promise((r) => setTimeout(r, 30));

beforeEach(() => {
  document.body.innerHTML = "";
  delete (window as unknown as Record<string, unknown>).__x;
});
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("stream wire", () => {
  it.each([1, 3, 7, 1000])("c14: applies fragments in arrival order, chunks of %i bytes", async (size) => {
    const stream =
      ": keep-alive\n\n" +
      frag("band-dns", '<g id="band-dns" class="one">é</g>') +
      ": another comment\n" +
      frag("nope", '<g id="nope"></g>') +
      frag("band-tls", '<g id="band-tls" class="two"></g>') +
      END;
    stubFetch(() => sseResponse(splitBytes(enc.encode(stream), size)));
    const form = build('<g id="band-dns"></g><g id="band-tls"></g>');
    const seen: string[] = [];
    document.addEventListener("netray:stream-fragment", (e) => seen.push((e as CustomEvent).detail));
    mount(document.body);
    await vi.waitFor(() => expect(form.getAttribute("data-stream-state")).toBe("done"));
    expect(document.getElementById("band-dns")?.outerHTML).toBe('<g id="band-dns" class="one">é</g>');
    expect(document.getElementById("band-tls")?.className).toBe("two");
    expect(document.getElementById("nope")).toBeNull();
    expect(seen).toEqual(["band-dns", "band-tls"]);
  });

  it("c14: fragment event bubbles to document", async () => {
    stubFetch(() => sseResponse([enc.encode(frag("band-dns", '<g id="band-dns"></g>') + END)]));
    const form = build('<g id="band-dns"></g>');
    const onDoc = vi.fn();
    document.addEventListener("netray:stream-fragment", onDoc);
    mount(document.body);
    await vi.waitFor(() => expect(form.getAttribute("data-stream-state")).toBe("done"));
    expect(onDoc).toHaveBeenCalledTimes(1);
  });

  it("c14: multi-line data is joined with a newline", async () => {
    const json = JSON.stringify({ id: "band-dns", html: '<g id="band-dns">a</g>' });
    const half = json.indexOf(",");
    const body = `event: fragment\ndata: ${json.slice(0, half)}\ndata: ${json.slice(half)}\n\n${END}`;
    stubFetch(() => sseResponse(splitBytes(enc.encode(body), 5)));
    const form = build('<g id="band-dns"></g>');
    mount(document.body);
    await vi.waitFor(() => expect(form.getAttribute("data-stream-state")).toBe("done"));
    expect(document.getElementById("band-dns")?.textContent).toBe("a");
  });

  it("c15: a second fragment for the same id replaces the first", async () => {
    stubFetch(() =>
      sseResponse([
        enc.encode(
          frag("band-dns", '<g id="band-dns">first</g>') + frag("band-dns", '<g id="band-dns">second</g>') + END,
        ),
      ]),
    );
    const form = build('<g id="band-dns"></g>');
    mount(document.body);
    await vi.waitFor(() => expect(form.getAttribute("data-stream-state")).toBe("done"));
    expect(document.getElementById("band-dns")?.textContent).toBe("second");
    expect(document.querySelectorAll("#band-dns").length).toBe(1);
  });

  it("c16: scripts are stripped and islands in the fragment are mounted", async () => {
    const probe = vi.fn();
    register("probe", probe);
    const html = '<div id="band-dns"><script>window.__x=1</script><span data-island="probe" id="p"></span></div>';
    stubFetch(() => sseResponse([enc.encode(frag("band-dns", html) + END)]));
    const form = build('<div id="band-dns"></div>');
    mount(document.body);
    await vi.waitFor(() => expect(form.getAttribute("data-stream-state")).toBe("done"));
    expect(document.querySelectorAll("script").length).toBe(0);
    expect((window as unknown as Record<string, unknown>).__x).toBeUndefined();
    expect(probe).toHaveBeenCalledWith(document.getElementById("p"));
  });

  it("c17: a 429 event-stream is applied, running while open, then done with stream-end", async () => {
    let ctl!: ReadableStreamDefaultController<Uint8Array>;
    const body = new ReadableStream<Uint8Array>({ start: (c) => void (ctl = c) });
    stubFetch(
      () => new Response(body, { status: 429, headers: { "content-type": "text/event-stream" } }),
    );
    const form = build('<g id="band-dns"></g>');
    const ended = vi.fn();
    document.addEventListener("netray:stream-end", (e) => ended(e.target));
    mount(document.body);
    ctl.enqueue(enc.encode(frag("band-dns", '<g id="band-dns">limited</g>')));
    await vi.waitFor(() => expect(document.getElementById("band-dns")?.textContent).toBe("limited"));
    expect(form.getAttribute("data-stream-state")).toBe("running");
    ctl.enqueue(enc.encode(END));
    ctl.close();
    await vi.waitFor(() => expect(form.getAttribute("data-stream-state")).toBe("done"));
    expect(ended).toHaveBeenCalledWith(form);
  });

  it("c17: error states", async () => {
    const cases: Array<[string, () => Response | Promise<Response>]> = [
      ["502 html", () => sseResponse([enc.encode("<html></html>")], 502, "text/html")],
      ["rejecting fetch", () => Promise.reject(new TypeError("network"))],
      ["closes without end", () => sseResponse([enc.encode(frag("band-dns", '<g id="band-dns"></g>'))])],
    ];
    for (const [name, res] of cases) {
      stubFetch(res);
      const form = build('<g id="band-dns"></g>');
      mount(document.body);
      await vi.waitFor(() => expect(form.getAttribute("data-stream-state"), name).toBe("error"));
      await settle();
      expect(form.querySelector("[data-stream-error]")?.hasAttribute("hidden"), name).toBe(false);
    }
  });
  it("a fragment for an SVG target is parsed in the SVG namespace", async () => {
    stubFetch(() =>
      sseResponse([enc.encode(frag("band-dns", '<g id="band-dns"><line x1="0" x2="1"></line></g>') + END)]),
    );
    const form = build('<svg><g id="band-dns"></g></svg>');
    mount(document.body);
    await vi.waitFor(() => expect(form.getAttribute("data-stream-state")).toBe("done"));
    const g = document.getElementById("band-dns") as Element;
    expect(g.namespaceURI).toBe("http://www.w3.org/2000/svg");
    expect(g.querySelector("line")?.namespaceURI).toBe("http://www.w3.org/2000/svg");
  });

  it("no fragment after end is applied, whatever the chunking", async () => {
    stubFetch(() =>
      sseResponse([enc.encode(frag("a", '<p id="a">1</p>') + END + frag("b", '<p id="b">late</p>'))]),
    );
    const form = build('<p id="a"></p><p id="b">old</p>');
    mount(document.body);
    await vi.waitFor(() => expect(form.getAttribute("data-stream-state")).toBe("done"));
    await settle();
    expect(document.getElementById("a")?.textContent).toBe("1");
    expect(document.getElementById("b")?.textContent).toBe("old");
  });

  it("a superseded run applies nothing more, even from the chunk in hand", async () => {
    let calls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        calls += 1;
        if (calls === 1) {
          return sseResponse([enc.encode(frag("a", '<p id="a">A1</p>') + frag("b", '<p id="b">B-stale</p>') + END)]);
        }
        return new Response(new ReadableStream<Uint8Array>({ start() {} }), {
          status: 200,
          headers: { "content-type": "text/event-stream" },
        });
      }),
    );
    const form = build('<p id="a"></p><p id="b">old</p>');
    form.insertAdjacentHTML("beforeend", '<button type="submit" name="trigger" value="recheck">go</button>');
    document.addEventListener(
      "netray:stream-fragment",
      (e) => {
        if ((e as CustomEvent).detail === "a") form.requestSubmit(form.querySelector("button") as HTMLButtonElement);
      },
      { once: true },
    );
    mount(document.body);
    await vi.waitFor(() => expect(calls).toBe(2));
    await settle();
    expect(document.getElementById("b")?.textContent).toBe("old");
    expect(form.getAttribute("data-stream-state")).toBe("running");
  });
});
