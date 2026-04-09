import { Show } from 'solid-js';
import type { FingerprintReport } from '../lib/types';

interface Props {
  fingerprint: FingerprintReport;
  deprecatedHeaders: string[];
}

export default function FingerprintView(props: Props) {
  return (
    <>
      <Show when={props.fingerprint.server}>
        <p style={{ 'font-size': '0.875rem' }}>
          Server: <span class="mono">{props.fingerprint.server}</span>
        </p>
      </Show>

      <Show when={props.fingerprint.info_leakage.exposed_headers.length > 0}>
        <p style={{ 'font-size': '0.875rem', 'margin-top': '0.5rem', color: 'var(--warn)' }}>
          Info leakage headers: {props.fingerprint.info_leakage.exposed_headers.join(', ')}
        </p>
      </Show>

      <Show when={props.deprecatedHeaders.length > 0}>
        <p style={{ 'font-size': '0.875rem', 'margin-top': '0.5rem', color: 'var(--warn)' }}>
          Deprecated headers: {props.deprecatedHeaders.join(', ')}
        </p>
      </Show>
    </>
  );
}
