import { For, Show } from 'solid-js';
import type { CheckItem, Verdict } from '../lib/types';

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
            <span class="check-item__name">{check.name}</span>
            {check.message && (
              <span class="check-item__message">{check.message}</span>
            )}
            <Show when={check.guide_url}>
              <a
                class="check-item__guide-link"
                href={check.guide_url}
                target="_blank"
                rel="noopener noreferrer"
              >
                Learn why
              </a>
            </Show>
          </li>
        )}
      </For>
    </ul>
  );
}
