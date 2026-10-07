import { Show } from 'solid-js';
import CrossLink from '@netray-info/common-frontend/components/CrossLink';

interface DnsCrossLinksProps {
  domain?: string;
  tlsUrl?: string | null;
}

export function DnsCrossLinks(props: DnsCrossLinksProps) {
  const domain = () => {
    const raw = props.domain ?? '';
    // Extract just the domain part (strip record types, flags, server specs)
    return raw.split(/\s+/)[0].replace(/\.+$/, '');
  };

  const href = () => props.tlsUrl
    ? `${props.tlsUrl}/?h=${encodeURIComponent(domain())}`
    : null;

  return (
    <Show when={domain() && href()}>
      <div class="cross-links">
        <CrossLink href={href()!} label={`Inspect TLS for ${domain()}`} />
      </div>
    </Show>
  );
}
