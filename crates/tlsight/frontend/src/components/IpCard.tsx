import { createSignal, Show, For } from 'solid-js';
import type { IpResult } from '../lib/types';
import TlsParams from './TlsParams';
import CtView from './CtView';
import ChainView from './ChainView';
import CertDetail from './CertDetail';

interface Props {
  result: IpResult;
  defaultExpanded: boolean;
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

  const status = () => computeStatus(props.result);
  const leaf = () => props.result.chain?.[0];
  const tls = () => props.result.tls;
  const hasBody = () => !props.result.error && (props.result.tls || props.result.chain);

  return (
    <div class={`ip-card ip-card--${status()}`}>
      <button
        class="ip-card__header"
        onClick={() => hasBody() && setExpanded(!expanded())}
        disabled={!hasBody()}
      >
        <div class="ip-card__left">
          <span class="ip-card__ip">{props.result.ip}</span>
          <span class="ip-card__version">{props.result.ip_version}</span>
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

        <Show when={hasBody()}>
          <span class="ip-card__chevron" classList={{ 'ip-card__chevron--open': expanded() }}>
            &#x25B8;
          </span>
        </Show>
      </button>

      <Show when={!props.result.error && leaf()}>
        {(l) => (
          <div class="ip-card__leaf-summary">
            <span>{l().subject}</span>
            <span class="ip-card__leaf-sep">|</span>
            <span>{l().issuer.replace(/^.*O=/, '').replace(/,.*$/, '')}</span>
            <span class="ip-card__leaf-sep">|</span>
            <span>{l().key_type} {l().key_size > 0 ? l().key_size : ''}</span>
          </div>
        )}
      </Show>

      <Show when={props.result.validation}>
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
          </div>
        )}
      </Show>

      <Show when={expanded() && hasBody()}>
        <div class="ip-card__body">
          <Show when={props.result.tls}>
            {(t) => <TlsParams params={t()} />}
          </Show>

          <Show when={props.result.ct}>
            {(ct) => <CtView ct={ct()} />}
          </Show>

          <Show when={props.result.chain}>
            {(chain) => (
              <>
                <ChainView chain={chain()} />
                <div class="cert-details">
                  <For each={chain()}>
                    {(cert) => <CertDetail cert={cert} />}
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
