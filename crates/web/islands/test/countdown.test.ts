// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

// Both fixtures are written by the Rust renderer (crates/web/tests/it/result_islands_fixtures.rs).
const fixture = (name: string) => readFileSync(resolve(process.cwd(), "test/fixtures", name));
const stale = fixture("result-stale.html").toString("utf8");
const refused = fixture("result-refused.sse");

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

const button = () => document.querySelector('#recheck button[name="trigger"]') as HTMLButtonElement;
const span = () => document.querySelector('[data-island="countdown"]') as HTMLElement;

describe("countdown island", () => {
  it("c3: a refused re-check shows the wait and mounts the countdown on it", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(new Blob([refused]).stream(), {
            status: 429,
            headers: { "content-type": "text/event-stream" },
          }),
      ),
    );
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    document.body.innerHTML = `<main>${stale}</main>`;
    mount(document.body);
    await vi.waitFor(() => expect(span()).not.toBeNull());
    const line = document.getElementById("run-line") as HTMLElement;
    expect(line.textContent?.replace(/\s+/g, " ").trim()).toBe(
      "too many checks from your address; try again in 2 s",
    );
    expect(button().hasAttribute("disabled")).toBe(true);
    vi.advanceTimersByTime(1000);
    expect(span().textContent).toBe("1");
  });

  it("c5: two ticks reach 0, enable the button and stop", () => {
    vi.useFakeTimers();
    document.body.innerHTML =
      `<form id="recheck"><button name="trigger" value="recheck">re-check now</button></form>` +
      `<span data-island="countdown" data-retry-after="2">2</span>`;
    mount(document.body);
    expect(button().hasAttribute("disabled")).toBe(true);
    vi.advanceTimersByTime(2000);
    expect(span().textContent).toBe("0");
    expect(span().getAttribute("data-retry-after")).toBe("0");
    expect(button().hasAttribute("disabled")).toBe(false);
    vi.advanceTimersByTime(5000);
    expect(span().textContent).toBe("0");
    expect(span().getAttribute("data-retry-after")).toBe("0");
    expect(button().hasAttribute("disabled")).toBe(false);
  });
});
