import { createSignal, createEffect, Show } from 'solid-js';
import Explain from './Explain';
import type { TlsInfo, OcspInfo, OcspRevocationResult } from '../lib/types';

interface Props {
  params: TlsInfo;
  explain?: boolean;
  expanded?: boolean;
}

function formatRelativeTime(isoStr: string): string {
  const then = new Date(isoStr);
  const diffMs = Date.now() - then.getTime();
  const diffMins = Math.round(diffMs / 60_000);
  if (diffMins < 0) return 'in the future';
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.round(diffMins / 60);
  if (diffHours < 48) return `${diffHours}h ago`;
  return `${Math.round(diffHours / 24)}d ago`;
}

function formatDuration(fromIso: string, toIso: string): string {
  const from = new Date(fromIso);
  const to = new Date(toIso);
  const diffMs = to.getTime() - from.getTime();
  if (diffMs <= 0) return 'expired';
  const diffMins = Math.round(diffMs / 60_000);
  if (diffMins < 60) return `${diffMins}m`;
  const diffHours = Math.round(diffMins / 60);
  if (diffHours < 48) return `${diffHours}h`;
  return `${Math.round(diffHours / 24)}d`;
}

function OcspBadge(props: { ocsp: OcspInfo }) {
  const stapled = () => props.ocsp.stapled;
  const status = () => props.ocsp.status;
  const thisUpdate = () => props.ocsp.this_update;
  const nextUpdate = () => props.ocsp.next_update;

  const badgeClass = () => {
    if (!stapled()) return 'badge badge--skip';
    if (status() === 'good') return 'badge badge--pass';
    if (status() === 'revoked') return 'badge badge--fail';
    return 'badge badge--warn';
  };

  const badgeLabel = () => {
    if (!stapled()) return 'Not stapled';
    if (status()) return `Stapled \u00b7 ${status()}`;
    return 'Stapled';
  };

  const freshness = () => {
    const tu = thisUpdate();
    const nu = nextUpdate();
    if (!stapled() || !tu) return null;
    const updated = formatRelativeTime(tu);
    if (nu) {
      const valid = formatDuration(tu, nu);
      const expired = new Date(nu) < new Date();
      return `updated ${updated}, valid for ${valid}${expired ? ' (expired)' : ''}`;
    }
    return `updated ${updated}`;
  };

  return (
    <span class="ocsp-badge">
      <span class={badgeClass()}>{badgeLabel()}</span>
      <Show when={freshness()}>
        <span class="ocsp-badge__freshness">{freshness()}</span>
      </Show>
    </span>
  );
}

function OcspLiveBadge(props: { result: OcspRevocationResult }) {
  const statusClass = () => {
    if (props.result.status === 'good') return 'badge badge--pass';
    if (props.result.status === 'revoked') return 'badge badge--fail';
    return 'badge badge--warn';
  };
  const label = () => {
    if (props.result.status === 'revoked') {
      const reason = props.result.reason ? ` (${props.result.reason})` : '';
      return `Revoked${reason}`;
    }
    return props.result.status.charAt(0).toUpperCase() + props.result.status.slice(1);
  };
  return (
    <span class="ocsp-badge">
      <span class={statusClass()}>{label()}</span>
      <span class="ocsp-badge__freshness">checked {new Date(props.result.checked_at).toLocaleTimeString()}</span>
    </span>
  );
}

export default function TlsParams(props: Props) {
  const [expanded, setExpanded] = createSignal(false);
  createEffect(() => { if (props.expanded !== undefined) setExpanded(props.expanded); });

  const ocspLabel = () =>
    props.params.ocsp.stapled ? `OCSP stapled (${props.params.ocsp.status})` : 'no OCSP staple';

  return (
    <div class="tls-params" data-card>
      <button class="dns-section__toggle" onClick={() => setExpanded(!expanded())} aria-expanded={expanded() ? "true" : "false"}>
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
      <Explain when={!!props.explain} guideUrl="/guide/tls-protocol.html">These are the TLS connection parameters negotiated during the handshake. TLSv1.3 is current best practice. The cipher suite determines the encryption algorithm. OCSP stapling improves certificate revocation checking performance.</Explain>
      {expanded() && (
        <div class="dns-section__body">
          <table class="tls-params__table">
            <tbody>
              <tr><th>Version</th><td>{props.params.version}</td></tr>
              <tr><th>Cipher Suite</th><td class="mono">{props.params.cipher_suite}</td></tr>
              <tr><th>ALPN</th><td>{props.params.alpn ?? 'none'}</td></tr>
              <tr><th>SNI</th><td>{props.params.sni ?? 'none'}</td></tr>
              <Show when={props.params.key_exchange_group}>
                <tr><th>Key Exchange</th><td>{props.params.key_exchange_group}</td></tr>
              </Show>
              <tr><th>OCSP staple</th><td><OcspBadge ocsp={props.params.ocsp} /></td></tr>
              <Show when={props.params.ocsp_live}>
                {(live) => (
                  <tr><th>OCSP live check</th><td><OcspLiveBadge result={live()} /></td></tr>
                )}
              </Show>
              <Show when={props.params.starttls}>
                <tr>
                  <th>STARTTLS</th>
                  <td><span class="badge badge--pass">{props.params.starttls?.toUpperCase()}</span></td>
                </tr>
              </Show>
              <Show when={props.params.ech_advertised !== undefined}>
                <tr>
                  <th>ECH</th>
                  <td>
                    {props.params.ech_advertised
                      ? <span class="badge badge--pass">Advertised</span>
                      : <span class="badge badge--warn">Not advertised</span>
                    }
                  </td>
                </tr>
              </Show>
              <tr><th>Handshake</th><td>{props.params.handshake_ms}ms</td></tr>
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
