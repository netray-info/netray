import { For, Show, type JSX } from 'solid-js';
import type { SiteMeta } from '../lib/types';
import { GRADE_LEGEND } from '../lib/gradeLegend';

interface Props {
  site?: SiteMeta;
  onExampleClick: (domain: string) => void;
  /** The DomainInput, slotted between the hero and the example chips so the
   *  visual order is: hero → input → chips → legend → trust → callout
   *  (SDD §3 Requirement 10). */
  children?: JSX.Element;
}

// Apex landing-state content (idle, no `?d=` param). Per SDD product-
// repositioning §3 Requirement 10: hero heading + subheading + status pill,
// domain input, example chips, grade legend, trust strip, deeper-callout.
// Brand + tagline are rendered by the persistent <header> in App.tsx (also
// meta-driven).
//
// Reads strings from meta.features.site.* with hardcoded fallbacks so the
// component renders sensibly when /api/meta has not loaded or is missing.
export default function Landing(props: Props) {
  const heroHeading = () =>
    props.site?.hero_heading ?? 'How healthy is your domain?';

  const heroSubheading = () =>
    props.site?.hero_subheading ??
    'DNS, TLS, HTTP, email, and the IPs behind them — checked in parallel, one grade, usually under a second.';

  const statusPill = () =>
    props.site?.status_pill ?? 'open source · self-hosted · built in Rust';

  const exampleDomains = () =>
    props.site?.example_domains ?? ['example.com', 'github.com', 'cloudflare.com'];

  const trustStrip = () =>
    props.site?.trust_strip ?? 'No account · No ads · Open source · Self-hostable';

  return (
    <div class="landing">
      <section class="landing__hero">
        <h2 class="landing__heading">{heroHeading()}</h2>
        <p class="landing__subheading">{heroSubheading()}</p>
        <Show when={statusPill()}>
          <span class="landing__status-pill">{statusPill()}</span>
        </Show>
      </section>

      <Show when={props.children}>
        <div class="landing__input-slot">{props.children}</div>
      </Show>

      <section class="landing__examples" aria-label="Example domains">
        <p class="landing__examples-label">Try one:</p>
        <div class="landing__chips">
          <For each={exampleDomains()}>
            {(domain) => (
              <button
                class="example-chip"
                type="button"
                onClick={() => props.onExampleClick(domain)}
              >{domain}</button>
            )}
          </For>
        </div>
      </section>

      <section class="landing__legend" aria-label="Grade legend">
        <h3 class="landing__legend-title">What the grades mean</h3>
        <ul class="landing__legend-list" role="list">
          <For each={GRADE_LEGEND}>
            {(entry) => (
              <li class="landing__legend-item">
                <span class={`landing__legend-grade landing__legend-grade--${entry.grade.replace('+', 'plus')}`}>
                  {entry.grade}
                </span>
                <span class="landing__legend-descriptor">{entry.descriptor}</span>
                <span class="landing__legend-meaning">— {entry.meaning}</span>
              </li>
            )}
          </For>
        </ul>
      </section>

      <p class="landing__trust">{trustStrip()}</p>

      <p class="landing__deeper">
        Want to look deeper? →{' '}
        <a class="landing__deeper-link" href="/tools">Raw data for every check</a>
      </p>
    </div>
  );
}
