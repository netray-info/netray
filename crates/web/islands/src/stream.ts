type Mount = (root: ParentNode) => void;

interface Sse {
  event: string;
  data: string;
}

const LINE_END = /\r\n|\n|\r/;

function parser(dispatch: (e: Sse) => void): (text: string) => void {
  let buf = "";
  let event = "";
  let data: string[] = [];

  const line = (l: string): void => {
    if (l === "") {
      if (event !== "" || data.length > 0) dispatch({ event: event || "message", data: data.join("\n") });
      event = "";
      data = [];
      return;
    }
    if (l.startsWith(":")) return;
    const i = l.indexOf(":");
    const field = i < 0 ? l : l.slice(0, i);
    let value = i < 0 ? "" : l.slice(i + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    if (field === "event") event = value;
    else if (field === "data") data.push(value);
  };

  return (text) => {
    buf += text;
    for (;;) {
      const m = LINE_END.exec(buf);
      if (!m) return;
      if (m[0] === "\r" && m.index + 1 === buf.length) return;
      const l = buf.slice(0, m.index);
      buf = buf.slice(m.index + m[0].length);
      line(l);
    }
  };
}

const SVG_NS = "http://www.w3.org/2000/svg";

function parse(target: Element, html: string): DocumentFragment {
  if (target.namespaceURI === SVG_NS) {
    const tpl = document.createElement("template");
    tpl.innerHTML = `<svg>${html}</svg>`;
    const out = document.createDocumentFragment();
    out.append(...(tpl.content.firstElementChild?.childNodes ?? []));
    return out;
  }
  const range = document.createRange();
  range.selectNode(target);
  return range.createContextualFragment(html);
}

export function stream(form: HTMLFormElement, mount: Mount): void {
  const autostart = form.getAttribute("data-stream-autostart");
  let current: AbortController | null = null;

  const setState = (state: string): void => form.setAttribute("data-stream-state", state);
  const errorBox = (): Element | null => form.querySelector("[data-stream-error]");

  const apply = (payload: string, live: () => boolean): void => {
    if (!live()) return;
    let id: unknown;
    let html: unknown;
    try {
      ({ id, html } = JSON.parse(payload));
    } catch {
      return;
    }
    if (typeof id !== "string" || typeof html !== "string") return;
    const target = document.getElementById(id);
    if (!target) return;
    const fragment = parse(target, html);
    for (const s of fragment.querySelectorAll("script")) s.remove();
    const inserted = Array.from(fragment.children);
    target.replaceWith(fragment);
    for (const el of inserted) {
      mount(el);
      el.dispatchEvent(new CustomEvent("netray:stream-fragment", { bubbles: true, detail: id }));
    }
  };

  const run = async (trigger: string | null): Promise<void> => {
    current?.abort();
    const ctl = new AbortController();
    current = ctl;
    const live = (): boolean => current === ctl;

    const body = new URLSearchParams();
    for (const [k, v] of new FormData(form)) {
      if (typeof v === "string" && k !== "trigger") body.append(k, v);
    }
    if (trigger !== null) body.append("trigger", trigger);

    setState("running");
    errorBox()?.setAttribute("hidden", "");
    // A refused job answers one fragment; elements a started job re-sends (the inspector's
    // back-link) are emptied first, so a refusal never leaves the previous target's (v2-cutover C12).
    for (const el of document.querySelectorAll("[data-stream-reset]")) el.replaceChildren();

    let ended = false;
    try {
      const res = await fetch(form.getAttribute("data-stream-url") ?? form.getAttribute("action") ?? "", {
        method: "POST",
        headers: { Accept: "text/event-stream" },
        credentials: "same-origin",
        body,
        signal: ctl.signal,
      });
      if (!live()) return;
      if (!(res.headers.get("content-type") ?? "").startsWith("text/event-stream") || !res.body) {
        throw new Error("not an event stream");
      }
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      const feed = parser((e) => {
        if (ended || !live()) return;
        if (e.event === "fragment") apply(e.data, live);
        else if (e.event === "end") ended = true;
      });
      while (live() && !ended) {
        const { done, value } = await reader.read();
        if (!live()) break;
        if (done) break;
        feed(decoder.decode(value, { stream: true }));
      }
      if (!live()) {
        void reader.cancel().catch(() => {});
        return;
      }
      if (ended) void reader.cancel().catch(() => {});
    } catch {
      if (!live()) return;
    }
    if (!live()) return;
    if (ended) {
      setState("done");
      form.dispatchEvent(new CustomEvent("netray:stream-end", { bubbles: true }));
    } else {
      setState("error");
      errorBox()?.removeAttribute("hidden");
    }
  };

  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const s = (e as SubmitEvent).submitter as HTMLButtonElement | HTMLInputElement | null;
    void run(s && s.name === "trigger" ? s.value : autostart);
  });

  if (autostart !== null) {
    const start = (): void => void run(autostart);
    if ((document as Document & { prerendering?: boolean }).prerendering) {
      document.addEventListener("prerenderingchange", start, { once: true });
    } else {
      start();
    }
  }
}
