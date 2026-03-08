import { createSignal, Show, For } from 'solid-js';
import type { IpResult } from '../lib/types';
import { certDisplayName } from '../lib/cert';
import TlsParams from './TlsParams';
import CtView from './CtView';
import ChainView from './ChainView';
import CertDetail from './CertDetail';

interface Props {
  ips: IpResult[];
}

function daysLabel(days: number): string {
  if (days < 0) return 'expired';
  if (days === 0) return 'today';
  return `${days}d`;
}

function daysClass(days: number): string {
  if (days <= 7) return 'ip-card__days--fail';
  if (days <= 30) return 'ip-card__days--warn';
  return 'ip-card__days--pass';
}

export default function UnifiedIpView(props: Props) {
  const [certsExpanded, setCertsExpanded] = createSignal(false);

  const rep = () => props.ips[0];
  const leaf = () => rep().chain?.[0];
  const tls = () => rep().tls;
  const hasCerts = () => (rep().chain?.length ?? 0) > 0;

  return (
    <div class="unified-ip" data-card>
      <div class="unified-ip__header">
        <div class="unified-ip__ips">
          <For each={props.ips}>
            {(ip) => <span class="unified-ip__ip-badge">{ip.ip}</span>}
          </For>
        </div>
        <div class="unified-ip__header-right">
          <span class="unified-ip__consistent-badge">* consistent</span>
        </div>
      </div>

      <div class="unified-ip__timings">
        <For each={props.ips}>
          {(ip) => (
            <Show when={ip.tls}>
              {(t) => (
                <span class="unified-ip__timing">
                  {ip.ip} <span class="unified-ip__timing-val">{t().handshake_ms}ms</span>
                </span>
              )}
            </Show>
          )}
        </For>
      </div>

      <Show when={rep().validation}>
        {(v) => (
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
            <Show when={leaf()}>
              {(l) => (
                <span class={`ip-card__days ${daysClass(l().days_remaining)}`}>
                  {daysLabel(l().days_remaining)}
                </span>
              )}
            </Show>
          </div>
        )}
      </Show>

      <Show when={!rep().validation && leaf()}>
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

      <div class="ip-card__body">
        <Show when={tls()}>
          {(t) => <TlsParams params={t()} />}
        </Show>

        <Show when={rep().ct}>
          {(ct) => <CtView ct={ct()} />}
        </Show>

        <Show when={rep().chain}>
          {(chain) => (
            <>
              <ChainView chain={chain()} />
              <Show when={hasCerts()}>
                <div class="ip-card__cert-toolbar">
                  <button
                    class="detail-toggle"
                    onClick={() => setCertsExpanded(!certsExpanded())}
                  >
                    {certsExpanded() ? 'Collapse all' : 'Expand all'}
                  </button>
                </div>
              </Show>
              <div class="cert-details">
                <For each={chain()}>
                  {(cert) => <CertDetail cert={cert} expanded={certsExpanded()} />}
                </For>
              </div>
            </>
          )}
        </Show>
      </div>
    </div>
  );
}
