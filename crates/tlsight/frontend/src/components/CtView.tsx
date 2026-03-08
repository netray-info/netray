import { createSignal, createEffect, Show, For } from 'solid-js';
import Explain from './Explain';
import type { CtInfo } from '../lib/types';

export default function CtView(props: { ct: CtInfo; explain?: boolean; expanded?: boolean }) {
  const [expanded, setExpanded] = createSignal(false);
  createEffect(() => { if (props.expanded !== undefined) setExpanded(props.expanded); });

  const statusClass = () => props.ct.sct_count >= 2 ? 'badge--pass' : 'badge--warn';
  const statusLabel = () => props.ct.sct_count >= 2 ? 'Pass' : 'Warn';

  return (
    <div class="dns-section">
      <div class="dns-section__title">
        Certificate Transparency
        <span class={`badge ${statusClass()}`}>{statusLabel()}</span>
        <span class="badge">{props.ct.sct_count} SCT{props.ct.sct_count !== 1 ? 's' : ''}</span>
      </div>
      <Explain when={!!props.explain}>Certificate Transparency (CT) logs are public, append-only ledgers where CAs must publish certificates. Having 2+ SCTs (Signed Certificate Timestamps) from different logs is required by browsers.</Explain>
      <Show when={props.ct.scts.length > 0}>
        <button
          class="detail-toggle"
          onClick={() => setExpanded(e => !e)}
        >
          {expanded() ? 'hide' : 'show'} SCT details
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
