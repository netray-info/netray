// @vitest-environment jsdom
// Tests SDD §10 step 17: Summary shows grade badge embed button.
import { render, cleanup } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import Summary from './Summary';
import type { SummaryEvent } from '../lib/types';

afterEach(cleanup);

Object.defineProperty(navigator, 'clipboard', {
  value: { writeText: vi.fn().mockResolvedValue(undefined) },
  configurable: true,
});

const SUMMARY: SummaryEvent = {
  grade: 'A',
  score: 95,
  overall: 'pass',
  sections: {},
  section_grades: {},
  hard_fail: false,
  hard_fail_checks: [],
  not_applicable: {},
};

const DONE = { domain: 'example.com', duration_ms: 100, cached: false, snapshot_id: null };

describe('Summary — grade badge embed button (SDD §10 step 17)', () => {
  it('shows badge embed button when badgesEnabled, domain, and done are set', () => {
    const { getByRole } = render(() => (
      <Summary
        summary={SUMMARY}
        done={DONE}
        domain="example.com"
        badgesEnabled={true}
      />
    ));
    expect(getByRole('button', { name: /share/i })).toBeTruthy();
  });

  it('does not show button when done is null (check not yet complete)', () => {
    const { queryByRole } = render(() => (
      <Summary
        summary={SUMMARY}
        done={null}
        domain="example.com"
        badgesEnabled={true}
      />
    ));
    expect(queryByRole('button', { name: /share/i })).toBeNull();
  });

  it('does not show button when grade is error', () => {
    const errorSummary = { ...SUMMARY, grade: 'error' };
    const { queryByRole } = render(() => (
      <Summary
        summary={errorSummary}
        done={DONE}
        domain="example.com"
        badgesEnabled={true}
      />
    ));
    expect(queryByRole('button', { name: /share/i })).toBeNull();
  });

  it('does not show button when badgesEnabled is false', () => {
    const { queryByRole } = render(() => (
      <Summary
        summary={SUMMARY}
        done={DONE}
        domain="example.com"
        badgesEnabled={false}
      />
    ));
    expect(queryByRole('button', { name: /share/i })).toBeNull();
  });

  it('does not show button when domain is empty', () => {
    const { queryByRole } = render(() => (
      <Summary
        summary={SUMMARY}
        done={DONE}
        domain=""
        badgesEnabled={true}
      />
    ));
    expect(queryByRole('button', { name: /share/i })).toBeNull();
  });
});
