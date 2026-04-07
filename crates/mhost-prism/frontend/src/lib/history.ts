import { createQueryHistory } from '@netray-info/common-frontend/history';

export type { HistoryEntry } from '@netray-info/common-frontend/history';

const { getHistory, addToHistory, clearHistory } = createQueryHistory('prism_history', 50);
export { getHistory, addToHistory, clearHistory };
