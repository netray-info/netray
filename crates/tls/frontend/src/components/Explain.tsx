import { Show } from 'solid-js';
import type { JSX } from 'solid-js';

export default function Explain(props: { when: boolean; children: JSX.Element; guideUrl?: string }) {
  return (
    <Show when={props.when}>
      <div class="explain-card">
        {props.children}
        <Show when={props.guideUrl}>
          {' '}<a href={props.guideUrl} target="_blank" rel="noopener noreferrer" class="explain-card__guide-link">Learn more ↗</a>
        </Show>
      </div>
    </Show>
  );
}
