const STORAGE_KEY = 'tlsight_history';
const MAX_ENTRIES = 20;

export interface HistoryEntry {
  query: string;
  timestamp: number;
}

export function getHistory(): HistoryEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    return JSON.parse(raw);
  } catch {
    return [];
  }
}

export function addToHistory(query: string): void {
  const entries = getHistory().filter(e => e.query !== query);
  entries.unshift({ query, timestamp: Date.now() });
  if (entries.length > MAX_ENTRIES) entries.length = MAX_ENTRIES;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
}

export function clearHistory(): void {
  localStorage.removeItem(STORAGE_KEY);
}
