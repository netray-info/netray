import { createSignal, createEffect, Show } from 'solid-js';

interface Props {
  onSubmit: (url: string) => void;
  onClear?: () => void;
  loading: boolean;
  initialValue?: string;
  value?: string;
  inputRef?: (el: HTMLInputElement) => void;
}

export default function UrlInput(props: Props) {
  const [value, setValue] = createSignal(props.initialValue ?? props.value ?? '');
  createEffect(() => { if (props.value !== undefined) setValue(props.value); });

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
          >&times;</button>
        </Show>
      </div>
      <button class="btn-primary url-input__submit" type="submit" disabled={props.loading || !value().trim()}>
        {props.loading ? 'Inspecting...' : 'Inspect'}
      </button>
    </form>
  );
}
