const KEY = "netray.grouping";
type Grouping = "owner" | "severity" | "protocol";

const SEVERITY = ["critical", "high", "medium", "low"];

const PROTOCOL_LABEL: Record<string, string> = {
  dns: "DNS",
  tls: "TLS",
  http: "HTTP",
  email: "Email",
  ip: "IP",
};

const isGrouping = (v: string | null): v is Grouping =>
  v === "owner" || v === "severity" || v === "protocol";

function rank(row: HTMLElement): [number, number, number] {
  const sev = SEVERITY.indexOf(row.dataset.severity ?? "");
  return [
    row.dataset.status === "fail" ? 0 : 1,
    sev < 0 ? SEVERITY.length : sev,
    Number(row.dataset.order),
  ];
}

function compare(a: HTMLElement, b: HTMLElement): number {
  const x = rank(a);
  const y = rank(b);
  return x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
}

export function issues(el: HTMLElement): void {
  const options = [...el.querySelectorAll<HTMLElement>(".grouping-switch__option")];
  const rows = [...el.querySelectorAll<HTMLElement>(".issue")];

  const labelOf = (key: Grouping, value: string, first: HTMLElement): string => {
    if (key === "protocol") return PROTOCOL_LABEL[value] ?? value;
    if (key === "owner") return first.dataset.ownerLabel ?? value;
    return value;
  };

  const regroup = (key: Grouping): void => {
    const sorted = [...rows].sort(compare);
    const groups = new Map<string, HTMLElement[]>();
    for (const row of sorted) {
      const value = row.dataset[key] ?? "";
      const members = groups.get(value);
      if (members) members.push(row);
      else groups.set(value, [row]);
    }
    const sections: HTMLElement[] = [];
    for (const [value, members] of groups) {
      const section = document.createElement("section");
      section.className = "issue-group";
      section.dataset.group = value;
      const head = document.createElement("header");
      head.className = "issue-group__head";
      const h3 = document.createElement("h3");
      h3.className = "issue-group__label";
      h3.textContent = labelOf(key, value, members[0]);
      const count = document.createElement("span");
      count.className = "issue-group__count";
      count.textContent = members.length === 1 ? "1 line" : `${members.length} lines`;
      head.append(h3, count);
      section.append(head, ...members);
      sections.push(section);
    }
    for (const old of el.querySelectorAll("section.issue-group")) old.remove();
    el.append(...sections);
    for (const o of options) o.setAttribute("aria-pressed", String(o.dataset.group === key));
  };

  el.dataset.ready = "";

  const pressed = options.find((o) => o.getAttribute("aria-pressed") === "true")?.dataset.group;
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(KEY);
  } catch {
    stored = null;
  }
  if (isGrouping(stored) && stored !== pressed) regroup(stored);

  for (const o of options) {
    o.addEventListener("click", () => {
      const key = o.dataset.group ?? null;
      if (!isGrouping(key)) return;
      regroup(key);
      try {
        localStorage.setItem(KEY, key);
      } catch {
        // storage unavailable: the grouping holds for this page only
      }
    });
  }
}
