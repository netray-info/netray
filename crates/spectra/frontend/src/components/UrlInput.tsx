import { createSignal, createEffect, onCleanup, Show } from 'solid-js';
import { copyToClipboard } from '@netray-info/common-frontend/utils';

interface Props {
  onSubmit: (url: string) => void;
  onClear?: () => void;
  loading: boolean;
  initialValue?: string;
  value?: string;
  inputRef?: (el: HTMLInputElement) => void;
  showCopyLink?: boolean;
}

export default function UrlInput(props: Props) {
  const [value, setValue] = createSignal(props.initialValue ?? props.value ?? '');
  const [linkCopied, setLinkCopied] = createSignal(false);
  const [elapsed, setElapsed] = createSignal(0);
  createEffect(() => { if (props.value !== undefined) setValue(props.value); });
  createEffect(() => {
    if (!props.loading) return;
    setElapsed(0);
    const id = setInterval(() => setElapsed(s => s + 1), 1000);
    onCleanup(() => clearInterval(id));
  });

  async function handleCopyLink() {
    const ok = await copyToClipboard(window.location.href);
    if (ok) { setLinkCopied(true); setTimeout(() => setLinkCopied(false), 2000); }
  }


  function handleSubmit(e: Event) {
    e.preventDefault();
    const v = value().trim();
    if (v) props.onSubmit(v);
  }

  function handleClear() {
    setValue('');
    props.onClear?.();
  }

  return (
    <form class="url-input" onSubmit={handleSubmit}>
      <div class="url-input__field-wrap">
        <input
          ref={el => props.inputRef?.(el)}
          class="url-input__field"
          type="text"
          placeholder="https://example.com"
          aria-label="URL to inspect"
          value={value()}
          onInput={(e) => setValue(e.currentTarget.value)}
          autofocus
          autocomplete="off"
          autocorrect="off"
          autocapitalize="none"
          spellcheck={false}
          disabled={props.loading}
        />
        <Show when={value().trim()}>
          <button
            class="url-input__clear"
            type="button"
            onClick={handleClear}
            title="Clear"
            aria-label="Clear"
          >&times;</button>
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
      <button class="btn-primary url-input__submit" type="submit" disabled={props.loading || !value().trim()}>
        {props.loading ? `Inspecting... ${elapsed()}s` : 'Inspect'}
      </button>
    </form>
  );
}
