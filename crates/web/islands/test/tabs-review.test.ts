// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { tabs } from "../src/tabs";

// Spec v2-result-fidelity, Phase 5 review: the streamed nav and in-page links into panels.
const PROTOCOLS = ["dns", "tls", "http", "email", "ip"];

function nav(currentTab: string): string {
  const links = PROTOCOLS.map(
    (p) =>
      `<a class="evidence-tabs__tab marker" href="#evidence-${p}" data-tab="${p}"${
        p === currentTab ? ' aria-current="true"' : ""
      }>${p}</a>`,
  ).join("");
  return `<nav id="evidence-nav" class="evidence-tabs__nav" aria-label="Evidence views">${links}</nav>`;
}

function panel(p: string, shown: boolean): string {
  return `<div id="evidence-${p}" class="evidence-tabs__panel" data-panel="${p}"${
    shown ? " data-shown" : ""
  }><table><tr id="ev-${p}-row"><td>${p} body</td></tr></table></div>`;
}

function markup(currentTab: string): string {
  return `<a id="outside" href="#ev-http-row">issue</a><a id="plain" href="#elsewhere">x</a><p id="elsewhere">e</p><div class="evidence-tabs" data-island="tabs">${nav(currentTab)}${PROTOCOLS.map((p) => panel(p, p === currentTab)).join("")}</div>`;
}

const root = (): HTMLElement => document.querySelector(".evidence-tabs") as HTMLElement;
const shown = (): string[] =>
  [...root().querySelectorAll<HTMLElement>("[data-panel]")]
    .filter((e) => e.hasAttribute("data-shown"))
    .map((e) => e.getAttribute("data-panel") as string);
const current = (): string[] =>
  [...root().querySelectorAll<HTMLElement>("[data-tab]")]
    .filter((e) => e.hasAttribute("aria-current"))
    .map((e) => e.getAttribute("data-tab") as string);

// Replaces the elements with `id` the way the stream does: swap, then announce the new element.
function stream(id: string, html: string): void {
  const target = document.getElementById(id) as HTMLElement;
  const holder = document.createElement("div");
  holder.innerHTML = html;
  const inserted = Array.from(holder.children);
  target.replaceWith(...inserted);
  for (const el of inserted) {
    el.dispatchEvent(new CustomEvent("netray:stream-fragment", { bubbles: true, detail: id }));
  }
}

describe("tabs island, streamed nav", () => {
  beforeEach(() => {
    window.location.hash = "";
    document.body.innerHTML = markup("dns");
    tabs(root());
  });

  it("an inserted panel keeps the user's tab while the nav is unchanged", () => {
    (document.querySelector('[data-tab="http"]') as HTMLElement).dispatchEvent(
      new MouseEvent("click", { bubbles: true, cancelable: true }),
    );
    stream("evidence-tls", panel("tls", false));
    expect(current()).toEqual(["http"]);
    expect(shown()).toEqual(["http"]);
  });

  it("the streamed nav replaces the current tab: its panel shows, the stale one hides", () => {
    stream("evidence-tls", panel("tls", true));
    stream("evidence-dns", panel("dns", false));
    stream("evidence-nav", nav("tls"));
    expect(current()).toEqual(["tls"]);
    expect(shown()).toEqual(["tls"]);
    expect(document.querySelectorAll("[style]").length).toBe(0);
  });

  it("the streamed nav wins over a tab the user chose during the run", () => {
    (document.querySelector('[data-tab="email"]') as HTMLElement).dispatchEvent(
      new MouseEvent("click", { bubbles: true, cancelable: true }),
    );
    stream("evidence-nav", nav("http"));
    expect(shown()).toEqual(["http"]);
  });

  it("a click on the streamed nav's tab still switches", () => {
    stream("evidence-nav", nav("tls"));
    (document.querySelector('[data-tab="ip"]') as HTMLElement).dispatchEvent(
      new MouseEvent("click", { bubbles: true, cancelable: true }),
    );
    expect(shown()).toEqual(["ip"]);
  });
});

describe("tabs island, links into panels", () => {
  beforeEach(() => {
    window.location.hash = "";
    document.body.innerHTML = markup("dns");
    tabs(root());
  });

  it("a click on a link outside the card into a hidden panel's element shows that tab", () => {
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    (document.getElementById("outside") as HTMLElement).dispatchEvent(click);
    expect(shown()).toEqual(["http"]);
    expect(current()).toEqual(["http"]);
    expect(click.defaultPrevented).toBe(false);
  });

  it("a click on a link to the panel itself shows its tab", () => {
    const link = document.createElement("a");
    link.setAttribute("href", "#evidence-email");
    document.body.prepend(link);
    link.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    expect(shown()).toEqual(["email"]);
  });

  it("a click on a link to elsewhere leaves the tab", () => {
    (document.getElementById("plain") as HTMLElement).dispatchEvent(
      new MouseEvent("click", { bubbles: true, cancelable: true }),
    );
    expect(shown()).toEqual(["dns"]);
  });

  it("a hashchange to an element inside a hidden panel shows that tab", () => {
    window.location.hash = "#ev-ip-row";
    window.dispatchEvent(new Event("hashchange"));
    expect(shown()).toEqual(["ip"]);
    expect(current()).toEqual(["ip"]);
  });

  it("an initial hash into a hidden panel shows that tab on mount", () => {
    window.location.hash = "#ev-tls-row";
    document.body.innerHTML = markup("dns");
    tabs(root());
    expect(shown()).toEqual(["tls"]);
  });

  it("an initial hash outside the card leaves the server's tab", () => {
    window.location.hash = "#elsewhere";
    document.body.innerHTML = markup("http");
    tabs(root());
    expect(shown()).toEqual(["http"]);
  });
});
