import { Show, For } from 'solid-js';
import type { CspReport } from '../lib/types';

interface Props {
  csp: CspReport;
}

export default function CspAnalysis(props: Props) {
  const directives = () => Object.entries(props.csp.directives);

  return (
    <>
      <Show when={props.csp.issues.length > 0}>
        <ul style={{ 'list-style': 'none', padding: '0', margin: '0 0 0.75rem 0' }}>
          <For each={props.csp.issues}>
            {(issue) => (
              <li style={{ 'font-size': '0.875rem', padding: '0.25rem 0', color: 'var(--warn)' }}>
                {issue}
              </li>
            )}
          </For>
        </ul>
      </Show>

      <Show when={directives().length > 0}>
        <ul class="csp-directives">
          <For each={directives()}>
            {([name, values]) => (
              <li class="csp-directive">
                <span class="csp-directive__name">{name}</span>
                <span class="csp-directive__values">{values.join(' ')}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </>
  );
}
