import { For, Show } from 'solid-js';
import type { CheckItem, Verdict } from '../lib/types';
import { CHECK_LABELS } from '../lib/checkMeta';

function verdictIcon(v: Verdict): string {
  switch (v) {
    case 'pass':  return '✓';
    case 'warn':  return '!';
    case 'fail':  return '✗';
    case 'error': return '✗';
    case 'skip':  return '–';
  }
}

function verdictColor(v: Verdict): string {
  switch (v) {
    case 'pass':  return 'var(--verdict-pass)';
    case 'warn':  return 'var(--verdict-warn)';
    case 'fail':  return 'var(--verdict-fail)';
    case 'error': return 'var(--verdict-error)';
    case 'skip':  return 'var(--verdict-skip)';
  }
}

function scoreClass(v: Verdict): string {
  switch (v) {
    case 'warn':  return 'check-item__score check-item__score--warn';
    case 'fail':
    case 'error': return 'check-item__score check-item__score--fail';
    default:      return 'check-item__score';
  }
}

function scoreNumerator(v: Verdict, weight: number): number {
  switch (v) {
    case 'pass':  return weight;
    case 'warn':  return Math.floor(weight / 2);
    default:      return 0;
  }
}

interface Props {
  checks: CheckItem[];
}

export default function CheckList(props: Props) {
  return (
    <ul class="check-list" role="list">
      <For each={props.checks}>
        {(check) => (
          <li class="check-item">
            <span
              class="verdict-icon"
              style={{ color: verdictColor(check.verdict) }}
              aria-label={check.verdict}
            >
              {verdictIcon(check.verdict)}
            </span>
            <div class="check-item__name">
              {CHECK_LABELS[check.name] ?? check.name}
              <Show when={check.guide_url}>
                <a
                  class="check-item__guide-link"
                  href={check.guide_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  title="Learn more"
                  aria-label="Learn more"
                >
                  ?↗
                </a>
              </Show>
              <Show when={check.messages && check.messages!.length > 0}>
                <For each={check.messages}>
                  {(msg) => <div class="check-item__message">{msg}</div>}
                </For>
              </Show>
            </div>
            <Show when={check.weight !== undefined && check.verdict !== 'skip'}>
              <span class={scoreClass(check.verdict)}>
                {scoreNumerator(check.verdict, check.weight!)}/{check.weight}
              </span>
            </Show>
          </li>
        )}
      </For>
    </ul>
  );
}
