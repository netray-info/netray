// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

const MONITORS = "https://stats.uptimerobot.com/api/getMonitorList/36qCbftdDW";
const EVENTS = "https://stats.uptimerobot.com/api/getEventFeed/36qCbftdDW";

type Mon = { name: string; statusClass: string };

function build(): HTMLElement {
  document.body.innerHTML = `
    <section data-island="status">
      <p data-status-banner></p>
      <div data-status-monitors></div>
      <div data-status-incidents></div>
      <p data-status-error hidden>Status unavailable</p>
      <template data-status-monitor>
        <article data-state=""><h2 data-field="name"></h2><span data-field="status"></span></article>
      </template>
      <template data-status-incident>
        <article><h3 data-field="title"></h3><time data-field="date"></time></article>
      </template>
    </section>`;
  return document.querySelector("[data-island=status]") as HTMLElement;
}

function stubFetch(monitors: Mon[], events: unknown[] = []) {
  const f = vi.fn(async (url: string) => {
    const data = url === MONITORS ? monitors : url === EVENTS ? events : null;
    return { ok: data !== null, status: data !== null ? 200 : 404, json: async () => ({ data }) };
  });
  vi.stubGlobal("fetch", f);
  return f;
}

const q = (s: string) => document.querySelector(s) as HTMLElement;
const cards = () => [...document.querySelectorAll("[data-status-monitors] > *")] as HTMLElement[];
const loaded = () => vi.waitFor(() => expect(cards().length).toBeGreaterThan(0));

beforeEach(() => {
  document.body.innerHTML = "";
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
  document.body.innerHTML = "";
});

describe("status island", () => {
  it("fetches exactly the two UptimeRobot endpoints with credentials omitted", async () => {
    const f = stubFetch([{ name: "a", statusClass: "success" }]);
    build();
    mount(document.body);
    await loaded();
    expect(f.mock.calls.map((c) => c[0]).sort()).toEqual([EVENTS, MONITORS].sort());
    for (const c of f.mock.calls) expect(c[1]).toEqual({ credentials: "omit" });
  });

  it("names the affected count in the banner and marks the down card", async () => {
    stubFetch([
      { name: "dns", statusClass: "success" },
      { name: "tls", statusClass: "danger" },
      { name: "ip", statusClass: "success" },
    ]);
    build();
    mount(document.body);
    await loaded();
    expect(q("[data-status-banner]").textContent).toContain("1 of 3");
    const state = Object.fromEntries(
      cards().map((c) => [c.querySelector("[data-field=name]")!.textContent, c.getAttribute("data-state")]),
    );
    expect(state).toEqual({ dns: "operational", tls: "down", ip: "operational" });
  });

  it.each([
    ["success", "operational"],
    ["warning", "degraded"],
    ["danger", "down"],
    ["paused", "paused"],
  ])("maps statusClass %s to data-state %s", async (cls, state) => {
    stubFetch([{ name: "svc", statusClass: cls }]);
    build();
    mount(document.body);
    await loaded();
    expect(cards()[0].getAttribute("data-state")).toBe(state);
  });

  it("renders a hostile monitor name as text", async () => {
    const name = "<img src=x onerror=alert(1)>";
    stubFetch([{ name, statusClass: "success" }]);
    build();
    mount(document.body);
    await loaded();
    expect(q("[data-status-monitors] [data-field=name]").textContent).toBe(name);
    expect(document.querySelector("img")).toBeNull();
  });

  it("renders incidents from the template", async () => {
    stubFetch(
      [{ name: "svc", statusClass: "success" }],
      [{ title: "DNS outage", date: "2026-10-01" }],
    );
    build();
    mount(document.body);
    await vi.waitFor(() =>
      expect(q("[data-status-incidents] [data-field=title]").textContent).toBe("DNS outage"),
    );
    expect(q("[data-status-incidents] [data-field=date]").textContent).toContain("2026-10-01");
  });

  it("shows the error line when fetch rejects", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new Error("offline"))));
    build();
    mount(document.body);
    await vi.waitFor(() => expect(q("[data-status-error]").hasAttribute("hidden")).toBe(false));
  });

  it("refetches every 60 seconds", async () => {
    vi.useFakeTimers();
    const f = stubFetch([{ name: "svc", statusClass: "success" }]);
    build();
    mount(document.body);
    await vi.advanceTimersByTimeAsync(0);
    const first = f.mock.calls.length;
    expect(first).toBe(2);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(f.mock.calls.length).toBe(first * 2);
  });
});
