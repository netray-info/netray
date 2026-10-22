type Snapshot = { notAfter: string; issuer: string; sans: string[]; sha256: string };

const PREFIX = "netray.cert.";

export function describeChange(old: Snapshot, now: Snapshot): string[] | null {
  if (old.sha256 === now.sha256) return null;
  const out: string[] = [];
  if (old.notAfter.slice(0, 10) !== now.notAfter.slice(0, 10)) {
    out.push(`expiry ${old.notAfter.slice(0, 10)} → ${now.notAfter.slice(0, 10)}`);
  }
  if (old.issuer !== now.issuer) out.push(`issuer ${old.issuer} → ${now.issuer}`);
  const added = now.sans.filter((n) => !old.sans.includes(n));
  const removed = old.sans.filter((n) => !now.sans.includes(n));
  if (added.length) out.push(`names added: ${added.join(", ")}`);
  if (removed.length) out.push(`names removed: ${removed.join(", ")}`);
  return out.length ? out : ["same names and issuer, new key"];
}

function parse(raw: string | null): Snapshot | null {
  if (!raw) return null;
  try {
    const v = JSON.parse(raw) as Partial<Snapshot>;
    if (
      typeof v?.notAfter !== "string" ||
      typeof v.issuer !== "string" ||
      !Array.isArray(v.sans) ||
      typeof v.sha256 !== "string"
    ) {
      return null;
    }
    return { notAfter: v.notAfter, issuer: v.issuer, sans: v.sans.map(String), sha256: v.sha256 };
  } catch {
    return null;
  }
}

function compare(storeKey: string, now: Snapshot): string[] | null {
  let old: Snapshot | null = null;
  try {
    old = parse(globalThis.localStorage.getItem(storeKey));
  } catch {
    old = null;
  }
  try {
    globalThis.localStorage.setItem(storeKey, JSON.stringify(now));
  } catch {
    /* storage unavailable */
  }
  return old ? describeChange(old, now) : null;
}

export function certdiff(el: HTMLElement): void {
  // Only a run of this page compares: a shared run (`?r=`) renders a stored certificate, which
  // is no visit of the live one.
  if (!el.closest(".inspector-shell")) return;
  const state = document.querySelector<HTMLElement>("form[data-stream-state]")?.dataset.streamState;
  if (state !== "running" && state !== "done") return;
  const d = el.dataset;
  const key = d.certKey ?? "";
  const now: Snapshot = {
    notAfter: d.notAfter ?? "",
    issuer: d.issuer ?? "",
    sans: (d.sans ?? "").split("\n").filter((n) => n !== ""),
    sha256: d.sha256 ?? "",
  };
  if (!key || !now.sha256) return;
  const storeKey = PREFIX + key;
  const changes = compare(storeKey, now);
  const template = el.querySelector<HTMLTemplateElement>("template.tls-certdiff__template");
  if (!changes || !template) return;
  const banner = (template.content.cloneNode(true) as DocumentFragment).querySelector<HTMLElement>(
    ".tls-certdiff__banner",
  );
  if (!banner) return;
  const list = banner.querySelector(".tls-certdiff__changes");
  if (list) list.textContent = changes.join("; ");
  banner.querySelector(".tls-certdiff__forget")?.addEventListener("click", () => {
    try {
      globalThis.localStorage.removeItem(storeKey);
    } catch {
      /* storage unavailable */
    }
    banner.remove();
    el.hidden = true;
  });
  el.appendChild(banner);
  el.hidden = false;
}
