import { Show } from 'solid-js';
import type { JSX } from 'solid-js';

export default function Explain(props: { when: boolean; children: JSX.Element }) {
  return (
    <Show when={props.when}>
      <div class="explain-card">{props.children}</div>
    </Show>
  );
}
