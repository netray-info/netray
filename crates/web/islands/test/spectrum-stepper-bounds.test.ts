import { describe, expect, it } from "vitest";
import { spectrum } from "../src/spectrum";

// Board M greys out the step that leads nowhere (visual reading V10, v2-result-fidelity Phase 8).
function figure(): HTMLElement {
  document.body.innerHTML = `<figure id="spectrum">
<div id="spectrum-stepper"><button type="button" data-step="1">&lsaquo;</button><button type="button" data-step="-1" disabled>&rsaquo;</button></div>
<ol class="trace"><li class="trace__row" data-run="0" data-run-no="3" data-date="2026-10-20" data-grade="A"></li><li class="trace__row" data-run="1" data-run-no="2" data-date="2026-10-13" data-grade="A"></li><li class="trace__row" data-run="2" data-run-no="1" data-date="2026-10-06" data-grade="B"></li></ol>
</figure>`;
  const el = document.getElementById("spectrum") as HTMLElement;
  spectrum(el);
  return el;
}

describe("spectrum stepper bounds", () => {
  it("disables newer on the latest run and older on the oldest", () => {
    const el = figure();
    const older = el.querySelector('[data-step="1"]') as HTMLButtonElement;
    const newer = el.querySelector('[data-step="-1"]') as HTMLButtonElement;
    expect(newer.disabled).toBe(true);
    expect(older.disabled).toBe(false);
    older.click();
    expect(newer.disabled).toBe(false);
    older.click();
    expect(older.disabled).toBe(true);
    newer.click();
    newer.click();
    expect(newer.disabled).toBe(true);
    expect(older.disabled).toBe(false);
  });
});
