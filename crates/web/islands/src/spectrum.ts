const PHONE = "(max-width: 480px)";

export function spectrum(el: HTMLElement): void {
  const rows = [...el.querySelectorAll<HTMLElement>("li.trace__row[data-run]")];
  const gutter = el.querySelector("#spectrum-gutter");
  const sentence = el.querySelector("#spectrum-sentence");
  const oldest = rows.length - 1;

  let hovered: number | null = null;
  let pinned: number | null = null;
  let selected: string | null = null;

  const phone = (): boolean =>
    typeof window.matchMedia === "function" && window.matchMedia(PHONE).matches;
  const runOf = (row: HTMLElement): number => Number.parseInt(row.getAttribute("data-run") ?? "0", 10);
  const label = (row: HTMLElement | undefined): string =>
    row
      ? `${row.querySelector(".trace__date")?.textContent?.trim() ?? ""} · ${
          row.querySelector(".trace__grade")?.textContent?.trim() ?? ""
        }`
      : "";

  const render = (): void => {
    const shown = hovered ?? pinned ?? 0;
    for (const e of el.querySelectorAll(".spectrum__run[data-run], .spectrum__labels[data-run]")) {
      e.toggleAttribute("data-shown", Number.parseInt(e.getAttribute("data-run") ?? "0", 10) === shown);
    }
    for (const row of rows) {
      const k = runOf(row);
      row.classList.toggle("trace__row--hover", hovered === k);
      row.classList.toggle("trace__row--pinned", pinned === k);
      row.setAttribute("aria-pressed", String(pinned === k));
      if (k === shown) row.setAttribute("aria-current", "true");
      else row.removeAttribute("aria-current");
    }
    if (gutter) gutter.textContent = label(rows.find((row) => runOf(row) === shown));
    if (sentence) {
      const item = [...el.querySelectorAll<HTMLElement>("#spectrum-sentences li[data-code]")].find(
        (li) => li.getAttribute("data-code") === selected,
      );
      sentence.textContent = item?.getAttribute("data-sentence")?.trim() ?? "";
    }
  };

  for (const row of rows) {
    const k = runOf(row);
    for (const type of ["pointerenter", "mouseenter"]) {
      row.addEventListener(type, () => {
        if (phone()) return;
        hovered = k;
        render();
      });
    }
    for (const type of ["pointerleave", "mouseleave"]) {
      row.addEventListener(type, () => {
        if (hovered !== k) return;
        hovered = null;
        render();
      });
    }
    row.addEventListener("click", () => {
      pinned = pinned === k ? null : k;
      render();
    });
  }

  for (const target of el.querySelectorAll("[data-target]")) {
    const select = (): void => {
      selected = target.getAttribute("data-target");
      render();
    };
    target.addEventListener("pointerenter", () => {
      if (!phone()) select();
    });
    target.addEventListener("mouseenter", () => {
      if (!phone()) select();
    });
    target.addEventListener("click", select);
  }

  for (const button of el.querySelectorAll("#spectrum-stepper [data-step]")) {
    button.addEventListener("click", () => {
      const step = Number.parseInt(button.getAttribute("data-step") ?? "0", 10);
      const next = Math.max(0, Math.min(oldest, (pinned ?? 0) + step));
      pinned = next > 0 ? next : null;
      render();
    });
  }

  render();
}
