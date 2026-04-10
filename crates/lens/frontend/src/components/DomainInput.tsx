import { createSignal, createEffect, Show, For } from 'solid-js';
import { copyToClipboard } from '@netray-info/common-frontend/utils';
import { getHistory } from '../lib/history';

interface Props {
  onSubmit: (domain: string) => void;
  onClear?: () => void;
  loading: boolean;
  value?: string;
  inputRef?: (el: HTMLInputElement) => void;
  showCopyLink?: boolean;
}

const LISTBOX_ID = 'domain-history-listbox';

export default function DomainInput(props: Props) {
  const [value, setValue] = createSignal(props.value ?? '');
  const [historyOpen, setHistoryOpen] = createSignal(false);
  const [historyIdx, setHistoryIdx] = createSignal(-1);
  const [linkCopied, setLinkCopied] = createSignal(false);

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

  async function handleCopyLink() {
    const ok = await copyToClipboard(window.location.href);
    if (ok) { setLinkCopied(true); setTimeout(() => setLinkCopied(false), 2000); }
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
      <Show when={props.showCopyLink}>
        <button
          class="share-btn"
          type="button"
          onClick={handleCopyLink}
          title={linkCopied() ? 'Copied!' : 'Copy shareable link'}
          aria-label="Copy shareable link"
        >
          <Show when={linkCopied()} fallback={
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
              <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
            </svg>
          }>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="20 6 9 17 4 12" />
            </svg>
          </Show>
        </button>
      </Show>
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
