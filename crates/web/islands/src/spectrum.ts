const PHONE = "(max-width: 480px)";

export function spectrum(el: HTMLElement): void {
  const rows = [...el.querySelectorAll<HTMLElement>("li.trace__row[data-run]")];
  const gutter = el.querySelector("#spectrum-gutter");
  const sentence = el.querySelector("#spectrum-sentence");
  const oldest = rows.length - 1;

  let hovered: number | null = null;
  let pinned: number | null = null;
  let selected: string | null = null;
  let sticky = false;

  const phone = (): boolean =>
    typeof window.matchMedia === "function" && window.matchMedia(PHONE).matches;
  const runOf = (row: HTMLElement): number => Number.parseInt(row.getAttribute("data-run") ?? "0", 10);
  const query = <T extends HTMLElement>(sel: string): T | null => el.ownerDocument.querySelector<T>(sel);
  const caption = el.querySelector<HTMLElement>(".trace__caption");
  const hint = el.querySelector<HTMLElement>(".trace__hint");
  const tapHint = el.querySelector<HTMLElement>(".spectrum-stepper__hint");
  if (tapHint && !tapHint.hasAttribute("data-rest")) {
    tapHint.setAttribute("data-rest", tapHint.textContent ?? "");
  }
  const rest = (e: HTMLElement | null): string => e?.getAttribute("data-rest") ?? "";
  const setText = (e: Element | null, value: string): void => {
    if (e && e.textContent !== value) e.textContent = value;
  };
  let titleSaved: { el: HTMLElement; nodes: Node[] } | null = null;

  const fillGutter = (row: HTMLElement | undefined): void => {
    if (!gutter || !row) return;
    const no = row.getAttribute("data-run-no");
    setText(gutter.querySelector(".spectrum-gutter__run"), no ? `Run ${no}` : "");
    setText(gutter.querySelector(".spectrum-gutter__date"), row.getAttribute("data-date") ?? "");
    const grade = gutter.querySelector(".spectrum-gutter__grade");
    const letter = row.getAttribute("data-grade") ?? "";
    if (grade && grade.textContent !== `grade ${letter}`) {
      const strong = el.ownerDocument.createElement("strong");
      strong.textContent = letter;
      grade.replaceChildren("grade ", strong);
    }
  };

  const stepper = el.querySelector("#spectrum-stepper");
  const fillStepper = (row: HTMLElement | undefined, replaying: boolean): void => {
    if (!stepper || !row) return;
    const run = stepper.querySelector(".spectrum-stepper__run");
    const no = row.getAttribute("data-run-no");
    const date = row.getAttribute("data-date") ?? "";
    const letter = row.getAttribute("data-grade") ?? "";
    const text = `${no ? `Run ${no} · ` : ""}${date} · grade ${letter}`;
    if (run && run.textContent !== text) {
      const strong = (value: string): HTMLElement => {
        const e = el.ownerDocument.createElement("strong");
        e.textContent = value;
        return e;
      };
      run.replaceChildren(
        ...(no ? ["Run ", strong(no), " · "] : []),
        strong(date),
        " · grade ",
        strong(letter),
      );
    }
    setText(
      stepper.querySelector(".spectrum-stepper__state"),
      replaying ? "replaying history · › to return" : "latest run",
    );
  };

  const retitle = (row: HTMLElement | undefined, replaying: boolean): void => {
    const title = query<HTMLElement>("#run-line .run-line--title");
    if (replaying && title && row?.getAttribute("data-run-no")) {
      if (titleSaved?.el !== title) titleSaved = { el: title, nodes: [...title.childNodes] };
      setText(
        title,
        `Run ${row.getAttribute("data-run-no")} · ${row.getAttribute("data-date") ?? ""} · replaying history`,
      );
    } else if (titleSaved) {
      if (titleSaved.el.isConnected) titleSaved.el.replaceChildren(...titleSaved.nodes);
      titleSaved = null;
    }
  };

  const render = (): void => {
    const shown = hovered ?? pinned ?? 0;
    const replaying = shown !== 0;
    const shownRow = rows.find((row) => runOf(row) === shown);
    for (const e of el.querySelectorAll(".spectrum__run[data-run], .spectrum__labels[data-run]")) {
      e.toggleAttribute("data-shown", Number.parseInt(e.getAttribute("data-run") ?? "0", 10) === shown);
    }
    for (const e of el.querySelectorAll(".spectrum__region-grades[data-run], .legend__fixed[data-run]")) {
      const on = Number.parseInt(e.getAttribute("data-run") ?? "0", 10) === shown;
      e.toggleAttribute("data-shown", on);
      e.toggleAttribute("hidden", !on);
    }
    const grades = el.querySelector(`.spectrum__region-grades[data-run="${shown}"]`);
    if (grades) {
      for (const label of el.querySelectorAll<HTMLElement>(".spectrum__region-label[data-protocol]")) {
        const letter = grades.querySelector(`[data-protocol="${(label.getAttribute("data-protocol") ?? "").toLowerCase()}"]`)?.textContent ?? "";
        let slot = label.querySelector(".spectrum__region-grade");
        if (!slot && letter) {
          slot = label.ownerDocument.createElement("span");
          slot.className = "spectrum__region-grade";
          label.append(slot);
        }
        if (slot) slot.textContent = letter;
      }
    }
    for (const row of rows) {
      const k = runOf(row);
      row.classList.toggle("trace__row--hover", hovered === k);
      row.classList.toggle("trace__row--pinned", pinned === k);
      row.toggleAttribute("data-hovered", hovered === k);
      row.toggleAttribute("data-dimmed", hovered !== null && hovered !== k);
      row.toggleAttribute("data-pinned", pinned === k);
      row.setAttribute("aria-pressed", String(pinned === k));
      if (k === shown) row.setAttribute("aria-current", "true");
      else row.removeAttribute("aria-current");
    }
    fillGutter(shownRow);
    fillStepper(shownRow, replaying);
    gutter?.toggleAttribute("data-replaying", replaying);
    retitle(shownRow, replaying);

    const item = [...el.querySelectorAll<HTMLElement>("#spectrum-sentences li[data-code]")].find(
      (li) => li.getAttribute("data-code") === selected,
    );
    const sentenceText = item?.getAttribute("data-sentence")?.trim() ?? "";
    if (sentence) sentence.textContent = sentenceText;
    const name = item?.getAttribute("data-name") || selected;
    const line = `${name}${sentenceText ? ` · ${sentenceText}` : ""}`;
    if (caption) {
      const named = item !== undefined && (hovered !== null || pinned !== null);
      caption.toggleAttribute("data-active", named);
      setText(caption, named ? line : rest(caption));
    }
    if (tapHint) {
      const tapped = item !== undefined && phone();
      tapHint.toggleAttribute("data-active", tapped);
      setText(tapHint, tapped ? line : rest(tapHint));
    }
    if (hint) {
      const run = shownRow?.getAttribute("data-run-no");
      const replayingText = `replaying ${run ? `run ${run}` : "this run"} · move away to return`;
      const pinnedShown = pinned !== null && (hovered === null || hovered === pinned);
      setText(
        hint,
        pinnedShown
          ? "pinned · tap again to release"
          : hovered !== null && (replaying || pinned !== null)
            ? replayingText
            : rest(hint),
      );
    }
  };

  const release = (): void => {
    selected = null;
    sticky = false;
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
      if (pinned === null) release();
      render();
    });
  }

  for (const target of el.querySelectorAll("[data-target]")) {
    const select = (): void => {
      selected = target.getAttribute("data-target");
      render();
    };
    const unselect = (): void => {
      if (sticky || selected !== target.getAttribute("data-target")) return;
      selected = null;
      render();
    };
    for (const type of ["pointerleave", "mouseleave"]) target.addEventListener(type, unselect);
    target.addEventListener("pointerenter", () => {
      if (!phone()) select();
    });
    target.addEventListener("mouseenter", () => {
      if (!phone()) select();
    });
    target.addEventListener("click", () => {
      if (phone() && sticky && selected === target.getAttribute("data-target")) {
        release();
        render();
        return;
      }
      sticky = true;
      select();
    });
  }

  for (const button of el.querySelectorAll("#spectrum-stepper [data-step]")) {
    button.addEventListener("click", () => {
      const step = Number.parseInt(button.getAttribute("data-step") ?? "0", 10);
      const next = Math.max(0, Math.min(oldest, (pinned ?? 0) + step));
      pinned = next > 0 ? next : null;
      if (pinned === null) release();
      render();
    });
  }

  render();
}
