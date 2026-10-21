const KEY = "netray.dns.view";
const TOGGLES = ["hide-empty", "compact", "only-divergent", "sort-slowest", "explain"];

function load(): Record<string, boolean> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    if (parsed === null || typeof parsed !== "object") return {};
    const state: Record<string, boolean> = {};
    for (const name of TOGGLES) state[name] = (parsed as Record<string, unknown>)[name] === true;
    return state;
  } catch {
    return {};
  }
}

function save(state: Record<string, boolean>): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(state));
  } catch {
    // Storage denied: the view works for this page only.
  }
}

export function dnsview(el: HTMLElement): void {
  for (const button of el.querySelectorAll<HTMLElement>("[data-server-filter]")) {
    button.removeAttribute("hidden");
  }
  el.querySelector(".dns-view__toolbar")?.removeAttribute("hidden");

  const apply = (server: string): void => {
    if (server === "") el.removeAttribute("data-server");
    else el.setAttribute("data-server", server);
    for (const item of el.querySelectorAll<HTMLElement>(".dns-group__server, .dns-server, .dns-record__server")) {
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

  const state = load();

  const reorder = (): void => {
    const results = el.querySelector("#dns-results");
    if (!results) return;
    const key = state["sort-slowest"] ? "rank" : "order";
    const cards = [...results.querySelectorAll<HTMLElement>(".dns-type")];
    cards.sort((a, b) => Number(a.dataset[key]) - Number(b.dataset[key]));
    for (const card of cards) card.parentElement?.appendChild(card);
  };

  const show = (): void => {
    for (const name of TOGGLES) {
      const on = state[name] === true;
      el.toggleAttribute(`data-${name}`, on);
      el.querySelector(`[data-toggle="${name}"]`)?.setAttribute("aria-pressed", String(on));
    }
    reorder();
  };
  show();

  el.addEventListener("click", (event) => {
    const target = event.target as Element | null;
    const filter = target?.closest<HTMLElement>("[data-server-filter]");
    if (filter && el.contains(filter)) {
      apply(filter.dataset.serverFilter ?? "");
      return;
    }
    const toggle = target?.closest<HTMLElement>("[data-toggle]");
    if (toggle && el.contains(toggle)) {
      const name = toggle.dataset.toggle ?? "";
      state[name] = state[name] !== true;
      show();
      save(state);
      return;
    }
    const action = target?.closest<HTMLElement>("[data-action]");
    if (action && el.contains(action)) {
      const open = action.dataset.action === "expand";
      for (const record of el.querySelectorAll<HTMLElement>("details.dns-record")) {
        record.toggleAttribute("open", open);
      }
    }
  });
}
