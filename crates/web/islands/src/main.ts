import { stream } from "./stream";

type Island = (el: HTMLElement) => void;

const islands = new Map<string, Island>();
const mounted = new WeakSet<Element>();

export function register(name: string, island: Island): void {
  islands.set(name, island);
}

export function mount(root: ParentNode): void {
  const found: Element[] = [];
  if (root instanceof Element && root.hasAttribute("data-island")) found.push(root);
  found.push(...root.querySelectorAll("[data-island]"));
  for (const el of found) {
    if (mounted.has(el)) continue;
    const island = islands.get(el.getAttribute("data-island") ?? "");
    if (!island) continue;
    mounted.add(el);
    island(el as HTMLElement);
  }
}

register("stream", (el) => stream(el as HTMLFormElement, mount));

mount(document);
