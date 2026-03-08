import type { TlsInfo } from '../lib/types';

interface Props {
  params: TlsInfo;
}

export default function TlsParams(props: Props) {
  return (
    <div class="tls-params">
      <h2>TLS Parameters</h2>
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
  );
}
