// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// stream.sse is written by the Rust encoder (crates/web/tests/it/stream_wire.rs).
const fixture = readFileSync(resolve(process.cwd(), "test/fixtures/stream.sse"));

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("stream fixture", () => {
  it("c23: the island applies the bytes the Rust encoder writes", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(new Blob([fixture]).stream(), {
            status: 200,
            headers: { "content-type": "text/event-stream" },
          }),
      ),
    );
    document.body.innerHTML =
      `<form data-island="stream" data-stream-url="/run" data-stream-autostart="visit"></form>` +
      `<svg><g id="band-dns" class="old"></g></svg>`;
    const form = document.querySelector("form") as HTMLFormElement;
    mount(document.body);
    await vi.waitFor(() => expect(form.getAttribute("data-stream-state")).toBe("done"));
    const g = document.getElementById("band-dns") as Element;
    expect(g.hasAttribute("class")).toBe(false);
  });
});
