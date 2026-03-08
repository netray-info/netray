import { For, Show } from 'solid-js';
import type { ConsistencyInfo } from '../lib/types';

interface Props {
  consistency: ConsistencyInfo;
}

export default function ConsistencyView(props: Props) {
  const allMatch = () =>
    props.consistency.certificates_match &&
    props.consistency.tls_versions_match &&
    props.consistency.cipher_suites_match;

  return (
    <div class="consistency-view">
      <h3>IP Consistency</h3>
      <p class="consistency-view__desc">Whether all IPs for this hostname serve the same certificate and TLS configuration.</p>
      <div class="consistency-view__badges">
        <Badge label="Certificates" match={props.consistency.certificates_match} />
        <Badge label="TLS Version" match={props.consistency.tls_versions_match} />
        <Badge label="Cipher Suite" match={props.consistency.cipher_suites_match} />
      </div>
      <Show when={!allMatch()}>
        <div class="consistency-view__mismatches">
          <For each={props.consistency.mismatches}>
            {(m) => (
              <div class="consistency-view__mismatch">
                <strong>{m.field}</strong>
                <div class="consistency-view__values">
                  <For each={Object.entries(m.values)}>
                    {([ip, val]) => (
                      <div class="consistency-view__value">
                        <span class="mono">{ip}</span>: {val}
                      </div>
                    )}
                  </For>
                </div>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

function Badge(props: { label: string; match: boolean }) {
  return (
    <span class={`consistency-badge consistency-badge--${props.match ? 'match' : 'mismatch'}`}>
      {props.match ? '*' : '!'} {props.label}
    </span>
  );
}
