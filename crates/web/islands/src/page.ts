import { closeHelp, exportClick, focusHelpOpenerOnClose, helpIsOpen, isTextTarget, openHelp, unhide } from "./controls";

export function page(el: HTMLElement): void {
  unhide(el);
  focusHelpOpenerOnClose(el);
  el.addEventListener("netray:stream-fragment", () => unhide(el));

  const step = (dir: number): void => {
    const rows = [
      ...el.querySelectorAll<HTMLElement>(el.querySelector("#issues") ? "#issues .check-row" : ".check-row"),
    ];
    // The item holding focus, also when focus sits on a link inside it.
    const active = document.activeElement;
    const at = rows.findIndex((row) => row === active || row.contains(active));
    const next = rows[Math.max(0, Math.min(rows.length - 1, at + dir))];
    if (!next) return;
    next.setAttribute("tabindex", "-1");
    next.focus();
  };

  document.addEventListener("keydown", (ev) => {
    if (!el.isConnected || isTextTarget(ev.target)) return;
    if (ev.ctrlKey || ev.metaKey || ev.altKey) return;
    if (document.querySelector("dialog[open]:not(.inspector-help)")) return;
    if (helpIsOpen(el)) {
      if (ev.key === "Escape") closeHelp(el);
      return;
    }
    switch (ev.key) {
      case "/": {
        const field =
          el.querySelector<HTMLElement>(".query-input__field") ??
          document.querySelector<HTMLElement>(".query-input__field");
        if (!field) break;
        ev.preventDefault();
        field.focus();
        break;
      }
      case "r": {
        const form = document.querySelector<HTMLFormElement>("#recheck");
        form?.requestSubmit(form.querySelector<HTMLElement>("button[name=trigger]") ?? undefined);
        break;
      }
      case "j":
        step(1);
        break;
      case "k":
        step(-1);
        break;
      case "?":
        openHelp(el);
        break;
    }
  });

  el.addEventListener("click", (ev) => {
    const target = ev.target as Element | null;
    if (target?.closest(".inspector-help__open")) return openHelp(el);
    if (target?.closest(".inspector-help__close")) {
      ev.preventDefault();
      return closeHelp(el);
    }
    const exporter = target?.closest<HTMLButtonElement>("button[data-export-action]");
    if (exporter) exportClick(el, exporter);
  });
}
