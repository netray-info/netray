import { createSignal } from 'solid-js';

interface Props {
  onSubmit: (input: string) => void;
  loading: boolean;
}

export default function HostInput(props: Props) {
  const [value, setValue] = createSignal('');

  const handleSubmit = (e: Event) => {
    e.preventDefault();
    const v = value().trim();
    if (v) props.onSubmit(v);
  };

  return (
    <form class="host-input" onSubmit={handleSubmit}>
      <input
        type="text"
        class="host-input__field"
        placeholder="example.com or example.com:443,8443"
        value={value()}
        onInput={e => setValue(e.currentTarget.value)}
        disabled={props.loading}
        autofocus
      />
      <button class="host-input__submit" type="submit" disabled={props.loading || !value().trim()}>
        {props.loading ? 'Inspecting...' : 'Inspect'}
      </button>
    </form>
  );
}
