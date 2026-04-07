import { For } from 'solid-js';
import type { CheckItem } from '../lib/types';
import { CHECK_LABELS } from '../lib/checkMeta';

function verdictIcon(v: string): string {
  switch (v) {
    case 'pass':  return '✓';
    case 'warn':  return '!';
    case 'fail':
    case 'error': return '✗';
    default:      return '–';
  }
}

function verdictColor(v: string): string {
  switch (v) {
    case 'pass':  return 'var(--verdict-pass)';
    case 'warn':  return 'var(--verdict-warn)';
    case 'fail':
    case 'error': return 'var(--verdict-error)';
    default:      return 'var(--verdict-skip)';
  }
}

function headlineChecks(checks: CheckItem[] | undefined): CheckItem[] {
  if (!checks || checks.length === 0) return [];
  const byWeight = (a: CheckItem, b: CheckItem) => (b.weight ?? 0) - (a.weight ?? 0);
  const failing = checks.filter(c => c.verdict === 'fail' || c.verdict === 'error').sort(byWeight);
  const warning = checks.filter(c => c.verdict === 'warn').sort(byWeight);
  const notable = [...failing, ...warning].slice(0, 5);
  if (notable.length > 0) return notable;
  return [...checks].filter(c => c.verdict !== 'skip').sort(byWeight).slice(0, 4);
}

interface Props {
  checks?: CheckItem[];
}

export default function SectionHeadline(props: Props) {
  return (
    <span class="section-headline-checks">
      <For each={headlineChecks(props.checks)}>
        {(check) => (
          <span class="section-headline-check">
            <span style={{ color: verdictColor(check.verdict) }} aria-hidden="true">
              {verdictIcon(check.verdict)}
            </span>
            {CHECK_LABELS[check.name] ?? check.name}
          </span>
        )}
      </For>
    </span>
  );
}
