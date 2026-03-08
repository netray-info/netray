import { Show, For } from 'solid-js';
import type { DnsContext, TlsaInfo } from '../lib/types';

export function CaaView(props: { caa: DnsContext['caa'] }) {
  return (
    <Show when={props.caa}>
      {(caa) => (
        <div class="dns-section">
          <h3 class="dns-section__title">
            CAA Records
            <Show when={caa().issuer_allowed !== null}>
              <span class={`badge badge--${caa().issuer_allowed ? 'pass' : 'fail'}`}>
                {caa().issuer_allowed ? 'Issuer Allowed' : 'Issuer Not Allowed'}
              </span>
            </Show>
          </h3>
          <Show when={caa().records.length > 0} fallback={<p class="dns-section__empty">No CAA records found</p>}>
            <ul class="dns-records">
              <For each={caa().records}>
                {(record) => <li class="dns-records__item mono">{record}</li>}
              </For>
            </ul>
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
