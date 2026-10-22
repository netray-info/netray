// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { share } from "../src/share";

// Spec v2-result-fidelity, Phase 7 review: previews load on open; a replaced dialog hands focus on.
function markup(): string {
  return `<div id="result-share"><a id="share" href="/r/Ab3dE5fG" data-island="share">Share</a><dialog id="share-dialog"><button type="button" class="share-dialog__close">×</button><div role="tablist"><button role="tab" aria-selected="true" aria-controls="share-badge">Badge</button></div><div role="tabpanel" id="share-badge"><label><input type="radio" name="badge-style" value="flat" checked data-markdown="m" data-html="h">flat</label><label><input type="radio" name="badge-style" value="for-the-badge" data-markdown="m2" data-html="h2">big</label><textarea id="share-badge-md" readonly>m</textarea><textarea id="share-badge-html" readonly>h</textarea><img class="share-dialog__badge" data-src="/badge/example.com.svg" alt="b"><img class="share-dialog__card" data-src="/r/Ab3dE5fG/og.png" alt="c"></div></dialog></div>`;
}

const link = (): HTMLElement => document.getElementById("share") as HTMLElement;
const badge = (): HTMLImageElement => document.querySelector(".share-dialog__badge") as HTMLImageElement;
const card = (): HTMLImageElement => document.querySelector(".share-dialog__card") as HTMLImageElement;
const click = (el: Element): void => {
  el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
};
const choose = (value: string): void => {
  const radio = document.querySelector(`input[value="${value}"]`) as HTMLInputElement;
  radio.checked = true;
  radio.dispatchEvent(new Event("change", { bubbles: true }));
};

beforeEach(() => {
  const proto = HTMLDialogElement.prototype as unknown as Record<string, unknown>;
  proto.showModal = function (this: HTMLElement) {
    this.setAttribute("open", "");
  };
  proto.close = function (this: HTMLElement) {
    this.removeAttribute("open");
    this.dispatchEvent(new Event("close"));
  };
  document.body.innerHTML = markup();
  share(link());
});

afterEach(() => {
  document.body.innerHTML = "";
});

describe("share island, review", () => {
  it("a mounted island fetches nothing: the previews keep no src until the dialog opens", () => {
    expect(badge().hasAttribute("src")).toBe(false);
    expect(card().hasAttribute("src")).toBe(false);
  });

  it("opening the dialog sets src from data-src on both previews", () => {
    click(link());
    expect(badge().getAttribute("src")).toBe("/badge/example.com.svg");
    expect(card().getAttribute("src")).toBe("/r/Ab3dE5fG/og.png");
  });

  it("the style radio rewrites the badge preview's src and data-src, and back", () => {
    click(link());
    choose("for-the-badge");
    expect(badge().getAttribute("src")).toBe("/badge/example.com.svg?style=for-the-badge");
    expect(badge().dataset.src).toBe("/badge/example.com.svg?style=for-the-badge");
    choose("flat");
    expect(badge().getAttribute("src")).toBe("/badge/example.com.svg");
    expect(badge().dataset.src).toBe("/badge/example.com.svg");
  });

  it("writes no style attribute", () => {
    click(link());
    choose("for-the-badge");
    expect(document.querySelectorAll("[style]").length).toBe(0);
  });

  it("a dialog replaced while open hands focus to the new Share link", () => {
    click(link());
    const old = link();
    document.body.innerHTML = markup();
    expect(old.isConnected).toBe(false);
    share(link());
    expect(document.activeElement).toBe(link());
  });

  it("a dialog closed before the replacement leaves focus alone on the next mount", () => {
    click(link());
    click(document.querySelector(".share-dialog__close") as Element);
    (document.body.appendChild(document.createElement("input")) as HTMLElement).focus();
    const other = document.activeElement;
    document.getElementById("result-share")?.remove();
    const holder = document.createElement("div");
    holder.innerHTML = markup();
    document.body.append(holder);
    share(link());
    expect(document.activeElement).toBe(other);
  });

  it("the focus is handed on once", () => {
    click(link());
    document.body.innerHTML = markup();
    share(link());
    (document.body.appendChild(document.createElement("input")) as HTMLElement).focus();
    const other = document.activeElement;
    document.getElementById("result-share")?.remove();
    document.body.insertAdjacentHTML("afterbegin", markup());
    share(link());
    expect(document.activeElement).toBe(other);
  });
});
