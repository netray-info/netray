import { createSignal, Show, For } from 'solid-js';
import type { DnsContext, TlsaInfo } from '../lib/types';

function parseCaaRecord(raw: string): string {
  // Format: 'issue "digicert.com"' or 'iodef "mailto:..."' (optional leading flags digit)
  const m = raw.match(/^(?:\d+\s+)?(\S+)\s+"(.+?)"\s*$/);
  if (!m) return raw;
  const [, tag, value] = m;
  if (tag === 'iodef') {
    return `violation report: ${value.replace(/^mailto:/, '')}`;
  }
  if (tag === 'issuewild') {
    return `issue wildcard: ${value}`;
  }
  return `${tag}: ${value}`;
}

export function CaaView(props: { caa: DnsContext['caa'] }) {
  const [expanded, setExpanded] = createSignal(false);

  return (
    <Show when={props.caa}>
      {(caa) => (
        <div class="dns-section" data-card>
          <button class="dns-section__toggle" onClick={() => setExpanded(!expanded())}>
            <span class="dns-section__toggle-left">
              CAA Records
              <Show when={caa().issuer_allowed !== null}>
                <span class={`badge badge--${caa().issuer_allowed ? 'pass' : 'fail'}`}>
                  {caa().issuer_allowed ? 'Issuer Allowed' : 'Issuer Not Allowed'}
                </span>
              </Show>
              <Show when={caa().records.length > 0}>
                <span class="dns-section__count">{caa().records.length}</span>
              </Show>
            </span>
            <span class="ip-card__chevron" classList={{ 'ip-card__chevron--open': expanded() }}>
              &#x25B8;
            </span>
          </button>
          <Show when={expanded()}>
            <div class="dns-section__body">
              <Show when={caa().records.length > 0} fallback={<p class="dns-section__empty">No CAA records found — any CA may issue.</p>}>
                <ul class="dns-records">
                  <For each={[...caa().records].map(parseCaaRecord).sort()}>
                    {(record) => <li class="dns-records__item mono">{record}</li>}
                  </For>
                </ul>
              </Show>
            </div>
          </Show>
        </div>
      )}
    </Show>
  );
}

export function TlsaView(props: { tlsa: TlsaInfo }) {
  return (
    <div class="dns-section">
      <h3 class="dns-section__title">
        TLSA Records
        <Show when={!props.tlsa.dnssec_signed}>
          <span class="badge badge--skip">No DNSSEC</span>
        </Show>
      </h3>
      <ul class="dns-records">
        <For each={props.tlsa.records}>
          {(record) => <li class="dns-records__item mono">{record}</li>}
        </For>
      </ul>
    </div>
  );
}
