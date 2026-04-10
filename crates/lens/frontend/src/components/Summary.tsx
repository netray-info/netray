import { For, Show } from 'solid-js';
import type { SummaryEvent, DoneEvent, IpAddress, Verdict } from '../lib/types';
import { CHECK_LABELS } from '../lib/checkMeta';
import VerdictDot from './VerdictDot';

function gradeStyle(grade: string): string {
  switch (grade) {
    case 'A+':
    case 'A':  return 'var(--grade-a)';
    case 'B':  return 'var(--grade-b)';
    case 'C':  return 'var(--grade-c)';
    case 'D':  return 'var(--grade-d)';
    default:   return 'var(--grade-f)';
  }
}

function overallLabel(v: Verdict): string {
  switch (v) {
    case 'pass':  return 'All checks passing';
    case 'warn':  return 'Warnings present';
    case 'fail':  return 'Failures detected';
    case 'error': return 'Check error';
    case 'skip':  return 'Skipped';
  }
}

function durationClass(ms: number): string {
  if (ms < 1000) return 'summary-duration summary-duration--fast';
  if (ms < 3000) return 'summary-duration summary-duration--ok';
  return 'summary-duration summary-duration--slow';
}

interface Props {
  summary: SummaryEvent;
  done: DoneEvent | null;
  addresses?: IpAddress[];
  ipDetailUrl?: string;
  httpServerIp?: string;
  httpServerOrg?: string;
  onCopyMd?: () => void;
  onDownloadJson?: () => void;
}

export default function Summary(props: Props) {
  const s = () => props.summary;
  const isError = () => s().grade === 'error';
  const sectionVerdict = (name: string): Verdict => s().sections[name] ?? 'error';
  const sectionGrade = (name: string): string | undefined => s().section_grades[name];
  const hasErroredSection = () =>
    sectionVerdict('dns') === 'error' || sectionVerdict('tls') === 'error' || sectionVerdict('ip') === 'error';
  const hasAddresses = () => (props.addresses?.length ?? 0) > 0 || !!props.httpServerIp;

  const dotsAndActions = () => (
    <>
      <div class="summary-dots" role="list" aria-label="Section statuses">
        <For each={['http', 'tls', 'dns', 'ip']}>
          {(section) => (
            <Show when={s().sections[section] !== undefined}>
              <div class="summary-dot-item" role="listitem">
                <VerdictDot verdict={sectionVerdict(section)} />
                <span>{section.toUpperCase()}</span>
                <Show when={sectionVerdict(section) === 'error'} fallback={
                  <Show when={sectionGrade(section)}>
                    <span class="summary-dot-grade" style={{ color: gradeStyle(sectionGrade(section)!) }}>
                      {sectionGrade(section)}
                    </span>
                  </Show>
                }>
                  <span class="summary-dot-error">err</span>
                </Show>
              </div>
            </Show>
          )}
        </For>
      </div>
      <Show when={props.onCopyMd || props.onDownloadJson}>
        <div class="summary-actions">
          <Show when={props.onCopyMd}>
            <button class="summary-action-btn" type="button" onClick={props.onCopyMd}>copy MD</button>
          </Show>
          <Show when={props.onCopyMd && props.onDownloadJson}>
            <span class="summary-action-sep">|</span>
          </Show>
          <Show when={props.onDownloadJson}>
            <button class="summary-action-btn" type="button" onClick={props.onDownloadJson}>JSON</button>
          </Show>
        </div>
      </Show>
    </>
  );

  return (
    <div class="summary-card" role="region" aria-label="Summary">
      <div class="summary-top">
        <Show when={isError()} fallback={
          <div class="summary-grade">
            <span
              class="summary-grade__letter"
              style={{ color: gradeStyle(s().grade) }}
              aria-label={`Grade ${s().grade}`}
            >
              {s().grade}
            </span>
            <div class="summary-grade__meta">
              <div class="summary-meta-row">
                <span class="summary-score">{s().score}%</span>
                {dotsAndActions()}
              </div>
              <div class="summary-labels">
                <span class="summary-overall">{overallLabel(s().overall)}</span>
                <Show when={hasErroredSection()}>
                  <span class="summary-incomplete">incomplete — some checks failed to run</span>
                </Show>
                <Show when={props.done}>
                  {(done) => (
                    <span class={durationClass(done().duration_ms)}>
                      <span class="summary-duration__label">Duration: </span>
                      {done().duration_ms}ms
                      <Show when={done().cached}>
                        {' '}<span class="cached-badge">cached</span>
                      </Show>
                    </span>
                  )}
                </Show>
              </div>
            </div>
          </div>
        }>
          <div class="summary-grade summary-grade--error" role="alert">
            <span class="summary-grade__letter grade--F" aria-label="Grade unavailable">?</span>
            <div class="summary-grade__meta">
              <span class="summary-overall">Grade unavailable — all backends failed</span>
            </div>
          </div>
        </Show>
      </div>

      <Show when={s().hard_fail}>
        <div class="summary-hard-fail" role="alert">
          Hard fail: {(s().hard_fail_checks ?? []).map(name => CHECK_LABELS[name] ?? name).join(', ')}
        </div>
      </Show>

      {/* ── Server info row ── */}
      <Show when={hasAddresses()}>
        <div class="summary-divider" />
        <div class="summary-servers">
          <div class="summary-servers__list">
            <Show when={props.httpServerIp}>
              <div class="summary-server-row">
                <span class="summary-server__label">HTTP</span>
                <span class="summary-server__ip">{props.httpServerIp}</span>
                <Show when={props.httpServerOrg}>
                  <span class="summary-server__hosted">Hosted by</span>
                  <span class="summary-server__org">{props.httpServerOrg}</span>
                </Show>
              </div>
            </Show>
            <For each={props.addresses}>
              {(addr) => (
                <div class="summary-server-row">
                  <span class="summary-server__label">IP</span>
                  <span class="summary-server__ip">{addr.ip}</span>
                  <Show when={addr.org}>
                    <span class="summary-server__hosted">Hosted by</span>
                    <span class="summary-server__org">{addr.org}</span>
                  </Show>
                  <Show when={addr.network_type && addr.network_type !== 'unknown'}>
                    <span class="summary-server__type">({addr.network_type})</span>
                  </Show>
                </div>
              )}
            </For>
          </div>
          <Show when={props.ipDetailUrl}>
            <a
              class="summary-servers__link"
              href={props.ipDetailUrl}
              target="_blank"
              rel="noopener noreferrer"
            >
              IP ↗
            </a>
          </Show>
        </div>
      </Show>
    </div>
  );
}
