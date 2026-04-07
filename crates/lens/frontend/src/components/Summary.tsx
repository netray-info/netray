import { Show } from 'solid-js';
import type { SummaryEvent, DoneEvent, Verdict } from '../lib/types';
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
          <VerdictDot verdict={s().tls} />
          <span>TLS</span>
        </div>
        <div class="summary-dot-item" role="listitem">
          <VerdictDot verdict={s().dns} />
          <span>DNS</span>
        </div>
        <div class="summary-dot-item" role="listitem">
          <VerdictDot verdict={s().ip} />
          <span>IP</span>
        </div>
      </div>

      <Show when={s().hard_fail}>
        <div class="summary-hard-fail" role="alert">
          Hard fail triggered — one or more critical checks failed.
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
