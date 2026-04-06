import { Show } from 'solid-js';

interface DnsCrossLinksProps {
  domain?: string;
}

export function DnsCrossLinks(props: DnsCrossLinksProps) {
  const domain = () => {
    const raw = props.domain ?? '';
    // Extract just the domain part (strip record types, flags, server specs)
    return raw.split(/\s+/)[0].replace(/\.+$/, '');
  };

  const tlsUrl = () => `https://tls.netray.info/?h=${encodeURIComponent(domain())}`;

  return (
    <Show when={domain()}>
      <div class="cross-links">
        <a href={tlsUrl()} class="cross-link">Inspect TLS for {domain()} →</a>
      </div>
    </Show>
  );
}
