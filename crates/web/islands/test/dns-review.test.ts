// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { dns } from "../src/dns";

const PICKER = `
  <div data-island="dns" id="picker">
    <input type="checkbox" name="dns-type" value="A">
    <input type="checkbox" name="dns-type" value="MX">
    <input type="radio" name="dns-job" value="query" checked>
    <input type="radio" name="dns-job" value="trace">
    <select name="dns-resolver"><option value="" selected></option></select>
  </div>`;

function tick(picker: HTMLElement, value: string): void {
  const box = picker.querySelector(`input[name=dns-type][value=${value}]`) as HTMLInputElement;
  box.checked = true;
  box.dispatchEvent(new Event("change", { bubbles: true }));
}

describe("dns island review", () => {
  beforeEach(() => {
    document.body.replaceChildren();
  });

  it("with an empty q a change writes nothing", () => {
    document.body.innerHTML = `<form><input name="q" value="">${PICKER}</form>`;
    const q = document.querySelector("input[name=q]") as HTMLInputElement;
    dns(document.getElementById("picker") as HTMLElement);

    tick(document.getElementById("picker") as HTMLElement, "MX");

    expect(q.value).toBe("");
  });

  it("with the picker beside the form a change writes q", () => {
    document.body.innerHTML = `<section><form><input name="q" value="example.com"></form>${PICKER}</section>`;
    const q = document.querySelector("input[name=q]") as HTMLInputElement;
    dns(document.getElementById("picker") as HTMLElement);

    tick(document.getElementById("picker") as HTMLElement, "MX");

    expect(q.value).toBe("example.com MX");
  });
});
