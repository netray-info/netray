import { Show } from 'solid-js';

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

  const tlsUrl = () => props.tlsUrl
    ? `${props.tlsUrl}/?h=${encodeURIComponent(domain())}`
    : null;

  return (
    <Show when={domain() && tlsUrl()}>
      <div class="cross-links">
        <a href={tlsUrl()!} class="cross-link">Inspect TLS for {domain()} &rarr;</a>
      </div>
    </Show>
  );
}
