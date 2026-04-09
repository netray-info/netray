import { createSignal, createEffect, Show, For } from 'solid-js';
import type { IpResult } from '../lib/types';
import { explainTrustReason } from '../lib/trust';
import { certDisplayName } from '../lib/cert';
import Explain from './Explain';
import IpBadges from './IpBadges';
import TlsParams from './TlsParams';
import CtView from './CtView';
import ChainView from './ChainView';
import CertDetail from './CertDetail';

interface Props {
  result: IpResult;
  defaultExpanded: boolean;
  expanded?: boolean;
  explain?: boolean;
  ipUrl?: string;
  dnsUrl?: string | null;
  host?: string;
  port?: number;
}

type Status = 'pass' | 'warn' | 'fail' | 'error' | 'neutral';

function computeStatus(result: IpResult): Status {
  if (result.error) return 'error';
  const v = result.validation;
  if (!v) return 'neutral';
  if (v.any_expired || !v.chain_trusted || !v.leaf_covers_hostname) return 'fail';
  if (v.earliest_expiry_days <= 30) return 'warn';
  return 'pass';
}

function cipherShort(cipher: string): string {
  const parts = cipher.split('_');
  // e.g. TLS_AES_256_GCM_SHA384 -> AES_256_GCM
  // e.g. TLS_CHACHA20_POLY1305_SHA256 -> CHACHA20_POLY1305
  if (parts.length >= 4 && parts[0] === 'TLS') {
    // Drop TLS_ prefix and last _SHAxxx part
    return parts.slice(1, -1).join('_');
  }
  return cipher;
}

function daysLabel(days: number): string {
  if (days < 0) return 'expired';
  if (days === 0) return 'today';
  return `${days}d`;
}

function daysClass(days: number): string {
  if (days < 0) return 'ip-card__days--fail';
  if (days <= 7) return 'ip-card__days--fail';
  if (days <= 30) return 'ip-card__days--warn';
  return 'ip-card__days--pass';
}

export default function IpCard(props: Props) {
  const [expanded, setExpanded] = createSignal(props.defaultExpanded);
  createEffect(() => { if (props.expanded !== undefined) setExpanded(props.expanded); });
  const [certsExpanded, setCertsExpanded] = createSignal(false);
  createEffect(() => { if (props.expanded !== undefined) setCertsExpanded(props.expanded); });

  const status = () => computeStatus(props.result);
  const leaf = () => props.result.chain?.[0];
  const tls = () => props.result.tls;
  const hasBody = () => !props.result.error && (props.result.tls || props.result.chain);
  const hasCerts = () => (props.result.chain?.length ?? 0) > 0;

  return (
    <div class={`ip-card ip-card--${status()}`} data-card>
      <button
        class="ip-card__header"
        onClick={() => hasBody() && setExpanded(!expanded())}
        disabled={!hasBody()}
      >
        <div class="ip-card__left">
          <span class="ip-card__ip">{props.result.ip}</span>
          <span class="ip-card__version">{props.result.ip_version}</span>
          <Show when={props.result.enrichment}>
            {(e) => <IpBadges info={e()} />}
          </Show>
        </div>

        <Show when={props.result.error}>
          {(err) => <span class="ip-card__error">{err().message}</span>}
        </Show>

        <Show when={tls()}>
          {(t) => (
            <div class="ip-card__meta">
              <span class="ip-card__badge">{t().version}</span>
              <span class="ip-card__badge">{cipherShort(t().cipher_suite)}</span>
              <span class="ip-card__badge">{t().handshake_ms}ms</span>
            </div>
          )}
        </Show>

        <Show when={leaf()}>
          {(l) => (
            <span class={`ip-card__days ${daysClass(l().days_remaining)}`}>
              {daysLabel(l().days_remaining)}
            </span>
          )}
        </Show>

        <Show when={props.ipUrl}>
          <a
            class="eco-link"
            href={`${props.ipUrl}/?ip=${encodeURIComponent(props.result.ip)}`}
            target="_blank"
            rel="noopener noreferrer"
            title="Open in IP inspector"
            onClick={(e: MouseEvent) => e.stopPropagation()}
          >IP &#x2197;</a>
        </Show>
        <Show when={hasBody()}>
          <span class="ip-card__chevron" classList={{ 'ip-card__chevron--open': expanded() }}>
            &#x25B8;
          </span>
        </Show>
      </button>
      <Explain when={!!props.explain}>Each card shows the TLS inspection result for one IP address. The left border color indicates overall status: green = good, orange = warning, red = problem.</Explain>

      <Show when={!props.result.error && leaf()}>
        {(l) => (
          <div class="ip-card__leaf-summary">
            <span>{certDisplayName(l().subject)}</span>
            <span class="ip-card__leaf-sep">|</span>
            <span>{l().issuer.replace(/^.*O=/, '').replace(/,.*$/, '')}</span>
            <span class="ip-card__leaf-sep">|</span>
            <span>{l().key_type} {l().key_size > 0 ? l().key_size : ''}</span>
          </div>
        )}
      </Show>

      <Show when={props.result.validation}>
        {(v) => (
          <>
            <div class="ip-card__validation">
              <span class={`ip-card__chip ${v().chain_trusted ? 'ip-card__chip--pass' : 'ip-card__chip--fail'}`}>
                {v().chain_trusted ? 'trusted' : 'untrusted'}
              </span>
              <span class={`ip-card__chip ${v().leaf_covers_hostname ? 'ip-card__chip--pass' : 'ip-card__chip--fail'}`}>
                {v().leaf_covers_hostname ? 'hostname ok' : 'hostname mismatch'}
              </span>
              <span class={`ip-card__chip ${!v().any_expired ? 'ip-card__chip--pass' : 'ip-card__chip--fail'}`}>
                {v().any_expired ? 'expired' : 'not expired'}
              </span>
            </div>
            <Show when={!v().chain_trusted && v().chain_trust_reason}>
              {(reason) => <div class="validation-reason">{explainTrustReason(reason())}</div>}
            </Show>
          </>
        )}
      </Show>

      <Show when={expanded() && hasBody()}>
        <div class="ip-card__body">
          <Show when={props.result.tls}>
            {(t) => <TlsParams params={t()} explain={props.explain} expanded={props.expanded} />}
          </Show>

          <Show when={props.result.ct}>
            {(ct) => <CtView ct={ct()} explain={props.explain} expanded={props.expanded} />}
          </Show>

          <Show when={props.result.chain}>
            {(chain) => (
              <>
                <ChainView chain={chain()} explain={props.explain} validation={props.result.validation} />
                <Show when={hasCerts()}>
                  <div class="ip-card__cert-toolbar">
                    <button
                      class="detail-toggle"
                      onClick={() => setCertsExpanded(!certsExpanded())}
                    >
                      {certsExpanded() ? 'collapse all' : 'expand all'}
                    </button>
                  </div>
                </Show>
                <div class="cert-details">
                  <For each={chain()}>
                    {(cert) => <CertDetail cert={cert} expanded={certsExpanded()} explain={props.explain} dnsUrl={props.dnsUrl} host={props.host} port={props.port} />}
                  </For>
                </div>
              </>
            )}
          </Show>
        </div>
      </Show>
    </div>
  );
}
