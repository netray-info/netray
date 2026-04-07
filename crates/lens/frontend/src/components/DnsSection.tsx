import { createSignal, Show } from 'solid-js';
import type { DnsEvent } from '../lib/types';
import VerdictDot from './VerdictDot';
import CheckList from './CheckList';
import SectionSkeleton from './SectionSkeleton';

interface Props {
  data: DnsEvent | null;
  loading: boolean;
  error?: string;
}

export default function DnsSection(props: Props) {
  const [open, setOpen] = createSignal(true);

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
                  <span class="section-card__title">DNS</span>
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
              <span class="section-card__title">DNS</span>
              <span class="section-card__headline">{data().headline}</span>
              <span class={`section-card__chevron${open() ? ' section-card__chevron--open' : ''}`}>▼</span>
            </div>
            <Show when={open()}>
              <div class="section-card__body">
                <CheckList checks={data().checks} />
                <a
                  class="section-detail-link"
                  href={data().detail_url}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  View in prism →
                </a>
              </div>
            </Show>
          </div>
        )}
      </Show>
    </Show>
  );
}
