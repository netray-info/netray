const TEXT_FIELD =
  'input:not([type="checkbox"]):not([type="radio"]):not([type="button"]):not([type="submit"]):not([type="reset"]), textarea, select, [contenteditable]';

export function unhide(el: HTMLElement): void {
  el
    .querySelectorAll<HTMLElement>(
      ".inspector-share__copy, .inspector-help__open, .inspector-export button",
    )
    .forEach((b) => (b.hidden = false));
}

const helpDialog = (el: HTMLElement): HTMLDialogElement | null =>
  el.querySelector<HTMLDialogElement>("dialog.inspector-help");

// `hasAttribute`, not `.open`: a browser without HTMLDialogElement only has the attribute.
export function helpIsOpen(el: HTMLElement): boolean {
  return !!helpDialog(el)?.hasAttribute("open");
}

export function openHelp(el: HTMLElement): void {
  const dialog = helpDialog(el);
  if (!dialog || helpIsOpen(el)) return;
  if (typeof dialog.showModal === "function") dialog.showModal();
  else dialog.setAttribute("open", "");
}

export function closeHelp(el: HTMLElement): void {
  const dialog = helpDialog(el);
  if (!dialog || !helpIsOpen(el)) return;
  if (typeof dialog.close === "function") dialog.close();
  else {
    dialog.removeAttribute("open");
    dialog.dispatchEvent(new Event("close"));
  }
}

export function focusHelpOpenerOnClose(el: HTMLElement): void {
  helpDialog(el)?.addEventListener("close", () =>
    el.querySelector<HTMLElement>(".inspector-help__open")?.focus(),
  );
}

// Text fields keep their keys; checkboxes, radios and buttons do not.
export function isTextTarget(target: EventTarget | null): boolean {
  return !!(target as HTMLElement | null)?.closest?.(TEXT_FIELD);
}

export function exportClick(el: HTMLElement, exporter: HTMLButtonElement): void {
  if (!el.contains(exporter) || exporter.disabled) return;
  const kind = exporter.dataset.exportAction ?? "";
  const box = exporter.closest<HTMLElement>(".inspector-export");
  const tpl = el.querySelector<HTMLTemplateElement>(`#inspect-export template[data-export="${kind}"]`);
  const text = tpl?.content.textContent;
  const note = box?.querySelector<HTMLElement>(".inspector-export__status");
  if (text == null) return;
  if (kind === "markdown") {
    // Announce only what happened: the write may be refused or the clipboard absent.
    const done = (msg: string) => () => {
      if (note) note.textContent = msg;
    };
    try {
      if (!navigator.clipboard) throw new Error("no clipboard");
      navigator.clipboard.writeText(text).then(done("Copied Markdown"), done("Copy failed"));
    } catch {
      done("Copy failed")();
    }
    return;
  }
  const url = URL.createObjectURL(new Blob([text], { type: kind === "csv" ? "text/csv" : "application/json" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = (box?.dataset.filename ?? "netray") + "." + kind;
  document.body.append(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
  if (note) note.textContent = kind === "csv" ? "Downloaded CSV" : "Downloaded JSON";
}
