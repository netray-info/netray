import { createSignal } from 'solid-js';

interface Props {
  onSubmit: (domain: string) => void;
  loading: boolean;
}

export default function DomainInput(props: Props) {
  const [value, setValue] = createSignal('');

  function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    const trimmed = value().trim();
    if (!trimmed || props.loading) return;
    props.onSubmit(trimmed);
  }

  return (
    <form class="domain-input" onSubmit={handleSubmit} noValidate>
      <input
        class="domain-input__field"
        type="text"
        placeholder="example.com"
        value={value()}
        onInput={(e) => setValue(e.currentTarget.value)}
        disabled={props.loading}
        aria-label="Domain name"
        autocomplete="off"
        autocapitalize="none"
        spellcheck={false}
      />
      <button
        class="domain-input__btn"
        type="submit"
        disabled={props.loading || !value().trim()}
        aria-busy={props.loading}
      >
        {props.loading
          ? <><span class="spinner" aria-hidden="true" /> Checking...</>
          : 'Check domain →'
        }
      </button>
    </form>
  );
}
