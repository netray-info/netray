// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { dnsview } from "../src/dnsview";

// Spec v2-dns-ui, Phase 2, requirement 9 (C2, C7).
const S1 = "udp:192.0.2.1:53";
const S2 = "udp:192.0.2.2:53";

function markup(): string {
  const card = (s: string) =>
    `<article class="dns-server" data-server="${s}"><h4 class="dns-server__name">${s}</h4><button type="button" class="dns-server__filter" data-server-filter="${s}" hidden>Only this server</button></article>`;
  const item = (s: string) => `<li class="dns-group__server" data-server="${s}">${s}</li>`;
  return `<section class="dns-view" data-island="dnsview"><button type="button" class="dns-view__all" data-server-filter="" hidden>All servers</button><ul class="dns-group__servers">${item(S1)}${item(S2)}</ul><div class="dns-servers">${card(S1)}${card(S2)}</div></section>`;
}

const root = (): HTMLElement => document.querySelector(".dns-view") as HTMLElement;
const filterButtons = (): HTMLElement[] => [
  ...document.querySelectorAll<HTMLElement>("[data-server-filter]"),
];
const filterFor = (s: string): HTMLElement =>
  document.querySelector(`.dns-server__filter[data-server-filter="${s}"]`) as HTMLElement;
const all = (): HTMLElement => document.querySelector(".dns-view__all") as HTMLElement;
const ofServer = (s: string): HTMLElement[] => [
  ...document.querySelectorAll<HTMLElement>(`[data-server="${s}"]`),
].filter((e) => e.matches(".dns-group__server, .dns-server"));
const off = (e: HTMLElement): boolean => e.hasAttribute("data-off");

describe("dnsview island", () => {
  beforeEach(() => {
    document.body.innerHTML = markup();
    dnsview(root());
  });

  it("C2/C7: unhides the buttons, filters to one server, and resets on All servers", () => {
    for (const b of filterButtons()) expect(b.hasAttribute("hidden")).toBe(false);

    filterFor(S1).dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    expect(root().getAttribute("data-server")).toBe(S1);
    expect(ofServer(S1).length).toBe(2);
    expect(ofServer(S2).length).toBe(2);
    for (const e of ofServer(S1)) expect(off(e)).toBe(false);
    for (const e of ofServer(S2)) expect(off(e)).toBe(true);

    all().dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    expect(root().hasAttribute("data-server")).toBe(false);
    expect(document.querySelectorAll("[data-off]").length).toBe(0);
    expect(document.querySelectorAll("[style]").length).toBe(0);
  });
});
