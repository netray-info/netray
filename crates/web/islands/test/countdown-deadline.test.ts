// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { countdown } from "../src/countdown";

afterEach(() => {
  vi.useRealTimers();
  document.body.innerHTML = "";
});

describe("countdown deadline", () => {
  it("counts to the server's deadline, not the number of ticks", () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    vi.setSystemTime(new Date("2026-10-18T12:00:00Z"));
    document.body.innerHTML =
      `<form id="recheck"><button name="trigger" value="recheck">re-check now</button></form>` +
      `<span data-island="countdown" data-retry-after="40">40</span>`;
    const span = document.querySelector('[data-island="countdown"]') as HTMLElement;
    const button = document.querySelector('#recheck button[name="trigger"]') as HTMLButtonElement;
    countdown(span);
    expect(button.hasAttribute("disabled")).toBe(true);
    // The page sat in a hidden tab or the back-forward cache: 35 s passed with no tick.
    vi.setSystemTime(new Date("2026-10-18T12:00:35Z"));
    vi.advanceTimersByTime(1000);
    expect(span.textContent).toBe("4");
    expect(span.getAttribute("data-retry-after")).toBe("4");
    vi.setSystemTime(new Date("2026-10-18T12:01:30Z"));
    vi.advanceTimersByTime(1000);
    expect(span.textContent).toBe("0");
    expect(button.hasAttribute("disabled")).toBe(false);
  });
});
