import { createSignal } from 'solid-js';

const PRESETS: { label: string; ports: string }[] = [
  { label: 'HTTPS', ports: ':443' },
  { label: 'Email', ports: ':25,465,587,993,995' },
  { label: 'All Common', ports: ':443,25,465,587,993,995,8443' },
];

interface Props {
  onSubmit: (input: string) => void;
  loading: boolean;
  inputRef?: (el: HTMLInputElement) => void;
}

export default function HostInput(props: Props) {
  const [value, setValue] = createSignal('');

  const handleSubmit = (e: Event) => {
    e.preventDefault();
    const v = value().trim();
    if (v) props.onSubmit(v);
  };

  const applyPreset = (ports: string) => {
    const current = value().trim();
    const hostname = current.split(':')[0];
    setValue(hostname + ports);
  };

  return (
    <div>
      <form class="host-input" onSubmit={handleSubmit}>
        <input
          ref={el => props.inputRef?.(el)}
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
