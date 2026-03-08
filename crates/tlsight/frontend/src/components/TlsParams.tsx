import { createSignal } from 'solid-js';
import type { TlsInfo } from '../lib/types';

interface Props {
  params: TlsInfo;
}

export default function TlsParams(props: Props) {
  const [expanded, setExpanded] = createSignal(false);

  const ocspLabel = () =>
    props.params.ocsp.stapled ? `OCSP stapled (${props.params.ocsp.status})` : 'no OCSP staple';

  return (
    <div class="tls-params" data-card>
      <button class="dns-section__toggle" onClick={() => setExpanded(!expanded())}>
        <span class="dns-section__toggle-left">
          TLS Parameters
          <span class="ip-card__badge">{props.params.version}</span>
          <span class="ip-card__badge">{props.params.cipher_suite}</span>
          <span class="ip-card__badge">{ocspLabel()}</span>
        </span>
        <span class="ip-card__chevron" classList={{ 'ip-card__chevron--open': expanded() }}>
          &#x25B8;
        </span>
      </button>
      {expanded() && (
        <div class="dns-section__body">
          <table class="tls-params__table">
            <tbody>
              <tr><th>Version</th><td>{props.params.version}</td></tr>
              <tr><th>Cipher Suite</th><td class="mono">{props.params.cipher_suite}</td></tr>
              <tr><th>ALPN</th><td>{props.params.alpn ?? 'none'}</td></tr>
              <tr><th>SNI</th><td>{props.params.sni ?? 'none'}</td></tr>
              <tr><th>OCSP</th><td>{props.params.ocsp.stapled ? `stapled (${props.params.ocsp.status})` : 'not stapled'}</td></tr>
              <tr><th>Handshake</th><td>{props.params.handshake_ms}ms</td></tr>
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
