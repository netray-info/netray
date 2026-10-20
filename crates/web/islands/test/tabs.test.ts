// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { tabs } from "../src/tabs";

// Spec v2-result-fidelity, Phase 5, C5 (requirement 35).
const PROTOCOLS = ["dns", "tls", "http", "email", "ip"];

function markup(): string {
  const links = PROTOCOLS.map(
    (p) =>
      `<a class="evidence-tabs__tab marker" href="#evidence-${p}" data-tab="${p}"${
        p === "tls" ? ' aria-current="true"' : ""
      }>${p.toUpperCase()}</a>`,
  ).join("");
  const panels = PROTOCOLS.map(
    (p) =>
      `<div id="evidence-${p}" class="evidence-tabs__panel" data-panel="${p}"${
        p === "tls" ? " data-shown" : ""
      }>${p} body</div>`,
  ).join("");
  return `<div class="evidence-tabs" data-island="tabs"><nav class="evidence-tabs__nav" aria-label="Evidence views">${links}</nav>${panels}</div>`;
}

const root = (): HTMLElement => document.querySelector(".evidence-tabs") as HTMLElement;
const tab = (p: string): HTMLElement => root().querySelector(`[data-tab="${p}"]`) as HTMLElement;
const panel = (p: string): HTMLElement => root().querySelector(`[data-panel="${p}"]`) as HTMLElement;
const shown = (): string[] =>
  [...root().querySelectorAll<HTMLElement>("[data-panel]")]
    .filter((e) => e.hasAttribute("data-shown"))
    .map((e) => e.getAttribute("data-panel") as string);
const current = (): string[] =>
  [...root().querySelectorAll<HTMLElement>("[data-tab]")]
    .filter((e) => e.hasAttribute("aria-current"))
    .map((e) => e.getAttribute("data-tab") as string);

describe("tabs island", () => {
  beforeEach(() => {
    document.body.innerHTML = markup();
    tabs(root());
  });

  it("marks the root ready on mount", () => {
    expect(root().hasAttribute("data-ready")).toBe(true);
  });

  it("C5: a click on the HTTP tab shows only the http panel, without a style", () => {
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    tab("http").dispatchEvent(click);
    expect(click.defaultPrevented).toBe(true);
    expect(shown()).toEqual(["http"]);
    expect(current()).toEqual(["http"]);
    expect(tab("http").getAttribute("aria-current")).toBe("true");
    expect(document.querySelectorAll("[style]").length).toBe(0);
  });

  it("C5: ArrowRight on the current tab moves to the next tab", () => {
    tab("tls").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    expect(current()).toEqual(["http"]);
    expect(shown()).toEqual(["http"]);
  });

  it("C5: a streamed panel replacing the current one is shown again", () => {
    const fresh = document.createElement("div");
    fresh.id = "evidence-tls";
    fresh.className = "evidence-tabs__panel";
    fresh.setAttribute("data-panel", "tls");
    panel("tls").replaceWith(fresh);
    expect(fresh.hasAttribute("data-shown")).toBe(false);
    fresh.dispatchEvent(
      new CustomEvent("netray:stream-fragment", { bubbles: true, detail: "evidence-tls" }),
    );
    expect(shown()).toEqual(["tls"]);
    expect(current()).toEqual(["tls"]);
  });
});
