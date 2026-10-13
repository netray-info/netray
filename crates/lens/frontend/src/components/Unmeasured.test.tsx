// @vitest-environment jsdom
import { render, cleanup } from '@solidjs/testing-library';
import { afterEach, describe, expect, it } from 'vitest';
import CheckList from './CheckList';
import Summary from './Summary';
import type { CheckItem, SummaryEvent, Verdict } from '../lib/types';

afterEach(cleanup);

const INCOMPLETE_NOTE = 'incomplete — some checks failed to run';

// "unmeasured" is not yet part of the Verdict union.
const check = (verdict: string): CheckItem =>
  ({ name: 'dnssec', verdict: verdict as Verdict, weight: 10 });

function summary(grade: string, sections: Record<string, string>): SummaryEvent {
  return {
    sections: sections as Record<string, Verdict>,
    section_grades: {},
    overall: 'warn',
    grade,
    score: 80,
    hard_fail: false,
    hard_fail_checks: [],
    not_applicable: {},
  };
}

describe('unmeasured vs skip', () => {
  it('C3: an unmeasured check renders "could not check", not the skip dash', () => {
    const { container } = render(() => <CheckList checks={[check('unmeasured')]} />);
    const icon = container.querySelector('.verdict-icon')!;
    expect(container.textContent).toContain('could not check');
    expect(icon.textContent).not.toBe('–');
  });

  it('C10: grade "incomplete" with an unmeasured check in a non-error section shows the incomplete note', () => {
    const { container } = render(() => (
      <Summary
        summary={summary('incomplete', { dns: 'warn', tls: 'pass' })}
        done={null}
        checks={[check('unmeasured'), check('pass')]}
      />
    ));
    expect(container.textContent).toContain(INCOMPLETE_NOTE);
  });

  it('C11: a skip check still renders the dash', () => {
    const { container } = render(() => <CheckList checks={[check('skip')]} />);
    expect(container.querySelector('.verdict-icon')!.textContent).toBe('–');
    expect(container.textContent).not.toContain('could not check');
  });

  it('C11: a summary whose only non-pass check is skip shows no incomplete note', () => {
    const { container } = render(() => (
      <Summary
        summary={summary('A', { dns: 'skip', tls: 'pass' })}
        done={null}
        checks={[check('skip'), check('pass')]}
      />
    ));
    expect(container.textContent).not.toContain(INCOMPLETE_NOTE);
  });
});
