import { For } from 'solid-js';
import type { PortQualityResult, CheckStatus } from '../lib/types';

interface Props {
  ports: number[];
  selected: number;
  onSelect: (port: number) => void;
  portQuality?: Record<number, PortQualityResult | undefined>;
}

function worstStatus(quality: PortQualityResult | undefined): CheckStatus | null {
  if (!quality) return null;
  const checks = quality.checks;
  if (checks.some(c => c.status === 'fail')) return 'fail';
  if (checks.some(c => c.status === 'warn')) return 'warn';
  if (checks.every(c => c.status === 'pass' || c.status === 'skip')) return 'pass';
  return null;
}

const STATUS_DOT: Record<string, string> = {
  pass: 'var(--pass)',
  warn: 'var(--warn)',
  fail: 'var(--fail)',
};

export default function PortTabs(props: Props) {
  const handleKeyDown = (e: KeyboardEvent) => {
    const currentIdx = props.ports.indexOf(props.selected);
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      const next = props.ports[(currentIdx + 1) % props.ports.length];
      props.onSelect(next);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      const prev = props.ports[(currentIdx - 1 + props.ports.length) % props.ports.length];
      props.onSelect(prev);
    }
  };

  return (
    <div class="port-tabs" role="tablist" onKeyDown={handleKeyDown}>
      <For each={props.ports}>
        {(port) => {
          const quality = props.portQuality?.[port];
          const status = worstStatus(quality);
          return (
            <button
              role="tab"
              id={`tab-${port}`}
              aria-controls={`panel-${port}`}
              aria-selected={port === props.selected ? 'true' : 'false'}
              tabindex={port === props.selected ? 0 : -1}
              class={`port-tabs__tab ${port === props.selected ? 'port-tabs__tab--active' : ''}`}
              onClick={() => props.onSelect(port)}
            >
              {status && (
                <span
                  class="port-tabs__dot"
                  style={{ background: STATUS_DOT[status] }}
                  aria-hidden="true"
                />
              )}
              :{port}
            </button>
          );
        }}
      </For>
    </div>
  );
}
