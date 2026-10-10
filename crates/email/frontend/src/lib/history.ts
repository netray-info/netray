import { createQueryHistory } from '@netray-info/common-frontend/history';

export type { HistoryEntry } from '@netray-info/common-frontend/history';

export const STORAGE_KEY = 'beacon_history';
export const MAX_ENTRIES = 20;

const { getHistory, addToHistory, clearHistory } = createQueryHistory(STORAGE_KEY, MAX_ENTRIES);
export { getHistory, addToHistory, clearHistory };
