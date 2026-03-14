import { storageGet, storageSet, storageRemove } from '@netray-info/common-frontend/storage';

const STORAGE_KEY = 'tlsight_history';
const MAX_ENTRIES = 20;

export interface HistoryEntry {
  query: string;
  timestamp: number;
}

export function getHistory(): HistoryEntry[] {
  return storageGet<HistoryEntry[]>(STORAGE_KEY, []);
}

export function addToHistory(query: string): void {
  const entries = getHistory().filter(e => e.query !== query);
  entries.unshift({ query, timestamp: Date.now() });
  if (entries.length > MAX_ENTRIES) entries.length = MAX_ENTRIES;
  storageSet(STORAGE_KEY, entries);
}

export function clearHistory(): void {
  storageRemove(STORAGE_KEY);
}
