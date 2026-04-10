import { Show } from 'solid-js';
import type { Accessor } from 'solid-js';
import type { CachingReport, CdnReport, QualityCheck } from '../lib/types';

interface Props {
  caching: CachingReport;
  cdn: CdnReport;
  check?: QualityCheck;
  showExplanations: Accessor<boolean>;
}

function formatLastModified(raw: string): string {
  const d = new Date(raw);
  if (isNaN(d.getTime())) return raw;
  return d.toLocaleString(undefined, {
    year: 'numeric', month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit', timeZoneName: 'short',
  });
}

function formatEtag(raw: string): string {
  // HTTP ETag format: "value" (strong) or W/"value" (weak)
  // Strip surrounding quotes; keep W/ prefix as "weak: "
  if (raw.startsWith('W/')) {
    return 'weak: ' + raw.slice(2).replace(/^"|"$/g, '');
  }
  return raw.replace(/^"|"$/g, '');
}

export default function CachingView(props: Props) {
  const d = () => props.caching.directives;

  return (
    <>
      <Show when={props.check?.message}>
        <p class="cache-check-message">{props.check!.message}</p>
      </Show>

      <Show when={props.caching.cache_control}>
        <p class="mono cache-value">
          Cache-Control: {props.caching.cache_control}
        </p>
      </Show>

      <ul class="cache-directive-list">
        <Show when={d().max_age != null}>
          <li>max-age: {d().max_age}s</li>
        </Show>
        <Show when={d().no_store}><li>no-store</li></Show>
        <Show when={d().no_cache}><li>no-cache</li></Show>
        <Show when={d().public}><li>public</li></Show>
        <Show when={d().private}><li>private</li></Show>
        <Show when={d().must_revalidate}><li>must-revalidate</li></Show>
        <Show when={d().immutable}><li>immutable</li></Show>
      </ul>

      <ul class="cache-vary-list">
        <li>
          ETag:{' '}
          {props.caching.etag
            ? <span class="mono cache-validator--value">{formatEtag(props.caching.etag)}</span>
            : <span class="cache-validator--absent">not set</span>}
        </li>
        <li>
          Last-Modified:{' '}
          {props.caching.last_modified
            ? <span class="cache-validator--value">{formatLastModified(props.caching.last_modified)}</span>
            : <span class="cache-validator--absent">not set</span>}
        </li>
        <Show when={props.caching.vary.length > 0}>
          <li>Vary: {props.caching.vary.join(', ')}</li>
        </Show>
        <Show when={props.caching.age != null}>
          <li>Age: {props.caching.age}s</li>
        </Show>
      </ul>

      <Show when={props.cdn.detected}>
        <div class="cache-cdn-section">
          <div class="section-label">CDN</div>
          <p class="cache-cdn-desc">
            Detected: <strong>{props.cdn.detected}</strong>
            <Show when={props.cdn.cache_status}>
              {' '}&mdash; Cache status: {props.cdn.cache_status}
            </Show>
          </p>
          <Show when={props.cdn.indicators.length > 0}>
            <p class="mono cache-indicators">
              Indicators: {props.cdn.indicators.join(', ')}
            </p>
          </Show>
        </div>
      </Show>

      <Show when={props.showExplanations() && props.check?.explanation}>
        <p class="check-explain">{props.check!.explanation}</p>
      </Show>
    </>
  );
}
