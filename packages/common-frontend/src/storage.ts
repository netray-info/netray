/**
 * Type-safe localStorage helpers with try/catch error handling.
 * Falls back to the provided default value on parse errors or when the key is absent.
 */

export function storageGet<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function storageSet(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Quota exceeded or private browsing — silently ignore
  }
}

export function storageRemove(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    // Silently ignore
  }
}
