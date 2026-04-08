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
    props.onClear?.();
    document.querySelector<HTMLInputElement>('.domain-input__field')?.focus();
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
          onInput={(e) => setValue(e.currentTarget.value)}
          onFocus={() => { if (history().length > 0 && !value().trim()) setHistoryOpen(true); }}
          onBlur={() => setTimeout(() => setHistoryOpen(false), 150)}
          disabled={props.loading}
          aria-label="Domain name to check"
          aria-autocomplete="list"
          aria-expanded={showHistory()}
          aria-controls={LISTBOX_ID}
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
              {(entry) => (
                <button
                  class="domain-input__history-item"
                  type="button"
                  role="option"
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
