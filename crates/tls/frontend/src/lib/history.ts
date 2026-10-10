import { createQueryHistory } from '@netray-info/common-frontend/history';

export type { HistoryEntry } from '@netray-info/common-frontend/history';

const { getHistory, addToHistory, clearHistory } = createQueryHistory('tlsight_history', 20);
export { getHistory, addToHistory, clearHistory };
