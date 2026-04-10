import { createEffect, createSignal, Show } from 'solid-js';
import type { HttpEvent } from '../lib/types';
import VerdictDot from './VerdictDot';
import CheckList from './CheckList';
import SectionHeadline from './SectionHeadline';
import SectionSkeleton from './SectionSkeleton';

interface Props {
  data: HttpEvent | null;
  loading: boolean;
  error?: string;
  expanded?: boolean;
}

export default function HttpSection(props: Props) {
  const [open, setOpen] = createSignal(false);
  createEffect(() => { if (props.expanded !== undefined) setOpen(props.expanded); });

  return (
    <Show
      when={!props.loading || props.data}
      fallback={<SectionSkeleton />}
    >
      <Show
        when={props.data}
        fallback={
          props.error
            ? (
              <div class="section-card">
                <div class="section-card__header">
                  <span class="verdict-dot verdict-dot--error" />
                  <span class="section-card__title">HTTP</span>
                </div>
                <div class="section-card__body">
                  <span style={{ color: 'var(--verdict-error)', 'font-size': '0.875rem' }}>{props.error}</span>
                </div>
              </div>
            )
            : null
        }
      >
        {(data) => {
          const hasServerInfo = () =>
            data().status_code !== undefined ||
            data().http_version !== undefined ||
            data().response_duration_ms !== undefined ||
            data().server_ip !== undefined ||
            data().server_org !== undefined;

          return (
            <div class="section-card">
              <div
                class="section-card__header"
                onClick={() => setOpen(v => !v)}
                role="button"
                tabIndex={0}
                aria-expanded={open()}
                onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setOpen(v => !v); } }}
                style={{ 'flex-wrap': 'wrap', 'row-gap': '0.375rem' }}
              >
                <VerdictDot verdict={data().status} />
                <span class="section-card__title">HTTP</span>
                <Show when={data().status === 'error'} fallback={
                  <SectionHeadline checks={data().checks} />
                }>
                  <span class="section-card__error">{data().headline}</span>
                </Show>
                <a
                  class="section-card__link"
                  href={data().detail_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={(e) => e.stopPropagation()}
                >
                  HTTP ↗
                </a>
                <span class={`section-card__chevron${open() ? ' section-card__chevron--open' : ''}`}>▼</span>
                <Show when={hasServerInfo()}>
                  <div class="http-meta" onClick={(e) => e.stopPropagation()}>
                    <Show when={data().status_code !== undefined}>
                      <span class="http-meta__item">
                        <span class="http-meta__label">Status</span>
                        <span class="http-meta__value">{data().status_code}</span>
                      </span>
                    </Show>
                    <Show when={data().http_version}>
                      <span class="http-meta__item">
                        <span class="http-meta__label">Protocol</span>
                        <span class="http-meta__value">{data().http_version}</span>
                      </span>
                    </Show>
                    <Show when={data().response_duration_ms !== undefined}>
                      <span class="http-meta__item">
                        <span class="http-meta__label">Duration</span>
                        <span class="http-meta__value http-meta__value--timing">{data().response_duration_ms}ms</span>
                      </span>
                    </Show>
                    <Show when={data().server_ip}>
                      <span class="http-meta__item">
                        <span class="http-meta__label">IP</span>
                        <span class="http-meta__value http-meta__value--mono">{data().server_ip}</span>
                      </span>
                    </Show>
                    <Show when={data().server_org}>
                      <span class="http-meta__item">
                        <span class="http-meta__label">Org</span>
                        <span class="http-meta__value">{data().server_org}</span>
                      </span>
                    </Show>
                  </div>
                </Show>
              </div>
              <Show when={open()}>
                <div class="section-card__body">
                  <CheckList checks={data().checks} />
                </div>
              </Show>
            </div>
          );
        }}
      </Show>
    </Show>
  );
}
