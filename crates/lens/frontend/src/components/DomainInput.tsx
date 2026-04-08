import { createSignal, createEffect, Show, For } from 'solid-js';
import { getHistory } from '../lib/history';

interface Props {
  onSubmit: (domain: string) => void;
  onClear?: () => void;
  loading: boolean;
  value?: string;
  inputRef?: (el: HTMLInputElement) => void;
}

const LISTBOX_ID = 'domain-history-listbox';

export default function DomainInput(props: Props) {
  const [value, setValue] = createSignal(props.value ?? '');
  const [historyOpen, setHistoryOpen] = createSignal(false);
  const [historyIdx, setHistoryIdx] = createSignal(-1);

  // Sync controlled value from parent (e.g. restore from URL on mount)
  createEffect(() => {
    if (props.value !== undefined) setValue(props.value);
  });

  function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    const trimmed = value().trim();
    if (!trimmed || props.loading) return;
    setHistoryOpen(false);
    props.onSubmit(trimmed);
  }

  function handleHistorySelect(query: string) {
    setValue(query);
    setHistoryOpen(false);
    props.onSubmit(query);
  }

  function handleClear() {
    setValue('');
    setHistoryOpen(false);
    setHistoryIdx(-1);
    props.onClear?.();
    document.querySelector<HTMLInputElement>('.domain-input__field')?.focus();
  }

  function handleKeyDown(e: KeyboardEvent) {
    const items = history();
    if (!showHistory() || items.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setHistoryIdx(i => Math.min(i + 1, items.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHistoryIdx(i => Math.max(i - 1, 0));
    } else if (e.key === 'Enter' && historyIdx() >= 0) {
      e.preventDefault();
      handleHistorySelect(items[historyIdx()].query);
    } else if (e.key === 'Escape') {
      setHistoryOpen(false);
      setHistoryIdx(-1);
    }
  }

  const history = () => getHistory().slice(0, 8);
  const showHistory = () => historyOpen() && history().length > 0 && !value().trim();

  return (
    <form class="domain-input" onSubmit={handleSubmit} noValidate>
      <div class="domain-input__field-wrap">
        <input
          class="domain-input__field"
          type="text"
          placeholder="example.com"
          value={value()}
          onInput={(e) => { setValue(e.currentTarget.value); setHistoryIdx(-1); }}
          onFocus={() => { if (history().length > 0 && !value().trim()) setHistoryOpen(true); }}
          onBlur={() => setTimeout(() => { setHistoryOpen(false); setHistoryIdx(-1); }, 150)}
          onKeyDown={handleKeyDown}
          disabled={props.loading}
          aria-label="Domain name to check"
          aria-autocomplete="list"
          aria-expanded={showHistory()}
          aria-controls={LISTBOX_ID}
          aria-activedescendant={historyIdx() >= 0 ? `${LISTBOX_ID}-${historyIdx()}` : undefined}
          autocomplete="off"
          autocapitalize="none"
          spellcheck={false}
          ref={(el) => props.inputRef?.(el)}
        />
        <Show when={value()}>
          <button
            class="domain-input__clear"
            type="button"
            aria-label="Clear"
            tabIndex={-1}
            onClick={handleClear}
          >
            ×
          </button>
        </Show>
        <Show when={showHistory()}>
          <div class="domain-input__history" id={LISTBOX_ID} role="listbox" aria-label="Recent checks">
            <For each={history()}>
              {(entry, i) => (
                <button
                  id={`${LISTBOX_ID}-${i()}`}
                  class={`domain-input__history-item${historyIdx() === i() ? ' domain-input__history-item--active' : ''}`}
                  type="button"
                  role="option"
                  aria-selected={historyIdx() === i()}
                  onClick={() => handleHistorySelect(entry.query)}
                >
                  {entry.query}
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>
      <button
        class="btn-primary domain-input__btn"
        type="submit"
        disabled={props.loading || !value().trim()}
        aria-busy={props.loading}
      >
        <Show when={props.loading} fallback={'Check'}>
          <span class="spinner" aria-hidden="true" />{' '}Checking...
        </Show>
      </button>
    </form>
  );
}
