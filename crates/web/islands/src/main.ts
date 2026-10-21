import { certdiff } from "./certdiff";
import { countdown } from "./countdown";
import { dns } from "./dns";
import { dnsview } from "./dnsview";
import { issues } from "./issues";
import { menu } from "./menu";
import { page } from "./page";
import { ports } from "./ports";
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
register("certdiff", certdiff);
register("countdown", countdown);
register("dns", dns);
register("dnsview", dnsview);
register("issues", issues);
register("nav-menu", menu);
register("page", page);
register("ports", ports);
register("recent", recent);
register("share", share);
register("shell", shell);
register("spectrum", spectrum);
register("status", status);
register("tabs", tabs);
register("theme", theme);

mount(document);
