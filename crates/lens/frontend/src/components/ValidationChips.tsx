import { For, Show } from 'solid-js';

interface Props {
  checks: Array<{ verdict: string }>;
}

const CHIP_META: Record<string, { symbol: string; label: string }> = {
  pass: { symbol: '✓', label: 'pass' },
  warn: { symbol: '!', label: 'warn' },
  fail: { symbol: '✗', label: 'fail' },
  skip: { symbol: '–', label: 'skip' },
};

const CHIP_ORDER = ['pass', 'warn', 'fail', 'skip'];

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
      <style>{`
        .validation-chips {
          display: flex;
          flex-wrap: wrap;
          gap: 0.5rem;
        }

        .chip {
          display: inline-flex;
          align-items: center;
          gap: 0.25rem;
          font-family: var(--mono);
          font-size: 0.8rem;
          padding: 0.2rem 0.5rem;
          border-radius: 4px;
          border: 1px solid currentColor;
        }

        .chip--pass {
          color: var(--verdict-pass);
        }

        .chip--warn {
          color: var(--verdict-warn);
        }

        .chip--fail {
          color: var(--verdict-fail);
        }

        .chip--skip {
          color: var(--verdict-skip);
        }
      `}</style>
      <div class="validation-chips" role="status" aria-label="Validation summary">
        <For each={chips()}>
          {(chip) => (
            <span class={`chip chip--${chip.verdict}`}>
              {chip.symbol} {chip.count} {chip.label}
            </span>
          )}
        </For>
      </div>
    </Show>
  );
}
