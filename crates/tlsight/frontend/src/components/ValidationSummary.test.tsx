// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { render } from 'solid-js/web';
import ValidationSummary from './ValidationSummary';
import type { Summary } from '../lib/types';

const summary = {
  verdict: 'skip',
  checks: {
    chain_trusted: 'skip', not_expired: 'skip', hostname_match: 'skip', caa_compliant: 'skip',
    dane_valid: 'skip', ct_logged: 'skip', ocsp_stapled: 'skip', consistency: 'skip',
  },
} as Summary;

describe('ValidationSummary verdict', () => {
  it('shows skip, not pass, when every quality check is skip (tls_reachable: skip only)', () => {
    const el = document.createElement('div');
    document.body.appendChild(el);
    const dispose = render(() => (
      <ValidationSummary
        summary={summary}
        portQualities={[{
          port: 443,
          quality: {
            verdict: 'skip',
            checks: [{ id: 'tls_reachable', category: 'protocol', status: 'skip', label: 'TLS reachable', detail: 'not tested from here' }],
          },
        }]}
      />
    ), el);
    const badge = el.querySelector('.validation-summary .badge');
    expect(badge).not.toBeNull();
    expect(badge!.textContent).toBe('skip');
    expect(badge!.classList.contains('badge--skip')).toBe(true);
    dispose();
  });
});
