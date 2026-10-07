import { createSignal, Show, For } from 'solid-js';
import { copyToClipboard } from '@netray-info/common-frontend/utils';
import { getHistory } from '../lib/history';
import { formatQuery } from '../lib/parse';

function ShareButton(props: {
  domain: () => string;
  selectors: () => string[];
}) {
  const [status, setStatus] = createSignal<'idle' | 'copied'>('idle');

  const buildShareUrl = (): string => {
    const d = props.domain();
    if (!d) return window.location.href;
    const base = `${window.location.origin}${window.location.pathname}`;
    const params = new URLSearchParams();
    params.set('q', formatQuery(d, props.selectors()));
    return `${base}?${params.toString()}`;
  };

  const disabled = () => !props.domain();

  const handleClick = async () => {
    if (disabled()) return;
    await copyToClipboard(buildShareUrl());
    setStatus('copied');
    setTimeout(() => setStatus('idle'), 2000);
  };

  return (
    <span class="share-btn-wrapper">
      <button
        type="button"
        class="share-btn"
        aria-label="Copy shareable link"
        title={status() === 'copied' ? 'Copied!' : 'Copy shareable link'}
        disabled={disabled()}
        onClick={handleClick}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
          <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
        </svg>
      </button>
      <Show when={status() === 'copied'}>
        <span class="share-btn__toast" role="status" aria-live="polite">Copied!</span>
      </Show>
    </span>
  );
}

interface DomainInputProps {
  query: () => string;
  onQueryChange: (v: string) => void;
  onSubmit: (e: Event) => void;
  loading: () => boolean;
  inputRef: (el: HTMLInputElement) => void;
  inspectedDomain: () => string;
  inspectedSelectors: () => string[];
  onRunQuery: (raw: string) => void;
}

export default function DomainInput(props: DomainInputProps) {
  const [showHistory, setShowHistory] = createSignal(false);
  const [historyIndex, setHistoryIndex] = createSignal<number>(-1);

  return (
    <form class="inspect-form" onSubmit={props.onSubmit}>
      <div class="domain-input-row">
        <div class="domain-input-wrapper">
          <input
            ref={props.inputRef}
            type="text"
            class="domain-input"
            placeholder="example.com [dkim-selector ...]"
            value={props.query()}
            onInput={(e) => {
              props.onQueryChange(e.currentTarget.value);
              setHistoryIndex(-1);
            }}
            onFocus={() => { setShowHistory(true); setHistoryIndex(-1); }}
            onBlur={() => setTimeout(() => setShowHistory(false), 200)}
            onKeyDown={(e) => {
              const items = getHistory();
              const open = showHistory() && items.length > 0;
              if (e.key === 'Escape') {
                if (showHistory()) { e.preventDefault(); setShowHistory(false); setHistoryIndex(-1); }
                return;
              }
              if (!open) return;
              if (e.key === 'ArrowDown') {
                e.preventDefault();
                setHistoryIndex((i) => (i + 1) % items.length);
              } else if (e.key === 'ArrowUp') {
                e.preventDefault();
                setHistoryIndex((i) => (i <= 0 ? items.length - 1 : i - 1));
              } else if (e.key === 'Home') {
                e.preventDefault();
                setHistoryIndex(0);
              } else if (e.key === 'End') {
                e.preventDefault();
                setHistoryIndex(items.length - 1);
              } else if (e.key === 'Enter') {
                const idx = historyIndex();
                if (idx >= 0 && idx < items.length) {
                  e.preventDefault();
                  const item = items[idx].query;
                  props.onQueryChange(item);
                  setShowHistory(false);
                  setHistoryIndex(-1);
                  props.onRunQuery(item);
                }
              }
            }}
            role="combobox"
            aria-label="Domain and optional DKIM selectors, space-separated"
            aria-expanded={showHistory() && getHistory().length > 0}
            aria-autocomplete="list"
            aria-controls="history-listbox"
            aria-activedescendant={
              historyIndex() >= 0 ? `history-option-${historyIndex()}` : undefined
            }
            autocomplete="off"
            spellcheck={false}
          />
          <Show when={props.query()}>
            <button
              type="button"
              class="domain-input__clear"
              aria-label="Clear"
              onClick={() => props.onQueryChange('')}
            >×</button>
          </Show>
          <Show when={showHistory() && getHistory().length > 0}>
            <ul
              class="history-dropdown"
              id="history-listbox"
              role="listbox"
            >
              <For each={getHistory()}>
                {(entry, i) => {
                  const isActive = () => historyIndex() === i();
                  return (
                    <li
                      id={`history-option-${i()}`}
                      role="option"
                      aria-selected={isActive()}
                      classList={{ 'history-dropdown__item--active': isActive() }}
                      onMouseDown={() => {
                        props.onQueryChange(entry.query);
                        setShowHistory(false);
                        setHistoryIndex(-1);
                        props.onRunQuery(entry.query);
                      }}
                    >
                      {entry.query}
                    </li>
                  );
                }}
              </For>
            </ul>
          </Show>
        </div>
        <ShareButton
          domain={props.inspectedDomain}
          selectors={props.inspectedSelectors}
        />
        <button type="submit" class="btn-primary" disabled={props.loading() || !props.query().trim()}>
          {props.loading() ? 'Inspecting…' : 'Inspect'}
        </button>
      </div>
    </form>
  );
}
