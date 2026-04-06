import { Show } from 'solid-js';

interface CrossLinksProps {
  hostname?: string;
}

export function CrossLinks(props: CrossLinksProps) {
  const dnsUrl = () => props.hostname
    ? `https://dns.netray.info/?q=${encodeURIComponent(props.hostname)}`
    : 'https://dns.netray.info/';

  return (
    <Show when={props.hostname}>
      <div class="cross-links">
        <a href={dnsUrl()} class="cross-link" target="_self">
          Check DNS for {props.hostname} →
        </a>
      </div>
    </Show>
  );
}
