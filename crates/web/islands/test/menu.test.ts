// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { menu } from "../src/menu";
import { mount } from "../src/main";

const FIXTURE = `<nav class="page-nav" data-island="menu">
<details class="nav-menu"><summary class="nav-menu__button">Inspectors</summary>
<div class="nav-menu__panel" id="inspectors-menu">
<a class="nav-menu__item" href="/inspect/dns">DNS</a>
<a class="nav-menu__all" href="/tools">All tools →</a></div></details>
<a class="guides" href="/guide/">Guides</a></nav>`;

function build(): { nav: HTMLElement; details: HTMLDetailsElement; summary: HTMLElement } {
  document.body.innerHTML = FIXTURE;
  const nav = document.querySelector("nav") as HTMLElement;
  return {
    nav,
    details: nav.querySelector("details") as HTMLDetailsElement,
    summary: nav.querySelector("summary") as HTMLElement,
  };
}

const escape = () =>
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

beforeEach(() => {
  document.body.replaceChildren();
});

describe("menu island", () => {
  it("closes on Escape and returns focus to the summary", () => {
    const { nav, details, summary } = build();
    menu(nav);
    details.setAttribute("open", "");
    escape();
    expect(details.hasAttribute("open")).toBe(false);
    expect(document.activeElement).toBe(summary);
  });

  it("closes on a click on document.body", () => {
    const { nav, details } = build();
    menu(nav);
    details.setAttribute("open", "");
    document.body.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(details.hasAttribute("open")).toBe(false);
  });

  it("closes on a click on a link outside the details", () => {
    const { nav, details } = build();
    menu(nav);
    details.setAttribute("open", "");
    (nav.querySelector(".guides") as HTMLElement).dispatchEvent(
      new MouseEvent("click", { bubbles: true }),
    );
    expect(details.hasAttribute("open")).toBe(false);
  });

  it("stays open on a click inside the panel", () => {
    const { nav, details } = build();
    menu(nav);
    details.setAttribute("open", "");
    const item = nav.querySelector(".nav-menu__item") as HTMLElement;
    item.addEventListener("click", (e) => e.preventDefault());
    item.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    expect(details.hasAttribute("open")).toBe(true);
  });

  it("does nothing on Escape while closed", () => {
    const { nav, details } = build();
    menu(nav);
    expect(() => escape()).not.toThrow();
    expect(details.hasAttribute("open")).toBe(false);
  });

  it("is registered as 'menu' in main", () => {
    const { details } = build();
    mount(document);
    details.setAttribute("open", "");
    escape();
    expect(details.hasAttribute("open")).toBe(false);
  });
});
