import { Show } from 'solid-js';
import CrossLink from '@netray-info/common-frontend/components/CrossLink';

interface CrossLinksProps {
  hostname?: string;
  dnsUrl?: string | null;
}

export function CrossLinks(props: CrossLinksProps) {
  const href = () => {
    const base = props.dnsUrl ?? 'https://dns.netray.info/';
    return props.hostname
      ? `${base}?q=${encodeURIComponent(props.hostname)}`
      : base;
  };

  return (
    <Show when={props.hostname}>
      <div class="cross-links">
        <CrossLink href={href()}>
          Check DNS for {props.hostname} →
        </CrossLink>
      </div>
    </Show>
  );
}
