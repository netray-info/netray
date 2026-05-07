// @vitest-environment jsdom
// Tests SDD §10 step 17: Summary shows "Get the badge" button.
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

describe('Summary — "Get the badge" button (SDD §10 step 17)', () => {
  it('shows "Get the badge" button when badgesEnabled and domain are set', () => {
    const { getByText } = render(() => (
      <Summary
        summary={SUMMARY}
        done={null}
        domain="example.com"
        badgesEnabled={true}
      />
    ));
    expect(getByText('Get the badge')).toBeTruthy();
  });

  it('does not show button when grade is error', () => {
    const errorSummary = { ...SUMMARY, grade: 'error' };
    const { queryByText } = render(() => (
      <Summary
        summary={errorSummary}
        done={null}
        domain="example.com"
        badgesEnabled={true}
      />
    ));
    expect(queryByText('Get the badge')).toBeNull();
  });

  it('does not show button when badgesEnabled is false', () => {
    const { queryByText } = render(() => (
      <Summary
        summary={SUMMARY}
        done={null}
        domain="example.com"
        badgesEnabled={false}
      />
    ));
    expect(queryByText('Get the badge')).toBeNull();
  });

  it('does not show button when domain is empty', () => {
    const { queryByText } = render(() => (
      <Summary
        summary={SUMMARY}
        done={null}
        domain=""
        badgesEnabled={true}
      />
    ));
    expect(queryByText('Get the badge')).toBeNull();
  });
});
