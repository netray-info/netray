import { Show, For } from 'solid-js';
import type { CachingReport, CdnReport } from '../lib/types';

interface Props {
  caching: CachingReport;
  cdn: CdnReport;
}

export default function CachingView(props: Props) {
  const d = () => props.caching.directives;

  return (
    <div class="section">
      <div class="section__title">Caching</div>

      <Show when={props.caching.cache_control}>
        <p class="mono" style={{ 'font-size': '0.8125rem', 'margin-bottom': '0.5rem' }}>
          Cache-Control: {props.caching.cache_control}
        </p>
      </Show>

      <ul style={{ 'list-style': 'none', padding: '0', margin: '0 0 0.75rem 0', 'font-size': '0.875rem' }}>
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

      <ul style={{ 'list-style': 'none', padding: '0', margin: '0', 'font-size': '0.875rem' }}>
        <li>ETag: {props.caching.etag ? 'Yes' : 'No'}</li>
        <li>Last-Modified: {props.caching.last_modified ? 'Yes' : 'No'}</li>
        <Show when={props.caching.vary.length > 0}>
          <li>Vary: {props.caching.vary.join(', ')}</li>
        </Show>
        <Show when={props.caching.age != null}>
          <li>Age: {props.caching.age}s</li>
        </Show>
      </ul>

      <Show when={props.cdn.detected}>
        <div style={{ 'margin-top': '0.75rem' }}>
          <div class="section__title">CDN</div>
          <p style={{ 'font-size': '0.875rem' }}>
            Detected: <strong>{props.cdn.detected}</strong>
            <Show when={props.cdn.cache_status}>
              {' '}&mdash; Cache status: {props.cdn.cache_status}
            </Show>
          </p>
          <Show when={props.cdn.indicators.length > 0}>
            <p class="mono" style={{ 'font-size': '0.8125rem', color: 'var(--text-muted)' }}>
              Indicators: {props.cdn.indicators.join(', ')}
            </p>
          </Show>
        </div>
      </Show>
    </div>
  );
}
