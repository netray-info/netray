import { storageGet, storageSet, storageRemove } from './storage.js';

export interface HistoryEntry {
  query: string;
  timestamp: number;
}

/**
 * Create history management functions bound to a specific storage key.
 * Call once at module level and export the returned functions.
 */
export function createQueryHistory(storageKey: string, maxEntries = 20) {
  function getHistory(): HistoryEntry[] {
    return storageGet<HistoryEntry[]>(storageKey, []);
  }

  function addToHistory(query: string): void {
    const entries = getHistory().filter(e => e.query !== query);
    entries.unshift({ query, timestamp: Date.now() });
    if (entries.length > maxEntries) entries.length = maxEntries;
    storageSet(storageKey, entries);
  }

  function clearHistory(): void {
    storageRemove(storageKey);
  }

  return { getHistory, addToHistory, clearHistory };
}
