const HISTORY_MAX = 20;

export function shell(el: HTMLElement): void {
  const form = el.querySelector("form");
  const input = el.querySelector<HTMLInputElement>(".query-input__field");
  if (!form || !input) return;
  const clear = el.querySelector<HTMLElement>(".query-input__clear");
  const list = el.querySelector<HTMLElement>("#inspector-suggest");
  const tool = el.dataset.tool ?? "";
  const field = el.dataset.field ?? "";
  const storeKey = "netray.history." + tool;
  const words = (v: string | undefined): string[] => (v ?? "").split(" ").filter(Boolean);
  const types = words(el.dataset.suggestTypes);
  const flags = words(el.dataset.suggestFlags);
  const servers = words(el.dataset.suggestServers).map((s) => "@" + s);
  let restoring = false;
  let mode = "";
  let sel = -1;
  let span = [0, 0];

  const sync = (): void => {
    if (clear) clear.hidden = input.value === "";
  };
  const load = (): string[] => {
    try {
      const a = JSON.parse(localStorage.getItem(storeKey) ?? "[]");
      return Array.isArray(a) ? a.filter((x) => typeof x === "string") : [];
    } catch {
      return [];
    }
  };
  const remember = (q: string): void => {
    try {
      localStorage.setItem(
        storeKey,
        JSON.stringify([q, ...load().filter((x) => x !== q)].slice(0, HISTORY_MAX)),
      );
    } catch {
      /* storage unavailable */
    }
  };
  const mark = (i: number): void => {
    sel = i;
    [...(list?.children ?? [])].forEach((li, j) => li.setAttribute("aria-selected", String(j === i)));
  };
  const close = (): void => {
    if (list) list.hidden = true;
    sel = -1;
  };
  const open = (m: string, texts: string[], withClear: boolean, first: number): void => {
    if (!list || !texts.length) return close();
    mode = m;
    list.replaceChildren(
      ...texts.map((t) => {
        const li = document.createElement("li");
        li.setAttribute("role", "option");
        li.textContent = t;
        return li;
      }),
    );
    if (withClear) {
      const li = document.createElement("li");
      li.setAttribute("role", "option");
      li.dataset.action = "clear";
      li.textContent = "Clear history";
      list.append(li);
    }
    list.hidden = false;
    mark(first);
  };
  const openHistory = (): void => open("h", load(), true, -1);
  const suggest = (): void => {
    const v = input.value;
    const caret = input.selectionStart ?? v.length;
    const start = v.lastIndexOf(" ", caret - 1) + 1;
    const stop = v.indexOf(" ", caret);
    const tok = v.slice(start, caret).toLowerCase();
    if (!el.dataset.suggest || start === 0 || start > caret || !tok) return close();
    span = [start, stop < 0 ? v.length : stop];
    const pool = tok[0] === "@" ? servers : tok[0] === "+" ? flags : types;
    // Nothing preselected: Enter keeps submitting the typed query; Tab takes the first.
    open("s", pool.filter((s) => s.toLowerCase().startsWith(tok)), false, -1);
  };
  const pick = (i: number): void => {
    const li = list?.children[i] as HTMLElement | undefined;
    if (!li) return;
    const text = li.textContent ?? "";
    if (li.dataset.action === "clear") {
      try {
        localStorage.removeItem(storeKey);
      } catch {
        /* storage unavailable */
      }
      return close();
    }
    close();
    if (mode === "h") {
      input.value = text;
      sync();
      form.requestSubmit();
      return;
    }
    input.value = input.value.slice(0, span[0]) + text + input.value.slice(span[1]);
    input.setSelectionRange(span[0] + text.length, span[0] + text.length);
    sync();
  };

  sync();
  input.addEventListener("input", () => {
    sync();
    if (input.value) suggest();
    else openHistory();
  });
  input.addEventListener("focus", () => {
    if (!input.value) openHistory();
  });
  input.addEventListener("blur", close);
  input.addEventListener("keydown", (ev) => {
    if (!list || list.hidden) return;
    const last = list.children.length - 1;
    if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
      ev.preventDefault();
      mark(Math.max(0, Math.min(last, sel + (ev.key === "ArrowDown" ? 1 : -1))));
    } else if (ev.key === "Escape") {
      close();
    } else if (ev.key === "Tab" && mode === "s" && last >= 0) {
      ev.preventDefault();
      pick(sel >= 0 ? sel : 0);
    } else if (sel >= 0 && ev.key === "Enter") {
      ev.preventDefault();
      pick(sel);
    }
  });
  list?.addEventListener("mousedown", (ev) => ev.preventDefault());
  list?.addEventListener("click", (ev) => {
    const li = (ev.target as Element | null)?.closest("li");
    if (li) pick([...(list.children as HTMLCollection)].indexOf(li));
  });
  clear?.addEventListener("click", () => {
    input.value = "";
    sync();
    input.focus();
  });

  // The path and query the shell last put in the address bar: a popstate that keeps them (an
  // in-page `#ev-…` link) restores nothing.
  let current = location.pathname + location.search;
  form.addEventListener("submit", () => {
    const v = input.value.trim();
    close();
    if (tool && !restoring) {
      const url = "/inspect/" + tool + (v ? "?" + field + "=" + encodeURIComponent(v) : "");
      if (current !== url) history.pushState({ q: v }, "", url);
      current = url;
    }
    if (v) remember(v);
  });
  window.addEventListener("popstate", () => {
    if (!el.isConnected) return;
    const here = location.pathname + location.search;
    if (here === current) return;
    current = here;
    const p = new URLSearchParams(location.search);
    const v = p.get(field);
    // A shared run or the idle page is the server's to render: never an empty query.
    if (v === null) return location.reload();
    input.value = v;
    sync();
    restoring = true;
    try {
      form.requestSubmit();
    } finally {
      restoring = false;
    }
  });

  const shareUrl = (): string | undefined =>
    el.querySelector<HTMLElement>("#inspect-share[data-share-url]")?.dataset.shareUrl;
  const unhide = (): void =>
    el.querySelectorAll<HTMLElement>(".inspector-share__copy").forEach((b) => (b.hidden = false));
  unhide();
  el.addEventListener("netray:stream-fragment", unhide);
  el.addEventListener("netray:stream-end", () => {
    const u = shareUrl();
    if (u) {
      history.replaceState(null, "", u);
      current = u;
    }
  });

  el.addEventListener("click", (ev) => {
    const target = ev.target as Element | null;
    if (target?.closest(".inspector-share__copy")) {
      const u = shareUrl();
      const url = u ? location.origin + u : location.href;
      try {
        if (navigator.share && matchMedia("(pointer: coarse)").matches) {
          void navigator.share({ url }).catch(() => {});
        } else {
          void navigator.clipboard?.writeText(url).catch(() => {});
        }
      } catch {
        /* clipboard unavailable */
      }
      return;
    }
    const chip = target?.closest<HTMLElement>("a.inspector-examples__chip[data-example]");
    if (!chip || !el.contains(chip)) return;
    if (ev.button !== 0 || ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey) return;
    ev.preventDefault();
    input.value = chip.dataset.example ?? "";
    sync();
    form.requestSubmit();
  });
}
