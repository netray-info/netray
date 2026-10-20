// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { share } from "../src/share";

// Spec v2-result-fidelity, Phase 7, requirement 41 (C3 style, C5 copy, C6 Esc and focus).
const FLAT_MD = "[![netray grade](https://netray.info/badge/example.com.svg)](https://netray.info/example.com)";
const FLAT_HTML =
  '<a href="https://netray.info/example.com"><img src="https://netray.info/badge/example.com.svg" alt="netray grade"></a>';
const BIG_MD =
  "[![netray grade](https://netray.info/badge/example.com.svg?style=for-the-badge)](https://netray.info/example.com)";
const BIG_HTML =
  '<a href="https://netray.info/example.com"><img src="https://netray.info/badge/example.com.svg?style=for-the-badge" alt="netray grade"></a>';

function markup(): string {
  const names = ["Snapshot", "Re-run", "Badge", "Card"];
  const ids = ["share-snapshot", "share-rerun", "share-badge", "share-card"];
  const tabs = names
    .map(
      (n, i) =>
        `<button role="tab" aria-selected="${i === 0}" aria-controls="${ids[i]}">${n}</button>`,
    )
    .join("");
  return `<div id="result-share"><a id="share" class="button button--secondary" href="/r/Ab3dE5fG" data-island="share" aria-haspopup="dialog">Share</a><dialog id="share-dialog" class="share-dialog" aria-labelledby="share-dialog-title"><h2 id="share-dialog-title">Share and embed</h2><button type="button" class="share-dialog__close" aria-label="Close">×</button><div role="tablist">${tabs}</div>
<div role="tabpanel" id="share-snapshot"><input id="share-snapshot-url" class="share-dialog__field" readonly value="https://netray.info/r/Ab3dE5fG"><button type="button" class="share-dialog__copy" data-copy="share-snapshot-url">Copy</button></div>
<div role="tabpanel" id="share-rerun" hidden><p>re-run</p></div>
<div role="tabpanel" id="share-badge" hidden><label><input type="radio" name="badge-style" value="flat" checked data-markdown="${FLAT_MD}" data-html='${FLAT_HTML}'>flat</label><label><input type="radio" name="badge-style" value="for-the-badge" data-markdown="${BIG_MD}" data-html='${BIG_HTML}'>for-the-badge</label><textarea id="share-badge-md" class="share-dialog__field" readonly>${FLAT_MD}</textarea><textarea id="share-badge-html" class="share-dialog__field" readonly>${FLAT_HTML.replace(/</g, "&lt;")}</textarea><button type="button" class="share-dialog__copy" data-copy="share-badge-md">Copy</button><img class="share-dialog__badge" data-src="/badge/example.com.svg"></div>
<div role="tabpanel" id="share-card" hidden><p>card</p></div></dialog></div>`;
}

const link = (): HTMLElement => document.getElementById("share") as HTMLElement;
const dialog = (): HTMLDialogElement => document.getElementById("share-dialog") as HTMLDialogElement;
const tabBtn = (name: string): HTMLElement =>
  [...document.querySelectorAll<HTMLElement>('[role="tab"]')].find(
    (t) => t.textContent === name,
  ) as HTMLElement;
const panels = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('[role="tabpanel"]')];
const click = (el: Element): MouseEvent => {
  const e = new MouseEvent("click", { bubbles: true, cancelable: true });
  el.dispatchEvent(e);
  return e;
};
const settle = () => new Promise((r) => setTimeout(r, 0));

let writeText: ReturnType<typeof vi.fn>;

beforeEach(() => {
  const proto = HTMLDialogElement.prototype as unknown as Record<string, unknown>;
  if (typeof proto.showModal !== "function") {
    proto.showModal = function (this: HTMLElement) {
      this.setAttribute("open", "");
    };
  }
  if (typeof proto.close !== "function") {
    proto.close = function (this: HTMLElement) {
      this.removeAttribute("open");
      this.dispatchEvent(new Event("close"));
    };
  }
  writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  document.body.innerHTML = markup();
  share(link());
});

afterEach(() => {
  document.body.innerHTML = "";
});

describe("share island", () => {
  it("open: a click on the Share link is prevented and opens the dialog", () => {
    const e = click(link());
    expect(e.defaultPrevented).toBe(true);
    expect(dialog().hasAttribute("open")).toBe(true);
  });

  it("tabs: the Badge tab shows its panel, hides the others, moves aria-selected", () => {
    click(link());
    click(tabBtn("Badge"));
    for (const p of panels()) {
      expect(p.hasAttribute("hidden")).toBe(p.id !== "share-badge");
    }
    for (const t of document.querySelectorAll('[role="tab"]')) {
      expect(t.getAttribute("aria-selected")).toBe(t.textContent === "Badge" ? "true" : "false");
    }
  });

  it("C3: the for-the-badge radio rewrites both snippets and the preview", () => {
    click(link());
    click(tabBtn("Badge"));
    const radio = document.querySelector('input[value="for-the-badge"]') as HTMLInputElement;
    radio.checked = true;
    radio.dispatchEvent(new Event("change", { bubbles: true }));
    const md = document.getElementById("share-badge-md") as HTMLTextAreaElement;
    const html = document.getElementById("share-badge-html") as HTMLTextAreaElement;
    expect(md.value).toBe(BIG_MD);
    expect(html.value).toBe(BIG_HTML);
    expect(md.value).toContain("style=for-the-badge");
    expect(html.value).toContain("style=for-the-badge");
    const img = document.querySelector(".share-dialog__badge") as HTMLImageElement;
    expect(img.getAttribute("src")).toContain("?style=for-the-badge");
  });

  it("C5: a copy button writes the text of its input and announces Copied", async () => {
    click(link());
    const btn = document.querySelector('[data-copy="share-snapshot-url"]') as HTMLElement;
    click(btn);
    await settle();
    expect(writeText).toHaveBeenCalledWith("https://netray.info/r/Ab3dE5fG");
    expect(btn.textContent).toContain("Copied");
  });

  it("C6: Escape in the open dialog closes it and focuses the Share link", () => {
    click(link());
    dialog().dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    expect(dialog().hasAttribute("open")).toBe(false);
    expect(document.activeElement).toBe(link());
  });

  it("C6: the close button closes the dialog and focuses the Share link", () => {
    click(link());
    click(document.querySelector(".share-dialog__close") as Element);
    expect(dialog().hasAttribute("open")).toBe(false);
    expect(document.activeElement).toBe(link());
  });

  it("writes no style attribute anywhere", () => {
    click(link());
    click(tabBtn("Badge"));
    const radio = document.querySelector('input[value="for-the-badge"]') as HTMLInputElement;
    radio.checked = true;
    radio.dispatchEvent(new Event("change", { bubbles: true }));
    expect(document.querySelectorAll("[style]").length).toBe(0);
  });
});
