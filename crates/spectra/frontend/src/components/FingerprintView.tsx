import { Show } from 'solid-js';
import type { FingerprintReport } from '../lib/types';

interface Props {
  fingerprint: FingerprintReport;
  deprecatedHeaders: string[];
}

export default function FingerprintView(props: Props) {
  const isEmpty = () =>
    !props.fingerprint.server &&
    props.fingerprint.info_leakage.exposed_headers.length === 0 &&
    props.deprecatedHeaders.length === 0;

  return (
    <>
      <Show when={isEmpty()}>
        <p class="fingerprint-empty">No server header or information leakage headers detected.</p>
      </Show>

      <Show when={props.fingerprint.server}>
        <p class="fingerprint-server">
          Server: <span class="mono">{props.fingerprint.server}</span>
        </p>
      </Show>

      <Show when={props.fingerprint.info_leakage.exposed_headers.length > 0}>
        <p class="fingerprint-leak-warn">
          Info leakage headers: {props.fingerprint.info_leakage.exposed_headers.join(', ')}
        </p>
      </Show>

      <Show when={props.deprecatedHeaders.length > 0}>
        <p class="fingerprint-leak-warn">
          Deprecated headers: {props.deprecatedHeaders.join(', ')}
        </p>
      </Show>
    </>
  );
}
