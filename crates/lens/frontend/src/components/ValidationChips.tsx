import { For, Show } from 'solid-js';

interface Props {
  checks: Array<{ verdict: string }>;
}

const CHIP_META: Record<string, { label: string }> = {
  pass: { label: 'passed' },
  warn: { label: 'warnings' },
  fail: { label: 'failed' },
  skip: { label: 'skipped' },
};

const CHIP_ORDER = ['fail', 'warn', 'pass', 'skip'];

export default function ValidationChips(props: Props) {
  const counts = () => {
    const result: Record<string, number> = {};
    for (const check of props.checks) {
      result[check.verdict] = (result[check.verdict] ?? 0) + 1;
    }
    return result;
  };

  const chips = () =>
    CHIP_ORDER.filter((v) => (counts()[v] ?? 0) > 0).map((v) => ({
      verdict: v,
      count: counts()[v],
      ...CHIP_META[v],
    }));

  return (
    <Show when={chips().length > 0}>
      <div class="validation-chips" role="status" aria-label="Validation summary">
        <For each={chips()}>
          {(chip) => (
            <span class={`chip chip--${chip.verdict}`}>
              {chip.count} {chip.label}
            </span>
          )}
        </For>
      </div>
    </Show>
  );
}
