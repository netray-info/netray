import { createSignal, createEffect, Show, For } from 'solid-js';
import Explain from './Explain';
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

export function CaaView(props: { caa: DnsContext['caa']; explain?: boolean; expanded?: boolean; dnsUrl?: string; hostname?: string }) {
  const [expanded, setExpanded] = createSignal(false);
  createEffect(() => { if (props.expanded !== undefined) setExpanded(props.expanded); });

  return (
    <Show when={props.caa}>
      {(caa) => (
        <div class="dns-section" data-card>
          <button class="dns-section__toggle" onClick={() => setExpanded(!expanded())} aria-expanded={expanded() ? "true" : "false"}>
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
            <Show when={props.dnsUrl && props.hostname}>
              <a
                class="eco-link"
                href={`${props.dnsUrl}/?q=${encodeURIComponent(props.hostname!)}+CAA&ref=tlsight`}
                target="_blank"
                rel="noopener noreferrer"
                title="Open in DNS inspector"
                onClick={(e: MouseEvent) => e.stopPropagation()}
              >DNS &#x2197;</a>
            </Show>
            <span class="ip-card__chevron" classList={{ 'ip-card__chevron--open': expanded() }}>
              &#x25B8;
            </span>
          </button>
          <Explain when={!!props.explain} guideUrl="/guide/caa-records.html">CAA (Certification Authority Authorization) DNS records declare which CAs are allowed to issue certificates for this domain. If the issuing CA is not listed, the certificate may violate the domain owner's policy.</Explain>
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

export function TlsaView(props: { tlsa: TlsaInfo; explain?: boolean; dnsUrl?: string; hostname?: string; port?: number }) {
  const [expanded, setExpanded] = createSignal(false);

  return (
    <div class="dns-section" data-card>
      <button class="dns-section__toggle" onClick={() => setExpanded(!expanded())} aria-expanded={expanded() ? "true" : "false"}>
        <span class="dns-section__toggle-left">
          TLSA Records
          <Show when={!props.tlsa.dnssec_signed}>
            <span class="badge badge--skip">No DNSSEC</span>
          </Show>
          <Show when={props.tlsa.records.length > 0}>
            <span class="dns-section__count">{props.tlsa.records.length}</span>
          </Show>
        </span>
        <Show when={props.dnsUrl && props.hostname}>
          <a
            class="eco-link"
            href={`${props.dnsUrl}/?q=${encodeURIComponent(`_${props.port ?? 443}._tcp.${props.hostname!}`)}+TLSA&ref=tlsight`}
            target="_blank"
            rel="noopener noreferrer"
            title="Open in DNS inspector"
            onClick={(e: MouseEvent) => e.stopPropagation()}
          >DNS &#x2197;</a>
        </Show>
        <span class="ip-card__chevron" classList={{ 'ip-card__chevron--open': expanded() }}>
          &#x25B8;
        </span>
      </button>
      <Explain when={!!props.explain} guideUrl="/guide/dane-tlsa.html">TLSA records enable DANE — pinning certificates or CAs in DNS via DNSSEC. This provides an alternative trust path independent of the CA system. Requires DNSSEC to be meaningful.</Explain>
      <Show when={expanded()}>
        <div class="dns-section__body">
          <Show when={props.tlsa.records.length > 0} fallback={<p class="dns-section__empty">No TLSA records found.</p>}>
            <ul class="dns-records">
              <For each={props.tlsa.records}>
                {(record) => <li class="dns-records__item mono">{record}</li>}
              </For>
            </ul>
          </Show>
        </div>
      </Show>
    </div>
  );
}
