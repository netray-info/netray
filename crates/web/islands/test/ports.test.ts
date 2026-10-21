// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { formatHost, parsePorts, ports } from "../src/ports";


describe("parsePorts", () => {
  it.each([
    ["example.com:25,587", "example.com", [25, 587]],
    ["example.com", "example.com", null],
    ["example.com:443", "example.com", [443]],
    ["[2001:db8::1]:443,465", "[2001:db8::1]", [443, 465]],
    ["2001:db8::1", "2001:db8::1", null],
  ])("parsePorts(%s) = host %s, ports %j", (value, host, want) => {
    expect(parsePorts(value)).toEqual({ host, ports: want });
  });
});

describe("formatHost", () => {
  it.each([
    ["example.com", [443], "example.com"],
    ["example.com", [443, 8443], "example.com:443,8443"],
    ["example.com", [25, 587], "example.com:25,587"],
    ["[2001:db8::1]", [443, 465], "[2001:db8::1]:443,465"],
    ["2001:db8::1", [443], "2001:db8::1"],
  ])("formatHost(%s, %j) = %s", (host, list, want) => {
    expect(formatHost(host, list as number[])).toBe(want);
  });
});

function page(max = "7"): { group: HTMLElement; h: HTMLInputElement } {
  document.body.innerHTML = `
    <form id="f">
      <input name="h" value="example.com">
      <div class="port-chooser" data-island="ports" data-max-ports="${max}" hidden id="g">
        <span class="port-chooser__label">Ports</span>
        <div class="port-chooser__toggles">
          <button type="button" class="port-chooser__port" data-port="443" aria-pressed="true">443</button>
          <button type="button" class="port-chooser__port" data-port="8443" aria-pressed="false">8443</button>
          <button type="button" class="port-chooser__add" aria-label="Add port" aria-expanded="false">+</button>
        </div>
        <div class="port-chooser__menu" hidden>
          <button type="button" class="port-chooser__preset" data-ports="443">HTTPS</button>
          <button type="button" class="port-chooser__preset" data-ports="25,465,587,993,995">E-Mail</button>
          <button type="button" class="port-chooser__preset" data-ports="443,25,465,587,993,995,8443">All common</button>
          <input class="port-chooser__field" type="number" min="1" max="65535" inputmode="numeric" aria-label="Another port">
        </div>
        <p class="port-chooser__note" hidden></p>
        <template class="port-chooser__template"><button type="button" class="port-chooser__port" aria-pressed="true"></button></template>
      </div>
    </form>`;
  return {
    group: document.getElementById("g") as HTMLElement,
    h: document.querySelector("input[name=h]") as HTMLInputElement,
  };
}

function toggle(group: HTMLElement, port: number): HTMLElement | null {
  return group.querySelector(`.port-chooser__port[data-port="${port}"]`);
}

function pressed(group: HTMLElement): number[] {
  return [...group.querySelectorAll<HTMLElement>(".port-chooser__port")]
    .filter((b) => b.getAttribute("aria-pressed") === "true")
    .map((b) => Number(b.dataset.port))
    .sort((a, b) => a - b);
}

function typeHost(h: HTMLInputElement, value: string) {
  h.value = value;
  h.dispatchEvent(new Event("input", { bubbles: true }));
}

describe("ports island", () => {
  beforeEach(() => {
    document.body.replaceChildren();
  });

  it("removes hidden from the group on mount", () => {
    const { group } = page();
    expect(group.hasAttribute("hidden")).toBe(true);
    ports(group);
    expect(group.hasAttribute("hidden")).toBe(false);
  });

  it("pressing 8443 rewrites the field to example.com:443,8443", () => {
    const { group, h } = page();
    ports(group);
    (toggle(group, 8443) as HTMLElement).click();
    expect(toggle(group, 8443)?.getAttribute("aria-pressed")).toBe("true");
    expect(h.value).toBe("example.com:443,8443");
  });

  it("the last pressed port cannot be released", () => {
    const { group, h } = page();
    ports(group);
    (toggle(group, 443) as HTMLElement).click();
    expect(toggle(group, 443)?.getAttribute("aria-pressed")).toBe("true");
    expect(h.value).toBe("example.com");
  });

  it("typing example.com:25,587 presses 25 and 587 and releases 443", () => {
    const { group, h } = page();
    ports(group);
    typeHost(h, "example.com:25,587");
    expect(pressed(group)).toEqual([25, 587]);
    expect(toggle(group, 25)).not.toBeNull();
    expect(toggle(group, 587)).not.toBeNull();
    expect(toggle(group, 443)?.getAttribute("aria-pressed")).toBe("false");
  });

  it("the + button toggles the menu and aria-expanded", () => {
    const { group } = page();
    ports(group);
    const add = group.querySelector(".port-chooser__add") as HTMLElement;
    const menu = group.querySelector(".port-chooser__menu") as HTMLElement;
    add.click();
    expect(menu.hasAttribute("hidden")).toBe(false);
    expect(add.getAttribute("aria-expanded")).toBe("true");
    add.click();
    expect(menu.hasAttribute("hidden")).toBe(true);
    expect(add.getAttribute("aria-expanded")).toBe("false");
  });

  it("the E-Mail preset presses exactly 25,465,587,993,995 and rewrites the field", () => {
    const { group, h } = page();
    ports(group);
    const preset = [...group.querySelectorAll<HTMLElement>(".port-chooser__preset")].find(
      (b) => b.textContent === "E-Mail",
    ) as HTMLElement;
    preset.click();
    expect(pressed(group)).toEqual([25, 465, 587, 993, 995]);
    expect(h.value).toBe("example.com:25,465,587,993,995");
  });

  it("the field adds a port on change and ignores 0 and 70000", () => {
    const { group, h } = page();
    ports(group);
    const field = group.querySelector(".port-chooser__field") as HTMLInputElement;

    field.value = "9000";
    field.dispatchEvent(new Event("change", { bubbles: true }));
    expect(pressed(group)).toEqual([443, 9000]);
    expect(h.value).toBe("example.com:443,9000");

    for (const bad of ["0", "70000"]) {
      field.value = bad;
      field.dispatchEvent(new Event("change", { bubbles: true }));
    }
    expect(toggle(group, 0)).toBeNull();
    expect(toggle(group, 70000)).toBeNull();
    expect(pressed(group)).toEqual([443, 9000]);
    expect(h.value).toBe("example.com:443,9000");
  });

  it("refuses an eighth port with data-max-ports=7 and shows the note", () => {
    const { group, h } = page("7");
    ports(group);
    typeHost(h, "example.com:1,2,3,4,5,6,7");
    expect(pressed(group)).toEqual([1, 2, 3, 4, 5, 6, 7]);

    (toggle(group, 8443) as HTMLElement).click();
    expect(toggle(group, 8443)?.getAttribute("aria-pressed")).toBe("false");
    const note = group.querySelector(".port-chooser__note") as HTMLElement;
    expect(note.hasAttribute("hidden")).toBe(false);
    expect(note.textContent).toContain("7");
  });
});
