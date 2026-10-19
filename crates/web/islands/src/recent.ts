const KEY = "netray.recent";
const MAX = 8;

type Item = { domain: string; grade: string; date: string };

const valid = (v: unknown): v is Item => {
  const i = v as Item | null;
  return (
    !!i &&
    typeof i.domain === "string" &&
    i.domain !== "" &&
    typeof i.grade === "string" &&
    typeof i.date === "string"
  );
};

function read(): Item[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter(valid).slice(0, MAX) : [];
  } catch {
    return [];
  }
}

function write(item: Item): void {
  try {
    const rest = read().filter((i) => i.domain !== item.domain);
    localStorage.setItem(KEY, JSON.stringify([item, ...rest].slice(0, MAX)));
  } catch {
    // storage unavailable: nothing is remembered
  }
}

function record(el: HTMLElement): void {
  const domain = el.dataset.domain;
  const grade = el.querySelector<HTMLElement>("#result-grade");
  const letter = grade?.dataset.grade;
  const date = grade?.dataset.date;
  if (!domain || !letter || !date || letter === "incomplete") return;
  write({ domain, grade: letter, date });
}

function list(el: HTMLElement): void {
  const items = read();
  if (items.length === 0) return;
  el.replaceChildren(
    ...items.map((item) => {
      const li = document.createElement("li");
      const a = document.createElement("a");
      a.href = `/${item.domain}`;
      a.textContent = item.domain;
      li.append(a, ` ${item.grade} ${item.date}`);
      return li;
    }),
  );
  el.removeAttribute("hidden");
}

export function recent(el: HTMLElement): void {
  if (el.id !== "result") {
    list(el);
    return;
  }
  if (el.dataset.state === "fresh") record(el);
  document.addEventListener("netray:stream-end", () => record(el));
}
