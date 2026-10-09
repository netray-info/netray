// @vitest-environment jsdom
import { render, cleanup } from '@solidjs/testing-library';
import { afterEach, describe, expect, it } from 'vitest';
import CheckList from './CheckList';
import type { CheckItem } from '../lib/types';

afterEach(cleanup);

const HOSTILE = '"><img src=x onerror=alert(1)>[x](javascript:alert(1))';

describe('CheckList', () => {
  it('renders a hostile DNS check message as text, not markup', () => {
    const checks: CheckItem[] = [
      { name: 'dnssec', verdict: 'warn', messages: [HOSTILE] },
    ];
    const { container } = render(() => <CheckList checks={checks} />);
    expect(container.querySelector('img')).toBeNull();
    expect(container.textContent).toContain(HOSTILE);
  });
});
