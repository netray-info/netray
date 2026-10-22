const COPIED_MS = 1500;

let remembered: HTMLDialogElement | null = null;

const load = (dialog: HTMLElement): void => {
  for (const img of dialog.querySelectorAll<HTMLImageElement>("img[data-src]")) {
    const src = img.dataset.src;
    if (src !== undefined && img.getAttribute("src") !== src) img.setAttribute("src", src);
  }
};

export function share(link: HTMLElement): void {
  const root = link.parentElement;
  const dialog = root?.querySelector<HTMLDialogElement>("dialog");
  if (!root || !dialog) return;

  if (remembered && !remembered.isConnected) {
    remembered = null;
    link.focus();
  }

  const tabs = (): HTMLElement[] => [...dialog.querySelectorAll<HTMLElement>('[role="tab"]')];

  const select = (tab: HTMLElement): void => {
    const target = tab.getAttribute("aria-controls");
    for (const t of tabs()) t.setAttribute("aria-selected", String(t === tab));
    for (const panel of dialog.querySelectorAll<HTMLElement>('[role="tabpanel"]')) {
      if (panel.id === target) panel.removeAttribute("hidden");
      else panel.setAttribute("hidden", "");
    }
  };

  const forget = (): void => {
    if (remembered === dialog) remembered = null;
  };

  const close = (): void => {
    if (dialog.hasAttribute("open")) {
      if (typeof dialog.close === "function") dialog.close();
      else dialog.removeAttribute("open");
    }
    forget();
    link.focus();
  };

  link.addEventListener("click", (event) => {
    event.preventDefault();
    remembered = dialog;
    load(dialog);
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
  });

  dialog.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    close();
  });

  dialog.addEventListener("close", () => {
    forget();
    link.focus();
  });

  dialog.addEventListener("click", (event) => {
    const target = event.target as Element | null;
    if (target?.closest(".share-dialog__close")) {
      close();
      return;
    }
    const tab = target?.closest<HTMLElement>('[role="tab"]');
    if (tab) {
      select(tab);
      return;
    }
    const copy = target?.closest<HTMLElement>("[data-copy]");
    if (copy) void copyField(copy);
  });

  dialog.addEventListener("change", (event) => {
    const radio = event.target as HTMLInputElement | null;
    if (!radio || radio.name !== "badge-style") return;
    const md = dialog.querySelector<HTMLTextAreaElement>("#share-badge-md");
    const html = dialog.querySelector<HTMLTextAreaElement>("#share-badge-html");
    if (md && radio.dataset.markdown !== undefined) md.value = radio.dataset.markdown;
    if (html && radio.dataset.html !== undefined) html.value = radio.dataset.html;
    const img = dialog.querySelector<HTMLImageElement>(".share-dialog__badge");
    if (img) {
      const base = (img.dataset.src ?? img.getAttribute("src") ?? "").split("?")[0];
      const next = radio.value === "flat" ? base : `${base}?style=${radio.value}`;
      img.dataset.src = next;
      img.setAttribute("src", next);
    }
  });

  async function copyField(button: HTMLElement): Promise<void> {
    const field = dialog?.querySelector<HTMLInputElement | HTMLTextAreaElement>(
      `#${button.dataset.copy}`,
    );
    if (!field) return;
    const label = button.dataset.label ?? button.textContent ?? "Copy";
    button.dataset.label = label;
    try {
      await navigator.clipboard.writeText(field.value);
      button.textContent = "Copied";
    } catch {
      field.select();
      return;
    }
    setTimeout(() => {
      button.textContent = label;
    }, COPIED_MS);
  }
}
