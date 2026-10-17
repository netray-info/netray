// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount, register } from "../src/main";

const probe = vi.fn((_el: HTMLElement) => {});
register("probe", probe);

function el(html: string): HTMLElement {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstElementChild as HTMLElement;
}

function mounted(): HTMLElement[] {
  return probe.mock.calls.map((c) => c[0]);
}

beforeEach(() => {
  document.body.replaceChildren();
  probe.mockClear();
});

describe("mount", () => {
  it("runs a registered island on every matching element under root", () => {
    const root = el(`<div><p data-island="probe" id="a"></p><section><i data-island="probe" id="b"></i></section><b id="c"></b></div>`);
    document.body.append(root);
    mount(root);
    expect(mounted().map((e) => e.id).sort()).toEqual(["a", "b"]);
  });

  it("includes root itself when it carries the attribute", () => {
    const root = el(`<div data-island="probe" id="r"><span data-island="probe" id="k"></span></div>`);
    document.body.append(root);
    mount(root);
    expect(mounted().map((e) => e.id).sort()).toEqual(["k", "r"]);
  });

  it("mounts each element once when mount runs twice", () => {
    const root = el(`<div><p data-island="probe" id="a"></p></div>`);
    document.body.append(root);
    mount(root);
    mount(root);
    expect(mounted().map((e) => e.id)).toEqual(["a"]);
  });

  it("mounts once when mount(document) is followed by mount on a subtree", () => {
    const root = el(`<div id="sub"><p data-island="probe" id="a"></p></div>`);
    document.body.append(root);
    mount(document);
    mount(root);
    expect(mounted().map((e) => e.id)).toEqual(["a"]);
  });

  it("ignores an unknown island name without throwing or affecting others", () => {
    const root = el(`<div><p data-island="unknown" id="u"></p><p data-island="probe" id="a"></p></div>`);
    document.body.append(root);
    expect(() => mount(root)).not.toThrow();
    expect(mounted().map((e) => e.id)).toEqual(["a"]);
  });

  it("mounts an element added after a first mount on the second mount", () => {
    const root = el(`<div><p data-island="probe" id="a"></p></div>`);
    document.body.append(root);
    mount(root);
    root.append(el(`<p data-island="probe" id="late"></p>`));
    mount(root);
    expect(mounted().map((e) => e.id)).toEqual(["a", "late"]);
  });
});
