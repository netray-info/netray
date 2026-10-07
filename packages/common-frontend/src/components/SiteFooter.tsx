import { type Component, type JSX, For, Show } from 'solid-js';

interface FooterLink {
  href: string;
  label: string;
  external?: boolean;
}

interface SiteFooterProps {
  aboutText: JSX.Element | string;
  links: FooterLink[];
  version?: string;
}

const SiteFooter: Component<SiteFooterProps> = (props) => (
  <footer class="footer">
    <div class="footer-about">{props.aboutText}</div>
    <div class="footer-links">
      <For each={props.links}>
        {(link, i) => (
          <>
            <Show when={i() > 0}>
              <span class="footer-sep">&middot;</span>
            </Show>
            <a
              class="footer-link"
              href={link.href}
              target={link.external ? '_blank' : undefined}
              rel={link.external ? 'noopener noreferrer' : undefined}
            >
              {link.label}
            </a>
          </>
        )}
      </For>
      <Show when={props.version}>
        <span class="footer-sep">&middot;</span>
        <span class="footer-text">v{props.version}</span>
      </Show>
    </div>
  </footer>
);

export default SiteFooter;
