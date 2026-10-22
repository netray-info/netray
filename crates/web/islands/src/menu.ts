// The header's Inspectors menu (spec v2-site-nav, requirement 5): a <details> that opens without
// JavaScript; this island closes an open one on Escape (focus back on its summary) and on a click
// outside it.
export function menu(el: HTMLElement): void {
  const details = el.querySelector<HTMLDetailsElement>("details.nav-menu");
  if (!details) return;
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || !details.open) return;
    // Focus returns to the summary unless it sits elsewhere on the page (a field keeps it).
    const active = document.activeElement;
    const ours = !active || active === document.body || details.contains(active);
    details.open = false;
    if (ours) details.querySelector<HTMLElement>("summary")?.focus();
  });
  document.addEventListener("click", (e) => {
    if (details.open && e.target instanceof Node && !details.contains(e.target)) {
      details.open = false;
    }
  });
}
