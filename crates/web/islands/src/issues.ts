const KEY = "netray.grouping";
type Grouping = "owner" | "severity" | "protocol";

const ORDER: Record<Grouping, string[]> = {
  owner: ["dns_provider", "registrar", "certificate_provider", "web_server", "mail_provider", "hosting"],
  severity: ["critical", "high", "medium", "low"],
  protocol: ["dns", "tls", "http", "email", "ip"],
};

const PROTOCOL_LABEL: Record<string, string> = {
  dns: "DNS",
  tls: "TLS",
  http: "HTTP",
  email: "email",
  ip: "IP",
};

const isGrouping = (v: string | null): v is Grouping =>
  v === "owner" || v === "severity" || v === "protocol";

function rank(row: HTMLElement): [number, number, number] {
  const sev = ORDER.severity.indexOf(row.dataset.severity ?? "");
  return [
    sev < 0 ? ORDER.severity.length : sev,
    row.dataset.status === "fail" ? 0 : 1,
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
  options[0]?.closest("[hidden]")?.removeAttribute("hidden");

  const labelOf = (key: Grouping, value: string): string => {
    if (key === "protocol") return PROTOCOL_LABEL[value] ?? value;
    if (key === "owner") {
      const row = rows.find((r) => r.dataset.owner === value);
      return row?.dataset.ownerLabel ?? value;
    }
    return value;
  };

  const regroup = (key: Grouping): void => {
    const sorted = [...rows].sort(compare);
    const sections: HTMLElement[] = [];
    for (const value of ORDER[key]) {
      const members = sorted.filter((r) => r.dataset[key] === value);
      if (members.length === 0) continue;
      const section = document.createElement("section");
      section.className = "issue-group";
      section.dataset.group = value;
      const h3 = document.createElement("h3");
      h3.textContent = labelOf(key, value);
      section.append(h3, ...members);
      sections.push(section);
    }
    for (const old of el.querySelectorAll("section.issue-group")) old.remove();
    el.append(...sections);
    for (const o of options) o.setAttribute("aria-pressed", String(o.dataset.group === key));
  };

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
