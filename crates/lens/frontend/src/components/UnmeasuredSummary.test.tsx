// @vitest-environment jsdom
import { render, cleanup } from '@solidjs/testing-library';
import { afterEach, describe, expect, it } from 'vitest';
import Summary from './Summary';
import SectionHeadline from './SectionHeadline';
import ValidationChips from './ValidationChips';
import type { CheckItem, SummaryEvent, Verdict } from '../lib/types';

afterEach(cleanup);

const INCOMPLETE_NOTE = 'incomplete — some checks failed to run';

// "unmeasured" and "incomplete" are not yet part of the Verdict union.
const check = (verdict: string, name = 'dnssec'): CheckItem =>
  ({ name, verdict: verdict as Verdict, weight: 10 });

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

describe('unmeasured rendering', () => {
  it('C14: incomplete grade without errored sections shows the incomplete note, not "all backends failed"', () => {
    const { container } = render(() => (
      <Summary
        summary={summary('incomplete', { dns: 'pass', email: 'incomplete' })}
        done={null}
        checks={[check('unmeasured'), check('pass', 'spf')]}
      />
    ));
    expect(container.textContent).not.toContain('all backends failed');
    expect(container.textContent).toContain(INCOMPLETE_NOTE);
  });

  it('C14: grade "error" with all sections errored still says "all backends failed"', () => {
    const { container } = render(() => (
      <Summary summary={summary('error', { dns: 'error', tls: 'error' })} done={null} />
    ));
    expect(container.textContent).toContain('all backends failed');
  });

  it('C15: SectionHeadline renders an unmeasured check with an icon other than the skip dash', () => {
    const { container } = render(() => (
      <SectionHeadline checks={[check('unmeasured', 'dnssec'), check('fail', 'spf')]} />
    ));
    const items = Array.from(container.querySelectorAll('.section-headline-check'));
    expect(items.length).toBe(2);
    const unmeasured = items.find((el) => !el.textContent!.startsWith('✗'))!;
    expect(unmeasured).toBeDefined();
    expect(unmeasured.textContent!.startsWith('–')).toBe(false);
    expect(unmeasured.textContent!.startsWith('?')).toBe(true);
  });

  it('C16: ValidationChips counts unmeasured checks', () => {
    const { container } = render(() => (
      <ValidationChips checks={[check('pass'), check('pass'), check('unmeasured')]} />
    ));
    expect(container.textContent).toContain('1 could not check');
  });

  it('C17: a section with status "incomplete" renders a distinct label, not "err"', () => {
    const { container } = render(() => (
      <Summary summary={summary('B', { dns: 'pass', email: 'incomplete' })} done={null} />
    ));
    const email = Array.from(container.querySelectorAll('.summary-dot-item')).find((el) =>
      el.textContent!.includes('EMAIL'),
    )!;
    expect(email.textContent).not.toContain('err');
    expect(email.textContent).toContain('incomplete');
  });
});
