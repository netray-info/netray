import { createEffect, createSignal, For, Show } from 'solid-js';
import type { IpEvent } from '../lib/types';
import VerdictDot from './VerdictDot';
import CheckList from './CheckList';
import SectionHeadline from './SectionHeadline';
import SectionSkeleton from './SectionSkeleton';

function networkTypeBadgeClass(networkType: string): string {
  switch (networkType.toLowerCase()) {
    case 'residential': return 'ip-badge ip-badge--network ip-badge--residential';
    case 'datacenter':  return 'ip-badge ip-badge--network ip-badge--datacenter';
    case 'vpn':         return 'ip-badge ip-badge--network ip-badge--vpn';
    case 'tor':         return 'ip-badge ip-badge--network ip-badge--tor';
    default:            return 'ip-badge ip-badge--network';
  }
}

interface Props {
  data: IpEvent | null;
  loading: boolean;
  error?: string;
  explain: boolean;
  expanded?: boolean;
}

export default function IpSection(props: Props) {
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
                  <span class="section-card__title">IP</span>
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
              <span class="section-card__title">IP</span>
              <SectionHeadline checks={data().checks} />
              <Show when={data().guide_url}>
                <a
                  class="check-item__guide-link"
                  href={data().guide_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  onClick={(e) => e.stopPropagation()}
                >
                  Learn why
                </a>
              </Show>
              <a
                class="section-card__link"
                href={data().detail_url}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(e) => e.stopPropagation()}
              >
                IP ↗
              </a>
              <span class={`section-card__chevron${open() ? ' section-card__chevron--open' : ''}`}>▼</span>
            </div>
            <Show when={open()}>
              <div class="section-card__body">
                <ul class="ip-list" role="list">
                  <For each={data().addresses}>
                    {(addr) => (
                      <li class="ip-item">
                        <span class="ip-item__addr">{addr.ip}</span>
                        <Show when={addr.org}>
                          <span class="ip-badge">{addr.org}</span>
                        </Show>
                        <Show when={addr.geo}>
                          <span class="ip-badge">{addr.geo}</span>
                        </Show>
                        <span class={networkTypeBadgeClass(addr.network_type)}>
                          {addr.network_type}
                        </span>
                      </li>
                    )}
                  </For>
                </ul>
                <CheckList checks={data().checks} explain={props.explain} />
              </div>
            </Show>
          </div>
        )}
      </Show>
    </Show>
  );
}
