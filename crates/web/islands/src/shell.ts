import { closeHelp, exportClick, focusHelpOpenerOnClose, helpIsOpen, isTextTarget, openHelp, unhide } from "./controls";

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
  // The autocomplete hook: a view names its provider in data-suggest; a provider returns the
  // suggestions for the token at the caret. DNS is the one provider today.
  const providers: Record<string, (tok: string) => string[]> = {
    dns: (tok) =>
      (tok[0] === "@" ? servers : tok[0] === "+" ? flags : types).filter((s) =>
        s.toLowerCase().startsWith(tok),
      ),
  };
  const flags = words(el.dataset.suggestFlags);
  const servers = words(el.dataset.suggestServers).map((s) => "@" + s);
  let restoring = false;
  const parseUrl = el.dataset.suggestParse;
  const hint = el.querySelector<HTMLElement>("#inspector-parse-hint");
  let hintFor = "";
  let gen = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let mode = "";
  let sel = -1;
  let span = [0, 0];

  const sync = (): void => {
    if (clear) clear.hidden = input.value === "";
    // A parse hint belongs to the input it was given for.
    if (hint && input.value !== hintFor) {
      hint.textContent = "";
      hint.hidden = true;
    }
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
    // A parse answer still on its way is for a list that no longer shows.
    gen++;
    clearTimeout(timer);
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
    const provider = providers[el.dataset.suggest ?? ""];
    if (!provider || start === 0 || start > caret || !tok) return close();
    span = [start, stop < 0 ? v.length : stop];
    // Nothing preselected: Enter keeps submitting the typed query; Tab takes the first.
    open("s", provider(tok), false, -1);
  };
  // The parse endpoint (DNS only): 150 ms after the last keystroke past the domain it answers with
  // completions and the parse error. Any failure changes nothing; an answer that arrives after the
  // input changed, the list closed or a row was selected is dropped.
  const ask = (): void => {
    clearTimeout(timer);
    if (!parseUrl || !input.value) return;
    const v = input.value;
    const caret = input.selectionStart ?? v.length;
    const start = v.lastIndexOf(" ", caret - 1) + 1;
    if (start === 0) return;
    const tok = v.slice(start, caret).toLowerCase();
    const g = ++gen;
    timer = setTimeout(async () => {
      try {
        const res = await fetch(parseUrl, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            input: v,
            cursor_pos: new TextEncoder().encode(v.slice(0, caret)).length,
          }),
        });
        if (!res.ok) return;
        const body = await res.json();
        if (g !== gen || input.value !== v) return;
        const err = typeof body?.error === "string" ? body.error : "";
        if (hint) {
          hintFor = v;
          hint.textContent = err;
          hint.hidden = !err;
        }
        const hits: string[] = (Array.isArray(body?.completions) ? body.completions : [])
          .map((c: { label?: unknown }) => c?.label)
          .filter((l: unknown): l is string => typeof l === "string" && l.toLowerCase().startsWith(tok));
        // The parser's completions first, then the table's the parser does not list.
        const table = tok ? (providers[el.dataset.suggest ?? ""]?.(tok) ?? []) : [];
        const merged = [...hits, ...table.filter((t) => !hits.includes(t))];
        const showing = list && !list.hidden && mode === "s" && sel === -1;
        if (hits.length && tok && showing && document.activeElement === input) open("s", merged, false, -1);
      } catch {
        /* the table suggestions stay */
      }
    }, 150);
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
    ask();
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
  unhide(el);
  el.addEventListener("netray:stream-fragment", () => unhide(el));
  el.addEventListener("netray:stream-end", () => {
    const u = shareUrl();
    if (u) {
      history.replaceState(null, "", u);
      current = u;
    }
  });

  focusHelpOpenerOnClose(el);

  const step = (dir: number): void => {
    const rows = [...document.querySelectorAll<HTMLElement>("#inspect-result .check-row, #inspect-result .evidence-block, #inspect-result .protocol-view")];
    // The item holding focus, also when focus sits on a link inside it.
    const active = document.activeElement;
    const at = rows.findIndex((row) => row === active || row.contains(active));
    const next = rows[Math.max(0, Math.min(rows.length - 1, at + dir))];
    if (!next) return;
    next.setAttribute("tabindex", "-1");
    next.focus();
  };
  const tab = (dir: number): void => {
    const tabs = [...el.querySelectorAll<HTMLAnchorElement>("nav.inspector-tabs a.inspector-tabs__tab")];
    const at = tabs.findIndex((a) => a.getAttribute("aria-current") === "page");
    const next = tabs[at + dir];
    if (at >= 0 && next) location.assign(next.getAttribute("href") ?? "");
  };
  document.addEventListener("keydown", (ev) => {
    if (!el.isConnected) return;
    const target = ev.target as HTMLElement | null;
    if (isTextTarget(target)) return;
    if (helpIsOpen(el) && ev.key !== "Escape") return;
    if ((ev.ctrlKey || ev.metaKey) && !ev.altKey && !ev.shiftKey && ev.key.toLowerCase() === "l") {
      ev.preventDefault();
      input.focus();
      input.select();
      return;
    }
    if (ev.ctrlKey || ev.metaKey || ev.altKey) return;
    if (ev.key === "Escape") {
      if (helpIsOpen(el)) closeHelp(el);
      else close();
      return;
    }
    switch (ev.key) {
      case "/":
        ev.preventDefault();
        input.focus();
        break;
      case "r":
        // A shown run re-runs as it was asked, an empty IP input (the caller's own address) too.
        if (input.value.trim() || el.querySelector("#inspect-result .inspector-run__head")) form.requestSubmit();
        break;
      case "j":
        step(1);
        break;
      case "k":
        step(-1);
        break;
      case "Enter": {
        const row = target?.closest(".check-row");
        if (!row || target?.closest("a, button")) break;
        const guide = [...row.querySelectorAll<HTMLAnchorElement>(".check-row__link")]
          .reverse()
          .find((a) => (a.textContent ?? "").trim().toLowerCase().startsWith("guide"));
        const href = guide?.getAttribute("href");
        if (href) location.assign(href);
        break;
      }
      case "h":
        tab(-1);
        break;
      case "l":
        tab(1);
        break;
      case "?":
        openHelp(el);
        break;
    }
  });

  el.addEventListener("click", (ev) => {
    const target = ev.target as Element | null;
    if (target?.closest(".inspector-help__open")) return openHelp(el);
    if (target?.closest(".inspector-help__close")) {
      ev.preventDefault();
      return closeHelp(el);
    }
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
    const exporter = target?.closest<HTMLButtonElement>("button[data-export-action]");
    if (exporter) return exportClick(el, exporter);
    const chip = target?.closest<HTMLElement>("a.inspector-examples__chip[data-example]");
    if (!chip || !el.contains(chip)) return;
    if (ev.button !== 0 || ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey) return;
    ev.preventDefault();
    input.value = chip.dataset.example ?? "";
    sync();
    form.requestSubmit();
  });
}
