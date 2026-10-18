type Choice = "auto" | "light" | "dark";

const KEY = "theme";
const NEXT: Record<Choice, Choice> = { auto: "light", light: "dark", dark: "auto" };

function read(): Choice {
  try {
    const v = globalThis.localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "auto";
  } catch {
    return "auto";
  }
}

function write(choice: Choice): void {
  try {
    if (choice === "auto") globalThis.localStorage.removeItem(KEY);
    else globalThis.localStorage.setItem(KEY, choice);
  } catch {
  }
}

function apply(choice: Choice): void {
  const root = document.documentElement;
  if (choice === "auto") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", choice);
}

export function theme(el: HTMLElement): void {
  let current = read();
  requestAnimationFrame(() => apply(current));
  el.addEventListener("click", () => {
    current = NEXT[current];
    apply(current);
    write(current);
  });
}
