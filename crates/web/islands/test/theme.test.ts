// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "../src/main";

const root = document.documentElement;
const frame = () => new Promise((r) => requestAnimationFrame(() => r(null)));

function build(): HTMLButtonElement {
  document.body.innerHTML = `<button data-island="theme">Theme</button>`;
  return document.querySelector("button") as HTMLButtonElement;
}

function memoryStorage(): Storage {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => (m.has(k) ? (m.get(k) as string) : null),
    setItem: (k: string, v: string) => void m.set(k, String(v)),
    removeItem: (k: string) => void m.delete(k),
    clear: () => m.clear(),
    key: (i: number) => [...m.keys()][i] ?? null,
    get length() {
      return m.size;
    },
  } as Storage;
}

function useStorage(s: Storage) {
  vi.stubGlobal("localStorage", s);
  Object.defineProperty(window, "localStorage", { value: s, configurable: true });
}

function stored(): string | null {
  return localStorage.getItem("theme");
}

beforeEach(() => {
  useStorage(memoryStorage());
  root.removeAttribute("data-theme");
  document.body.innerHTML = "";
});
afterEach(() => {
  vi.unstubAllGlobals();
  root.removeAttribute("data-theme");
  document.body.innerHTML = "";
});

describe("theme island", () => {
  it("cycles auto -> light -> dark -> auto, setting data-theme and storing the choice", async () => {
    const btn = build();
    mount(document.body);
    await frame();

    btn.click();
    expect(root.getAttribute("data-theme")).toBe("light");
    expect(stored()).toBe("light");

    btn.click();
    expect(root.getAttribute("data-theme")).toBe("dark");
    expect(stored()).toBe("dark");

    btn.click();
    expect(root.hasAttribute("data-theme")).toBe(false);
    expect([null, "auto"]).toContain(stored());
  });

  it("applies a stored choice on mount and continues the cycle from it", async () => {
    localStorage.setItem("theme", "dark");
    const btn = build();
    mount(document.body);
    await vi.waitFor(() => expect(root.getAttribute("data-theme")).toBe("dark"));

    btn.click();
    expect(root.hasAttribute("data-theme")).toBe(false);
  });

  it("ignores a throwing storage", async () => {
    const boom = () => {
      throw new Error("storage denied");
    };
    const broken = { getItem: boom, setItem: boom, removeItem: boom, clear: boom, key: boom, length: 0 };
    useStorage(broken as unknown as Storage);

    const btn = build();
    expect(() => mount(document.body)).not.toThrow();
    await frame();
    expect(() => btn.click()).not.toThrow();
    expect(root.getAttribute("data-theme")).toBe("light");
  });
});
