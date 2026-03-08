import { createSignal, Show, For } from 'solid-js';
import type { CtInfo } from '../lib/types';

export default function CtView(props: { ct: CtInfo }) {
  const [expanded, setExpanded] = createSignal(false);

  const statusClass = () => props.ct.sct_count >= 2 ? 'badge--pass' : 'badge--warn';
  const statusLabel = () => props.ct.sct_count >= 2 ? 'Pass' : 'Warn';

  return (
    <div class="dns-section">
      <div class="dns-section__title">
        Certificate Transparency
        <span class={`badge ${statusClass()}`}>{statusLabel()}</span>
        <span class="badge">{props.ct.sct_count} SCT{props.ct.sct_count !== 1 ? 's' : ''}</span>
      </div>
      <Show when={props.ct.scts.length > 0}>
        <button
          class="detail-toggle"
          onClick={() => setExpanded(e => !e)}
        >
          {expanded() ? 'Hide' : 'Show'} SCT details
        </button>
        <Show when={expanded()}>
          <ul class="dns-records" style={{ "margin-top": "0.5rem" }}>
            <For each={props.ct.scts}>
              {(sct) => (
                <li class="dns-records__item mono">
                  Log: {sct.log_id.slice(0, 16)}... | v{sct.version} | {sct.timestamp}
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>
    </div>
  );
}
