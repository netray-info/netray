const BASE = "https://stats.uptimerobot.com/api";
const ID = "36qCbftdDW";
const MONITOR_LIST_URL = `${BASE}/getMonitorList/${ID}`;
const EVENT_FEED_URL = `${BASE}/getEventFeed/${ID}`;
const APEX = "netray.info";

type Monitor = { name: string; statusClass: string };

const STATE: Record<string, string> = {
  success: "operational",
  warning: "degraded",
  danger: "down",
  paused: "paused",
};

const LABEL: Record<string, string> = {
  operational: "Operational",
  degraded: "Degraded",
  down: "Down",
  paused: "Paused",
};

async function get<T>(url: string, key: "data" | "results"): Promise<T> {
  const res = await fetch(url, { credentials: "omit" });
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  const body = await res.json();
  if (key === "results" && !Array.isArray(body?.results)) throw new Error(`${url} has no results array`);
  return body[key] as T;
}

function order(list: Monitor[]): Monitor[] {
  const apex = list.filter((m) => m.name === APEX);
  const rest = list.filter((m) => m.name !== APEX).sort((a, b) => a.name.localeCompare(b.name));
  return [...apex, ...rest];
}

function clone(el: HTMLElement, selector: string): HTMLElement | null {
  const tpl = el.querySelector<HTMLTemplateElement>(selector);
  return (tpl?.content.firstElementChild?.cloneNode(true) as HTMLElement | undefined) ?? null;
}

function fill(root: HTMLElement, field: string, value: string | undefined): void {
  if (!value) return;
  const target = root.querySelector(`[data-field=${field}]`);
  if (target) target.textContent = value;
}

export function status(el: HTMLElement): void {
  const banner = el.querySelector<HTMLElement>("[data-status-banner]");
  const monitors = el.querySelector<HTMLElement>("[data-status-monitors]");
  const incidents = el.querySelector<HTMLElement>("[data-status-incidents]");
  const error = el.querySelector<HTMLElement>("[data-status-error]");

  async function load(): Promise<void> {
    try {
      const [list, events] = await Promise.all([
        get<Monitor[]>(MONITOR_LIST_URL, "data"),
        get<unknown[]>(EVENT_FEED_URL, "results"),
      ]);

      const affected = list.filter((m) => m.statusClass !== "success").length;
      if (banner) {
        banner.textContent =
          affected > 0
            ? `Service degraded — ${affected} of ${list.length} monitors affected`
            : "All systems operational";
      }

      const cards: HTMLElement[] = [];
      for (const m of order(list)) {
        const card = clone(el, "template[data-status-monitor]");
        if (!card) continue;
        const state = STATE[m.statusClass] ?? "unknown";
        card.setAttribute("data-state", state);
        fill(card, "name", m.name);
        fill(card, "status", LABEL[state] ?? "Unknown");
        cards.push(card);
      }
      monitors?.replaceChildren(...cards);

      const items: HTMLElement[] = [];
      for (const ev of events) {
        const item = clone(el, "template[data-status-incident]");
        if (!item) continue;
        const raw: unknown = ev;
        const rec = (typeof raw === "object" && raw !== null ? raw : {}) as Record<string, unknown>;
        fill(item, "title", typeof rec.title === "string" ? rec.title : JSON.stringify(raw));
        fill(item, "date", typeof rec.date === "string" ? rec.date : undefined);
        items.push(item);
      }
      if (items.length === 0) {
        const none = document.createElement("p");
        none.textContent = "No incidents to report.";
        items.push(none);
      }
      incidents?.replaceChildren(...items);

      if (error) error.hidden = true;
    } catch {
      if (error) error.hidden = false;
    }
  }

  void load();
  setInterval(load, 60_000);
}
