// @vitest-environment jsdom
import { render, cleanup } from '@solidjs/testing-library';
import { afterEach, describe, expect, it } from 'vitest';
import { LintTab } from './LintTab';

afterEach(cleanup);

const HOSTILE = '"><img src=x onerror=alert(1)>[x](javascript:alert(1))';

describe('LintTab', () => {
  it('renders a hostile lint message as text, not as DOM', () => {
    const { container } = render(() => (
      <LintTab
        categories={[{ category: 'dnssec', results: [{ Warning: HOSTILE }] }]}
        doneStats={null}
        isLoading={false}
      />
    ));
    expect(container.querySelector('img')).toBeNull();
    expect(container.textContent).toContain(HOSTILE);
  });
});
