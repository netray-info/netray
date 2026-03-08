import { createSignal, Show, For } from 'solid-js';
import { getHistory, clearHistory, type HistoryEntry } from '../lib/history';

interface Props {
  onSelect: (query: string) => void;
}

export default function QueryHistory(props: Props) {
  const [entries, setEntries] = createSignal<HistoryEntry[]>(getHistory());
  const [visible, setVisible] = createSignal(false);

  const refresh = () => setEntries(getHistory());

  const handleClear = () => {
    clearHistory();
    setEntries([]);
  };

  return (
    <Show when={entries().length > 0}>
      <div class="query-history">
        <button class="detail-toggle" onClick={() => { refresh(); setVisible(v => !v); }}>
          {visible() ? 'hide' : 'history'} ({entries().length})
        </button>
        <Show when={visible()}>
          <div class="query-history__list">
            <For each={entries()}>
              {(entry) => (
                <button class="query-history__item" onClick={() => props.onSelect(entry.query)}>
                  <span class="query-history__query mono">{entry.query}</span>
                  <span class="query-history__time">{new Date(entry.timestamp).toLocaleDateString()}</span>
                </button>
              )}
            </For>
            <button class="query-history__clear" onClick={handleClear}>clear history</button>
          </div>
        </Show>
      </div>
    </Show>
  );
}
