// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount, register } from "../src/main";

// Markup and stream are written by the Rust renderer (crates/web/tests/result_islands_fixtures.rs).
const read = (name: string): Buffer => readFileSync(resolve(process.cwd(), "test/fixtures", name));
const stale = read("result-stale.html").toString("utf8");
const fresh = read("result-fresh.html").toString("utf8");
const runSse = read("result-run.sse");

type Call = { url: string; init: RequestInit };
let calls: Call[];

function stubFetch(): void {
  calls = [];
  vi.stubGlobal(
    "fetch",
    vi.fn((url: unknown, init?: RequestInit) => {
      calls.push({ url: String(url), init: init ?? {} });
      return Promise.resolve(
        new Response(new Blob([runSse]).stream(), {
          status: 200,
          headers: { "content-type": "text/event-stream" },
        }),
      );
    }),
  );
}

function trigger(call: Call): string | null {
  return new URLSearchParams(String(call.init.body)).get("trigger");
}

const recheck = (): HTMLFormElement => document.getElementById("recheck") as HTMLFormElement;

async function streamDone(): Promise<void> {
  await vi.waitFor(() => expect(recheck().getAttribute("data-stream-state")).toBe("done"));
}

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
});

describe("result page stream wiring (requirement 23)", () => {
  it("c2: stale markup posts once to /run with trigger=visit", async () => {
    document.body.innerHTML = stale;
    mount(document);
    await streamDone();
    await settle();
    expect(calls.length).toBe(1);
    expect(calls[0].url).toBe("/shop.example.com/run");
    expect(trigger(calls[0])).toBe("visit");
  });

  it("c2: fresh markup does not post on mount", async () => {
    document.body.innerHTML = fresh;
    mount(document);
    await settle();
    expect(calls.length).toBe(0);
  });

  it("c2: fresh markup posts once with trigger=recheck when the button is submitted", async () => {
    document.body.innerHTML = fresh;
    mount(document);
    await settle();
    const button = document.querySelector('#recheck button[name="trigger"]') as HTMLButtonElement;
    recheck().requestSubmit(button);
    await vi.waitFor(() => expect(calls.length).toBe(1));
    await streamDone();
    expect(calls.length).toBe(1);
    expect(calls[0].url).toBe("/shop.example.com/run");
    expect(trigger(calls[0])).toBe("recheck");
  });

  it("c4: #spectrum is data-refreshing after the start fragment and not after the end", async () => {
    document.body.innerHTML = stale;
    const seen: boolean[] = [];
    document.addEventListener("netray:stream-fragment", (e) => {
      if ((e as CustomEvent).detail === "spectrum") {
        seen.push((e.target as Element).hasAttribute("data-refreshing"));
      }
    });
    mount(document);
    await streamDone();
    expect(seen.length).toBeGreaterThan(1);
    expect(seen[0]).toBe(true);
    expect(seen[seen.length - 1]).toBe(false);
    expect(document.getElementById("spectrum")?.hasAttribute("data-refreshing")).toBe(false);
  });

  it("c6: the end fragments mount the spectrum and issues islands on the new nodes", async () => {
    const spectrum: Element[] = [];
    const issues: Element[] = [];
    register("spectrum", (el) => void spectrum.push(el));
    register("issues", (el) => void issues.push(el));
    document.body.innerHTML = stale;
    mount(document);
    await streamDone();
    const newSpectrum = document.getElementById("spectrum");
    const newIssues = document.getElementById("issues");
    expect(newSpectrum).not.toBeNull();
    expect(newIssues).not.toBeNull();
    expect(spectrum).toContain(newSpectrum);
    expect(issues).toContain(newIssues);
    expect(spectrum[spectrum.length - 1]).toBe(newSpectrum);
    expect(issues[issues.length - 1]).toBe(newIssues);
  });

  it("c6: the recheck form stays attached and ends in data-stream-state=done", async () => {
    document.body.innerHTML = stale;
    const form = recheck();
    mount(document);
    await streamDone();
    expect(form.isConnected).toBe(true);
    expect(recheck()).toBe(form);
    expect(form.getAttribute("data-stream-state")).toBe("done");
  });
});
