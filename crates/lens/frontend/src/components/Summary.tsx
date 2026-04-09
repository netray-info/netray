import { Show } from 'solid-js';
import type { SummaryEvent, DoneEvent, Verdict } from '../lib/types';
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

interface Props {
  summary: SummaryEvent;
  done: DoneEvent | null;
}

export default function Summary(props: Props) {
  const s = () => props.summary;
  const isError = () => s().grade === 'error';
  const sectionVerdict = (name: string): Verdict => s().sections[name] ?? 'error';
  const sectionGrade = (name: string): string | undefined => s().section_grades[name];
  const hasErroredSection = () =>
    sectionVerdict('dns') === 'error' || sectionVerdict('tls') === 'error' || sectionVerdict('ip') === 'error';

  return (
    <div class="summary-card" role="region" aria-label="Summary">
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
            <span class="summary-score">{s().score}%</span>
            <span class="summary-overall">{overallLabel(s().overall)}</span>
            <Show when={hasErroredSection()}>
              <span class="summary-incomplete">incomplete — some checks failed to run</span>
            </Show>
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

      <div class="summary-dots" role="list" aria-label="Section statuses">
        <div class="summary-dot-item" role="listitem">
          <VerdictDot verdict={sectionVerdict('tls')} />
          <span>TLS</span>
          <Show when={sectionVerdict('tls') === 'error'} fallback={
            <Show when={sectionGrade('tls')}>
              <span class="summary-dot-grade" style={{ color: gradeStyle(sectionGrade('tls')!) }}>{sectionGrade('tls')}</span>
            </Show>
          }>
            <span class="summary-dot-error">err</span>
          </Show>
        </div>
        <div class="summary-dot-item" role="listitem">
          <VerdictDot verdict={sectionVerdict('dns')} />
          <span>DNS</span>
          <Show when={sectionVerdict('dns') === 'error'} fallback={
            <Show when={sectionGrade('dns')}>
              <span class="summary-dot-grade" style={{ color: gradeStyle(sectionGrade('dns')!) }}>{sectionGrade('dns')}</span>
            </Show>
          }>
            <span class="summary-dot-error">err</span>
          </Show>
        </div>
        <div class="summary-dot-item" role="listitem">
          <VerdictDot verdict={sectionVerdict('ip')} />
          <span>IP</span>
          <Show when={sectionVerdict('ip') === 'error'} fallback={
            <Show when={sectionGrade('ip')}>
              <span class="summary-dot-grade" style={{ color: gradeStyle(sectionGrade('ip')!) }}>{sectionGrade('ip')}</span>
            </Show>
          }>
            <span class="summary-dot-error">err</span>
          </Show>
        </div>
      </div>

      <Show when={s().hard_fail}>
        <div class="summary-hard-fail" role="alert">
          Hard fail: {(s().hard_fail_checks ?? []).map(name => CHECK_LABELS[name] ?? name).join(', ')}
        </div>
      </Show>

      <Show when={props.done}>
        {(done) => (
          <div class="summary-meta">
            <span>{done().duration_ms}ms</span>
            <Show when={done().cached}>
              <span class="cached-badge">cached</span>
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}
