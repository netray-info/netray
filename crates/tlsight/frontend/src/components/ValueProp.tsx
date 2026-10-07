import { Show } from 'solid-js';

interface ValuePropProps {
  visible: boolean;
}

export function ValueProp(props: ValuePropProps) {
  return (
    <Show when={props.visible}>
      <div class="value-prop">
        <p class="value-prop-headline">Like SSL Labs, but self-hostable.</p>
        <ul class="value-prop-list">
          <li>DANE/TLSA validation — checks DNS-Based Authentication</li>
          <li>Multi-IP consistency — detects CDN cert mismatches across all IPs</li>
          <li>No account, no rate limits, no data retention</li>
          <li>JSON API — scriptable, CI-friendly (<a href="/docs" class="value-prop-link">OpenAPI docs</a>)</li>
          <li>Self-hostable single binary</li>
        </ul>
      </div>
    </Show>
  );
}
