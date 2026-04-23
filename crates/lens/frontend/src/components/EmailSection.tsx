import { createEffect, createSignal, For, Show } from 'solid-js';
import type { EmailEvent, EmailBucket } from '../lib/types';
import VerdictDot from './VerdictDot';
import SectionSkeleton from './SectionSkeleton';

interface Props {
  data: EmailEvent | null;
  loading: boolean;
  error?: string;
  expanded?: boolean;
  grade?: string;
}

const BUCKET_LABELS: Record<string, string> = {
  email_authentication: 'Authentication',
  email_infrastructure: 'Infrastructure',
  email_transport: 'Transport',
  email_brand_policy: 'Brand Policy',
};

const BUCKET_ORDER = ['email_authentication', 'email_infrastructure', 'email_transport', 'email_brand_policy'] as const;

function bucketVerdict(bucket: EmailBucket): string {
  if (bucket.not_applicable) return 'N/A';
  switch (bucket.verdict) {
    case 'pass': return 'OK';
    case 'warn': return 'Warn';
    case 'fail': return 'Fail';
    case 'skip': return 'N/A';
    default: return bucket.verdict;
  }
}

function bucketVerdictClass(bucket: EmailBucket): string {
  if (bucket.not_applicable) return 'email-bucket__verdict--na';
  switch (bucket.verdict) {
    case 'pass': return 'email-bucket__verdict--pass';
    case 'warn': return 'email-bucket__verdict--warn';
    case 'fail': return 'email-bucket__verdict--fail';
    default: return 'email-bucket__verdict--na';
  }
}

function overallStatus(data: EmailEvent): string {
  if (data.status === 'error' || data.status === 'not_applicable') return data.status;
  if (!data.buckets) return 'error';
  const buckets = Object.values(data.buckets);
  if (buckets.some(b => !b.not_applicable && b.verdict === 'fail')) return 'fail';
  if (buckets.some(b => !b.not_applicable && b.verdict === 'warn')) return 'warn';
  return 'pass';
}

export default function EmailSection(props: Props) {
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
                  <span class="section-card__title">Email</span>
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
          const status = () => overallStatus(data());
          return (
            <div class="section-card">
              <div
                class="section-card__header"
                onClick={() => setOpen(v => !v)}
                role="button"
                tabIndex={0}
                aria-expanded={open()}
                onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setOpen(v => !v); } }}
              >
                <VerdictDot verdict={status() as any} grade={props.grade} />
                <span class="section-card__title">Email</span>
                <Show when={status() === 'error'} fallback={
                  <Show when={status() === 'not_applicable'} fallback={
                    <span class="section-card__headline">{data().headline ?? ''}</span>
                  }>
                    <span class="section-card__error">unavailable (timed out)</span>
                  </Show>
                }>
                  <span class="section-card__error">{data().error ?? 'unavailable'}</span>
                </Show>
                <Show when={data().detail_url}>
                  <a
                    class="ext-link"
                    href={data().detail_url}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={(e) => e.stopPropagation()}
                  >
                    Email ↗
                  </a>
                </Show>
                <span class={`section-card__chevron${open() ? ' section-card__chevron--open' : ''}`}>▼</span>
              </div>
              <Show when={open()}>
                <div class="section-card__body">
                  <Show
                    when={data().buckets}
                    fallback={
                      <div class="email-section__unavailable">
                        <Show when={status() === 'not_applicable'} fallback={
                          <span>Email check unavailable</span>
                        }>
                          <span>Email check unavailable (timed out)</span>
                        </Show>
                      </div>
                    }
                  >
                    {(buckets) => (
                      <div class="email-buckets">
                        <For each={BUCKET_ORDER}>
                          {(key) => {
                            const bucket = () => (buckets() as any)[key] as EmailBucket | undefined;
                            return (
                              <Show when={bucket()}>
                                {(b) => (
                                  <div class="email-bucket">
                                    <span class="email-bucket__label">{BUCKET_LABELS[key] ?? key}</span>
                                    <span class={`email-bucket__verdict ${bucketVerdictClass(b())}`}>
                                      {bucketVerdict(b())}
                                    </span>
                                    <Show when={b().not_applicable}>
                                      <span class="email-bucket__na-msg">No MX records — email receiving not configured</span>
                                    </Show>
                                    <Show when={!b().not_applicable && b().messages.length > 0}>
                                      <ul class="email-bucket__messages">
                                        <For each={b().messages}>
                                          {(msg) => <li>{msg}</li>}
                                        </For>
                                      </ul>
                                    </Show>
                                  </div>
                                )}
                              </Show>
                            );
                          }}
                        </For>
                      </div>
                    )}
                  </Show>
                </div>
              </Show>
            </div>
          );
        }}
      </Show>
    </Show>
  );
}
