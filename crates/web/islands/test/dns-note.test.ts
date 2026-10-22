// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { dns } from "../src/dns";

function page(): { builder: HTMLElement; q: HTMLInputElement; note: HTMLElement } {
  document.body.innerHTML = `
    <form id="f">
      <input name="q" value="example.com">
      <div class="dns-builder" data-island="dns" id="builder">
        <p class="dns-builder__note" hidden>+check is not a separate job: the DNS band covers its checks.</p>
      </div>
    </form>`;
  return {
    builder: document.getElementById("builder") as HTMLElement,
    q: document.querySelector("input[name=q]") as HTMLInputElement,
    note: document.querySelector(".dns-builder__note") as HTMLElement,
  };
}

function type(q: HTMLInputElement, value: string): void {
  q.value = value;
  q.dispatchEvent(new Event("input", { bubbles: true }));
}

describe("dns +check note", () => {
  beforeEach(() => {
    document.body.replaceChildren();
  });

  it("shows for a +check query and hides again for a plain one", () => {
    const { builder, q, note } = page();
    dns(builder);
    expect(note.hidden).toBe(true);

    type(q, "example.com +check");
    expect(note.hasAttribute("hidden")).toBe(false);

    type(q, "example.com A");
    expect(note.hasAttribute("hidden")).toBe(true);
  });
});
