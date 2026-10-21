// Mirrors crates/tls/src/input.rs: `split_target_and_ports` and `parse_ports`.
// `parse_ports` trims each item and parses it as a u16 (a leading `+` included); the unbracketed
// split only happens when the suffix is digits and commas, which `target` checks first.
function portList(list: string): number[] | null {
  const items = list.split(",").map((p) => p.trim());
  if (!items.every((p) => /^\+?\d+$/.test(p))) return null;
  const nums = items.map(Number);
  return nums.every((n) => n >= 1 && n <= 65535) ? nums : null;
}

interface Target {
  host: string;
  ports: number[] | null;
  fixed: boolean;
}

function target(value: string): Target {
  const v = value.trim();
  const fixed: Target = { host: v, ports: null, fixed: true };
  if (!v) return fixed;
  if (v.startsWith("[")) {
    const end = v.indexOf("]");
    if (end < 2) return fixed;
    const host = v.slice(0, end + 1);
    const rest = v.slice(end + 1);
    if (!rest) return { host, ports: null, fixed: false };
    if (!rest.startsWith(":")) return fixed;
    const ports = portList(rest.slice(1));
    return ports ? { host, ports, fixed: false } : fixed;
  }
  const colon = v.lastIndexOf(":");
  if (colon < 0) return { host: v, ports: null, fixed: false };
  const suffix = v.slice(colon + 1);
  // Unbracketed, the server splits only at a last colon followed by digits and commas; a host
  // left with a colon (an unbracketed IPv6 address) is then no address and no name: refused.
  const ports = /^[\d,]+$/.test(suffix) ? portList(suffix) : null;
  const host = v.slice(0, colon);
  if (!ports || colon === 0 || host.includes(":")) return fixed;
  return { host, ports, fixed: false };
}

export function parsePorts(value: string): { host: string; ports: number[] | null } {
  const { host, ports } = target(value);
  return { host, ports };
}

export function formatHost(host: string, ports: number[]): string {
  if (ports.length === 1 && ports[0] === 443) return host;
  const bare = host.includes(":") && !host.startsWith("[");
  return `${bare ? `[${host}]` : host}:${ports.join(",")}`;
}

export function ports(el: HTMLElement): void {
  const form = el.closest("form");
  const h = form?.querySelector<HTMLInputElement>("input[name=h]");
  const toggles = el.querySelector<HTMLElement>(".port-chooser__toggles");
  const add = el.querySelector<HTMLElement>(".port-chooser__add");
  const menu = el.querySelector<HTMLElement>(".port-chooser__menu");
  const field = el.querySelector<HTMLInputElement>(".port-chooser__field");
  const note = el.querySelector<HTMLElement>(".port-chooser__note");
  const template = el.querySelector<HTMLTemplateElement>(".port-chooser__template");
  if (!form || !h || !toggles || !add || !menu || !field || !note || !template) return;
  const max = Number(el.dataset.maxPorts) || 7;

  const buttons = () => [...toggles.querySelectorAll<HTMLElement>(".port-chooser__port")];
  let selected: number[] = [443];
  let current: Target = target(h.value);

  const refuse = () => {
    note.textContent = `Inspect at most ${max} ports at once.`;
    note.hidden = false;
  };

  // Nothing to write the ports to yet: say so rather than appear inert.
  const needHost = (): boolean => {
    if (h.value.trim()) return false;
    note.textContent = "Enter a host first.";
    note.hidden = false;
    return true;
  };

  const show = () => {
    for (const port of selected) {
      if (buttons().some((b) => Number(b.dataset.port) === port)) continue;
      const button = template.content.firstElementChild?.cloneNode(true) as HTMLElement | undefined;
      if (!button) continue;
      button.dataset.port = String(port);
      button.textContent = String(port);
      toggles.insertBefore(button, add);
    }
    for (const b of buttons()) {
      b.setAttribute("aria-pressed", String(selected.includes(Number(b.dataset.port))));
    }
  };

  const sync = (): boolean => {
    current = target(h.value);
    // A suffix the server refuses presses nothing; an empty or bare host is the default 443.
    const list = current.ports ?? (current.fixed && h.value.trim() ? [] : [443]);
    if (list.length > max) {
      // The server refuses more ports than the limit: nothing is pressed.
      refuse();
      selected = [];
      show();
      return false;
    }
    note.hidden = true;
    selected = list.filter((p, i) => list.indexOf(p) === i);
    show();
    return true;
  };

  const choose = (list: number[]) => {
    if (list.length > max) {
      refuse();
      return;
    }
    note.hidden = true;
    selected = list;
    show();
    h.value = formatHost(current.host, selected);
  };

  const addField = () => {
    const value = field.value.trim();
    field.value = "";
    const usable = sync();
    if (!/^\d+$/.test(value)) return;
    const port = Number(value);
    if (port < 1 || port > 65535) return;
    if (needHost() || !usable || current.fixed) return;
    if (!selected.includes(port)) choose([...selected, port]);
  };

  toggles.addEventListener("click", (event) => {
    const button = (event.target as Element).closest<HTMLElement>(".port-chooser__port");
    if (!button) return;
    if (!sync() || needHost() || current.fixed) return;
    const port = Number(button.dataset.port);
    if (selected.includes(port)) {
      if (selected.length > 1) choose(selected.filter((p) => p !== port));
    } else {
      choose([...selected, port]);
    }
  });

  add.addEventListener("click", () => {
    const open = menu.hidden;
    menu.hidden = !open;
    add.setAttribute("aria-expanded", String(open));
  });

  menu.addEventListener("click", (event) => {
    const preset = (event.target as Element).closest<HTMLElement>(".port-chooser__preset");
    if (!preset) return;
    sync();
    if (needHost() || current.fixed) return;
    const list = portList(preset.dataset.ports ?? "");
    if (list) choose(list);
  });

  field.addEventListener("change", addField);
  field.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    addField();
  });

  h.addEventListener("input", sync);
  h.addEventListener("change", sync);
  form.addEventListener("submit", sync, true);
  window.addEventListener("popstate", () => {
    if (el.isConnected) sync();
  });
  sync();
  el.hidden = false;
}
