import { createSignal } from 'solid-js';

interface Props {
  onSubmit: (url: string) => void;
  loading: boolean;
  initialValue?: string;
}

export default function UrlInput(props: Props) {
  const [value, setValue] = createSignal(props.initialValue ?? '');

  function handleSubmit(e: Event) {
    e.preventDefault();
    const v = value().trim();
    if (v) props.onSubmit(v);
  }

  return (
    <form class="url-input" onSubmit={handleSubmit}>
      <input
        class="url-input__field"
        type="text"
        placeholder="https://example.com"
        value={value()}
        onInput={(e) => setValue(e.currentTarget.value)}
        autofocus
      />
      <button class="btn-primary" type="submit" disabled={props.loading}>
        {props.loading ? 'Inspecting...' : 'Inspect'}
      </button>
    </form>
  );
}
