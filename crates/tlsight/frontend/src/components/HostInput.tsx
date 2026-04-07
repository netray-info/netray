import { createSignal, createEffect, Show, For, onMount, onCleanup } from 'solid-js';
import { getHistory } from '../lib/history';

const PRESETS: { label: string; ports: string }[] = [
  { label: 'HTTPS', ports: ':443' },
  { label: 'E-Mail', ports: ':25,465,587,993,995' },
  { label: 'All Common', ports: ':443,25,465,587,993,995,8443' },
];

interface Props {
  onSubmit: (input: string) => void;
  onClear?: () => void;
  loading: boolean;
  inputRef?: (el: HTMLInputElement) => void;
  value?: string;
}

export default function HostInput(props: Props) {
  const [value, setValue] = createSignal(props.value ?? '');
  createEffect(() => { if (props.value !== undefined) setValue(props.value); });
  const [historyOpen, setHistoryOpen] = createSignal(false);
  const [historyIdx, setHistoryIdx] = createSignal(-1);

  let savedInput = '';
  let wrapRef: HTMLDivElement | undefined;

  const history = () => getHistory().map(e => e.query);

  const handleSubmit = (e: Event) => {
    e.preventDefault();
    const v = value().trim();
    if (v) {
      setHistoryOpen(false);
      setHistoryIdx(-1);
      props.onSubmit(v);
    }
  };

  const handleClear = () => {
    setValue('');
    setHistoryIdx(-1);
    setHistoryOpen(false);
    props.onClear?.();
  };

  const applyPreset = (ports: string) => {
    const current = value().trim();
    const hostname = current.split(':')[0];
    setValue(hostname + ports);
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    const hist = history();
    const isDown = e.key === 'ArrowDown' || (e.key === 'j' && e.ctrlKey);
    const isUp = e.key === 'ArrowUp' || (e.key === 'k' && e.ctrlKey);
    if (isDown) {
      if (hist.length === 0) return;
      e.preventDefault();
      if (historyIdx() === -1) {
        savedInput = value();
        setHistoryOpen(true);
      }
      if (historyIdx() < hist.length - 1) {
        const next = historyIdx() + 1;
        setHistoryIdx(next);
        setValue(hist[next]);
      }
    } else if (isUp) {
      if (historyIdx() <= -1) return;
      e.preventDefault();
      if (historyIdx() > 0) {
        const next = historyIdx() - 1;
        setHistoryIdx(next);
        setValue(hist[next]);
      } else {
        setHistoryIdx(-1);
        setValue(savedInput);
        setHistoryOpen(false);
      }
    } else if (e.key === 'Escape') {
      if (historyOpen()) {
        setHistoryIdx(-1);
        setValue(savedInput);
        setHistoryOpen(false);
        e.preventDefault();
        e.stopPropagation();
      }
    }
  };

  const selectHistoryItem = (query: string) => {
    setValue(query);
    setHistoryOpen(false);
    setHistoryIdx(-1);
  };

  // Close dropdown on outside click
  const handleDocClick = (e: MouseEvent) => {
    if (wrapRef && !wrapRef.contains(e.target as Node)) {
      setHistoryOpen(false);
      setHistoryIdx(-1);
    }
  };

  onMount(() => {
    document.addEventListener('mousedown', handleDocClick);
    onCleanup(() => document.removeEventListener('mousedown', handleDocClick));
  });

  return (
    <div ref={wrapRef}>
      <form class="host-input" onSubmit={handleSubmit}>
        <div class="host-input__field-wrap">
          <input
            ref={el => props.inputRef?.(el)}
            type="text"
            class="host-input__field"
            placeholder="example.com or example.com:443,8443"
            value={value()}
            onInput={e => { setValue(e.currentTarget.value); setHistoryIdx(-1); }}
            onKeyDown={handleKeyDown}
            onFocus={() => { if (history().length > 0 && !value().trim()) setHistoryOpen(true); }}
            aria-label="Hostname to inspect"
            disabled={props.loading}
            autofocus
            role="combobox"
            aria-expanded={historyOpen() ? "true" : "false"}
            aria-autocomplete="list"
            aria-controls="host-history-listbox"
          />
          <Show when={value().trim()}>
            <button
              class="host-input__clear"
              type="button"
              onClick={handleClear}
              title="Clear"
              tabIndex={-1}
            >&times;</button>
          </Show>
        </div>
        <button class="host-input__submit" type="submit" disabled={props.loading || !value().trim()}>
          {props.loading ? 'Inspecting...' : 'Inspect'}
        </button>
      </form>

      <Show when={historyOpen() && history().length > 0}>
        <div class="host-input__history" role="listbox" id="host-history-listbox">
          <For each={history()}>
            {(q, i) => (
              <button
                class="host-input__history-item"
                classList={{ 'host-input__history-item--active': i() === historyIdx() }}
                onMouseDown={() => selectHistoryItem(q)}
                role="option"
              >
                <span class="mono">{q}</span>
              </button>
            )}
          </For>
        </div>
      </Show>

      <div class="host-input__presets">
        {PRESETS.map(p => (
          <button
            class="host-input__preset"
            type="button"
            onClick={() => applyPreset(p.ports)}
            disabled={props.loading}
          >
            {p.label}
          </button>
        ))}
      </div>
    </div>
  );
}
