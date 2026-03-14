import { type Component, Show } from 'solid-js';

export interface CrossLinksProps {
  /** Hostname context — used for TLS and DNS links. */
  hostname?: string;
  /** IP address context — used for IP info link. */
  ip?: string;
  /** Base URL for the DNS inspector (mhost-prism), e.g. "https://dns.netray.info". */
  dnsUrl?: string | null;
  /** Base URL for the IP info service (ifconfig-rs), e.g. "https://ip.netray.info". */
  ipUrl?: string | null;
  /** Base URL for the TLS inspector (tlsight), e.g. "https://tls.netray.info". */
  tlsUrl?: string | null;
  /** Identifies the originating service for the ?ref= param (e.g. "tlsight", "prism", "ifconfig"). */
  ref: string;
}

/** Inline ecosystem cross-links. Renders only the links that are applicable given the props. */
const CrossLinks: Component<CrossLinksProps> = (props) => {
  const dnsHref = () => {
    if (!props.dnsUrl || !props.hostname) return null;
    return `${props.dnsUrl}/?q=${encodeURIComponent(props.hostname)}&ref=${encodeURIComponent(props.ref)}`;
  };

  const tlsHref = () => {
    if (!props.tlsUrl || !props.hostname) return null;
    return `${props.tlsUrl}/?h=${encodeURIComponent(props.hostname)}&ref=${encodeURIComponent(props.ref)}`;
  };

  const ipHref = () => {
    if (!props.ipUrl || !props.ip) return null;
    return `${props.ipUrl}/?ip=${encodeURIComponent(props.ip)}&ref=${encodeURIComponent(props.ref)}`;
  };

  const hasAny = () => dnsHref() !== null || tlsHref() !== null || ipHref() !== null;

  return (
    <Show when={hasAny()}>
      <span class="cross-links">
        <Show when={dnsHref()}>
          {(href) => (
            <a
              class="eco-link"
              href={href()}
              target="_blank"
              rel="noopener noreferrer"
            >DNS ↗</a>
          )}
        </Show>
        <Show when={tlsHref()}>
          {(href) => (
            <a
              class="eco-link"
              href={href()}
              target="_blank"
              rel="noopener noreferrer"
            >TLS ↗</a>
          )}
        </Show>
        <Show when={ipHref()}>
          {(href) => (
            <a
              class="eco-link"
              href={href()}
              target="_blank"
              rel="noopener noreferrer"
            >IP ↗</a>
          )}
        </Show>
      </span>
    </Show>
  );
};

export default CrossLinks;
