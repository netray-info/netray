// @vitest-environment happy-dom
import { expect, it } from "vitest";
import { dnsview } from "../src/dnsview";

// Spec v2-dns-ui, Phase 2 review: a group whose servers are all filtered out hides with them.
const S1 = "udp:192.0.2.1:53";
const S2 = "udp:192.0.2.2:53";

it("a group with no shown server is off, and comes back on reset", () => {
  const group = (s: string, data: string) =>
    `<div class="dns-group" data-data="${data}"><code>${data}</code><ul><li class="dns-group__server" data-server="${s}">${s}</li></ul></div>`;
  document.body.innerHTML = `<section class="dns-view" data-island="dnsview"><button type="button" class="dns-view__all" data-server-filter="" hidden>All servers</button><button type="button" class="dns-server__filter" data-server-filter="${S1}" hidden>Only</button>${group(S1, "192.0.2.10")}${group(S2, "192.0.2.20")}</section>`;
  const root = document.querySelector(".dns-view") as HTMLElement;
  dnsview(root);
  const click = (sel: string) =>
    (document.querySelector(sel) as HTMLElement).dispatchEvent(
      new MouseEvent("click", { bubbles: true, cancelable: true }),
    );
  const groupOf = (d: string) => document.querySelector(`[data-data="${d}"]`) as HTMLElement;

  click(".dns-server__filter");
  expect(groupOf("192.0.2.10").hasAttribute("data-off")).toBe(false);
  expect(groupOf("192.0.2.20").hasAttribute("data-off")).toBe(true);

  click(".dns-view__all");
  expect(groupOf("192.0.2.20").hasAttribute("data-off")).toBe(false);
  expect(document.querySelector("[style]")).toBeNull();
});
