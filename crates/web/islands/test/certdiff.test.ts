// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { certdiff, describeChange } from "../src/certdiff";

type Snap = { notAfter: string; issuer: string; sans: string[]; sha256: string };

const base: Snap = {
  notAfter: "2026-12-25T00:00:00Z",
  issuer: "CN=R3",
  sans: ["example.com", "www.example.com"],
  sha256: "aa",
};

describe("describeChange", () => {
  it.each<[string, Partial<Snap>, string[] | null]>([
    ["equal sha256", { notAfter: "2030-01-01T00:00:00Z", issuer: "CN=X", sans: [] }, null],
    ["expiry", { sha256: "bb", notAfter: "2027-03-25T00:00:00Z" }, ["expiry 2026-12-25 → 2027-03-25"]],
    ["issuer", { sha256: "bb", issuer: "CN=E5" }, ["issuer CN=R3 → CN=E5"]],
    ["names added", { sha256: "bb", sans: ["example.com", "www.example.com", "a.example.com", "b.example.com"] }, ["names added: a.example.com, b.example.com"]],
    ["names removed", { sha256: "bb", sans: ["example.com"] }, ["names removed: www.example.com"]],
    ["only fingerprint", { sha256: "bb" }, ["same names and issuer, new key"]],
    [
      "expiry and issuer together",
      { sha256: "bb", notAfter: "2027-03-25T00:00:00Z", issuer: "CN=E5" },
      ["expiry 2026-12-25 → 2027-03-25", "issuer CN=R3 → CN=E5"],
    ],
  ])("%s", (_name, change, want) => {
    expect(describeChange(base, { ...base, ...change })).toEqual(want);
  });
});

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

// The island memoises the first comparison per key+sha256 for the page, so every test uses its own key.
let n = 0;
let key = "";

function el(opts: { shell?: boolean; sha?: string; notAfter?: string; sans?: string } = {}): HTMLElement {
  const { shell = true, sha = "bb", notAfter = "2027-03-25T00:00:00Z", sans = "example.com" } = opts;
  const html = `<div class="tls-certdiff" data-island="certdiff" data-cert-key="${key}" data-not-after="${notAfter}" data-issuer="CN=R3" data-sans="${sans}" data-sha256="${sha}" hidden>
    <template class="tls-certdiff__template"><div class="tls-certdiff__banner" role="status"><p>Certificate changed since your last visit: <span class="tls-certdiff__changes"></span></p><button type="button" class="tls-certdiff__forget">Forget</button></div></template></div>`;
  // A live run of this page: the stream island's form carries its state.
  const form = '<form data-stream-state="done"></form>';
  document.body.innerHTML = shell ? `<div class="inspector-shell">${form}${html}</div>` : html;
  return document.querySelector(".tls-certdiff") as HTMLElement;
}

// A part the stream sends again: fresh server markup for the same key and sha256.
function remount(): HTMLElement {
  return el();
}

const storeKey = () => `netray.cert.${key}`;
const old: Snap = { ...base, sha256: "aa" };

beforeEach(() => {
  useStorage(memoryStorage());
  n += 1;
  key = `example.com:443:${n}`;
  document.body.innerHTML = "";
});
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("certdiff island", () => {
  it("shows the changes against a stored snapshot of another sha and stores the new one", () => {
    localStorage.setItem(storeKey(), JSON.stringify(old));
    const e = el({ sans: "example.com" });
    certdiff(e);
    expect(e.hidden).toBe(false);
    const text = e.querySelector(".tls-certdiff__changes")?.textContent ?? "";
    expect(text).toContain("expiry 2026-12-25 → 2027-03-25");
    expect(text).toContain("names removed: www.example.com");
    const now = JSON.parse(localStorage.getItem(storeKey()) as string);
    expect(now.sha256).toBe("bb");
    expect(now.notAfter).toBe("2027-03-25T00:00:00Z");
  });

  it("names a SAN added against a stored snapshot with one SAN fewer", () => {
    localStorage.setItem(
      storeKey(),
      JSON.stringify({ ...old, sans: ["example.com"], notAfter: "2026-12-25T00:00:00Z" }),
    );
    const e = el({ sans: "example.com\nwww.example.com" });
    certdiff(e);
    expect(e.hidden).toBe(false);
    const text = e.querySelector(".tls-certdiff__changes")?.textContent ?? "";
    expect(text).toContain("expiry 2026-12-25 → 2027-03-25");
    expect(text).toContain("names added: www.example.com");
  });

  it("stays hidden without a banner on the same sha256", () => {
    localStorage.setItem(storeKey(), JSON.stringify({ ...old, sha256: "bb" }));
    const e = el();
    certdiff(e);
    expect(e.hidden).toBe(true);
    expect(e.querySelector(".tls-certdiff__banner")).toBeNull();
  });

  it("stores the snapshot and stays hidden when none was stored", () => {
    const e = el();
    certdiff(e);
    expect(e.hidden).toBe(true);
    expect(e.querySelector(".tls-certdiff__banner")).toBeNull();
    expect(JSON.parse(localStorage.getItem(storeKey()) as string).sha256).toBe("bb");
  });

  it("Forget removes the stored key and hides the element again", () => {
    localStorage.setItem(storeKey(), JSON.stringify(old));
    const e = el();
    certdiff(e);
    expect(e.hidden).toBe(false);
    (e.querySelector(".tls-certdiff__forget") as HTMLButtonElement).click();
    expect(localStorage.getItem(storeKey())).toBeNull();
    expect(e.hidden).toBe(true);
    expect(e.querySelector(".tls-certdiff__banner")).toBeNull();
  });

  it("stays hidden and does not throw when storage throws", () => {
    const boom = () => {
      throw new Error("denied");
    };
    useStorage({ ...memoryStorage(), getItem: boom, setItem: boom, removeItem: boom } as Storage);
    const e = el();
    expect(() => certdiff(e)).not.toThrow();
    expect(e.hidden).toBe(true);
    expect(e.querySelector(".tls-certdiff__banner")).toBeNull();
  });

  it("does nothing outside an .inspector-shell", () => {
    const set = vi.spyOn(localStorage, "setItem");
    localStorage.setItem(storeKey(), JSON.stringify(old));
    set.mockClear();
    const e = el({ shell: false });
    certdiff(e);
    expect(e.hidden).toBe(true);
    expect(set).not.toHaveBeenCalled();
    expect(JSON.parse(localStorage.getItem(storeKey()) as string).sha256).toBe("aa");
  });

  it("a second run of the same certificate in the page shows no banner", () => {
    localStorage.setItem(storeKey(), JSON.stringify(old));
    const first = el();
    certdiff(first);
    expect(first.hidden).toBe(false);

    const second = remount();
    certdiff(second);
    expect(second.hidden).toBe(true);
  });

  it("a shared run (no live stream in the page) neither compares nor stores", () => {
    localStorage.setItem(storeKey(), JSON.stringify(old));
    const e = el();
    document.querySelector("form[data-stream-state]")?.removeAttribute("data-stream-state");
    certdiff(e);
    expect(e.hidden).toBe(true);
    expect(JSON.parse(localStorage.getItem(storeKey()) ?? "{}").sha256).toBe("aa");
  });

  it("names no expiry change when only the time of day differs", () => {
    const a = { ...base, sha256: "aa", notAfter: "2027-01-06T10:00:00Z" };
    const b = { ...base, sha256: "bb", notAfter: "2027-01-06T12:00:00Z" };
    expect(describeChange(a, b)).toEqual(["same names and issuer, new key"]);
  });
});
