export function tabs(el: HTMLElement): void {
  const tabOf = (): HTMLElement[] => [...el.querySelectorAll<HTMLElement>("[data-tab]")];

  const show = (name: string): void => {
    for (const tab of tabOf()) {
      if (tab.dataset.tab === name) tab.setAttribute("aria-current", "true");
      else tab.removeAttribute("aria-current");
    }
    for (const panel of el.querySelectorAll<HTMLElement>("[data-panel]")) {
      if (panel.dataset.panel === name) panel.setAttribute("data-shown", "");
      else panel.removeAttribute("data-shown");
    }
  };

  const current = (): string | undefined =>
    (tabOf().find((t) => t.hasAttribute("aria-current")) ?? tabOf()[0])?.dataset.tab;

  el.addEventListener("click", (event) => {
    const tab = (event.target as Element | null)?.closest<HTMLElement>("[data-tab]");
    if (!tab || !el.contains(tab)) return;
    event.preventDefault();
    show(tab.dataset.tab as string);
  });

  el.addEventListener("keydown", (event) => {
    const tab = (event.target as Element | null)?.closest<HTMLElement>("[data-tab]");
    if (!tab) return;
    const all = tabOf();
    const at = all.indexOf(tab);
    const next =
      event.key === "ArrowRight"
        ? all[(at + 1) % all.length]
        : event.key === "ArrowLeft"
          ? all[(at + all.length - 1) % all.length]
          : event.key === "Home"
            ? all[0]
            : event.key === "End"
              ? all[all.length - 1]
              : undefined;
    if (!next) return;
    event.preventDefault();
    show(next.dataset.tab as string);
    next.focus();
  });

  el.addEventListener("netray:stream-fragment", () => {
    const name = current();
    if (name) show(name);
  });

  const targetOf = (hash: string): HTMLElement | null => {
    if (hash.length < 2) return null;
    let id = hash.slice(1);
    try {
      id = decodeURIComponent(id);
    } catch {
      return null;
    }
    const target = document.getElementById(id);
    return target && el.contains(target) ? target : null;
  };

  const reveal = (target: HTMLElement): boolean => {
    const panel = target.closest<HTMLElement>("[data-panel]");
    if (!panel || !el.contains(panel)) return false;
    show(panel.dataset.panel as string);
    return true;
  };

  document.addEventListener("click", (event) => {
    if (event.defaultPrevented) return;
    const link = (event.target as Element | null)?.closest<HTMLAnchorElement>('a[href^="#"]');
    const target = link && targetOf(link.getAttribute("href") as string);
    if (target) reveal(target);
  });

  const follow = (): void => {
    const target = targetOf(window.location.hash);
    if (target && reveal(target)) target.scrollIntoView?.();
  };
  window.addEventListener("hashchange", follow);

  const name = current();
  if (name) show(name);
  follow();
  el.setAttribute("data-ready", "");
}
