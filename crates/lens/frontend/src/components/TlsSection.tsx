import { createEffect, createSignal, Show } from 'solid-js';
import type { TlsEvent } from '../lib/types';
import VerdictDot from './VerdictDot';
import CheckList from './CheckList';
import SectionHeadline from './SectionHeadline';
import SectionSkeleton from './SectionSkeleton';

interface Props {
  data: TlsEvent | null;
  loading: boolean;
  error?: string;
  expanded?: boolean;
}

export default function TlsSection(props: Props) {
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
                  <span class="section-card__title">TLS</span>
                </div>
                <div class="section-card__body">
                  <span style={{ color: 'var(--verdict-error)', 'font-size': '0.875rem' }}>{props.error}</span>
                </div>
              </div>
            )
            : null
        }
      >
        {(data) => (
          <div class="section-card">
            <div
              class="section-card__header"
              onClick={() => setOpen(v => !v)}
              role="button"
              tabIndex={0}
              aria-expanded={open()}
              onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setOpen(v => !v); } }}
            >
              <VerdictDot verdict={data().status} />
              <span class="section-card__title">TLS</span>
              <SectionHeadline checks={data().checks} />
              <a
                class="section-card__link"
                href={data().detail_url}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(e) => e.stopPropagation()}
              >
                TLS ↗
              </a>
              <span class={`section-card__chevron${open() ? ' section-card__chevron--open' : ''}`}>▼</span>
            </div>
            <Show when={open()}>
              <div class="section-card__body">
                <CheckList checks={data().checks} />
              </div>
            </Show>
          </div>
        )}
      </Show>
    </Show>
  );
}
