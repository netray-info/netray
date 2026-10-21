export function dnsview(el: HTMLElement): void {
  for (const button of el.querySelectorAll<HTMLElement>("[data-server-filter]")) {
    button.removeAttribute("hidden");
  }

  const apply = (server: string): void => {
    if (server === "") el.removeAttribute("data-server");
    else el.setAttribute("data-server", server);
    for (const item of el.querySelectorAll<HTMLElement>(".dns-group__server, .dns-server")) {
      if (server === "" || (item.dataset.filterKey ?? item.dataset.server) === server) item.removeAttribute("data-off");
      else item.setAttribute("data-off", "");
    }
    // A group none of whose servers is shown hides with them: its records are theirs.
    for (const group of el.querySelectorAll<HTMLElement>(".dns-group")) {
      const shown = group.querySelector(".dns-group__server:not([data-off])");
      if (server === "" || shown) group.removeAttribute("data-off");
      else group.setAttribute("data-off", "");
    }
  };

  el.addEventListener("click", (event) => {
    const button = (event.target as Element | null)?.closest<HTMLElement>("[data-server-filter]");
    if (!button || !el.contains(button)) return;
    apply(button.dataset.serverFilter ?? "");
  });
}
