import { For, Show } from 'solid-js';
import type { MetaResponse } from '../lib/types';

interface Props {
  meta: MetaResponse;
  hostname: string;
  ips: string[];
}

export default function CrossLinks(props: Props) {
  const dnsUrl = () => props.meta.ecosystem?.dns_base_url;
  const ipUrl = () => props.meta.ecosystem?.ip_base_url;
  const hasLinks = () => dnsUrl() || ipUrl();

  return (
    <Show when={hasLinks()}>
      <div class="cross-links">
        <h3>Ecosystem</h3>
        <div class="cross-links__list">
          <Show when={dnsUrl()}>
            <a
              class="cross-links__link"
              href={`${dnsUrl()}/?q=${encodeURIComponent(props.hostname)}`}
              target="_blank"
              rel="noopener"
            >
              View DNS records
            </a>
          </Show>
          <Show when={ipUrl()}>
            <For each={props.ips}>
              {(ip) => (
                <a
                  class="cross-links__link"
                  href={`${ipUrl()}/?ip=${encodeURIComponent(ip)}`}
                  target="_blank"
                  rel="noopener"
                >
                  IP info: {ip}
                </a>
              )}
            </For>
          </Show>
        </div>
      </div>
    </Show>
  );
}
