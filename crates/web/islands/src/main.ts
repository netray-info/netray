import { countdown } from "./countdown";
import { dns } from "./dns";
import { issues } from "./issues";
import { page } from "./page";
import { recent } from "./recent";
import { share } from "./share";
import { shell } from "./shell";
import { spectrum } from "./spectrum";
import { status } from "./status";
import { tabs } from "./tabs";
import { stream } from "./stream";
import { theme } from "./theme";

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
register("countdown", countdown);
register("dns", dns);
register("issues", issues);
register("page", page);
register("recent", recent);
register("share", share);
register("shell", shell);
register("spectrum", spectrum);
register("status", status);
register("tabs", tabs);
register("theme", theme);

mount(document);
