// @vitest-environment jsdom
import { render, cleanup } from '@solidjs/testing-library';
import { afterEach, describe, expect, it } from 'vitest';
import CheckList from './CheckList';
import type { CheckItem } from '../lib/types';

afterEach(cleanup);

function scoreText(check: CheckItem): string | null | undefined {
  const { container } = render(() => <CheckList checks={[check]} />);
  return container.querySelector('.check-item__score')?.textContent;
}

describe('CheckList points', () => {
  it('gives a warn exactly half the weight', () => {
    expect(scoreText({ name: 'spf', verdict: 'warn', weight: 5 })).toBe('2.5/5');
    expect(scoreText({ name: 'spf', verdict: 'warn', weight: 3 })).toBe('1.5/3');
    expect(scoreText({ name: 'spf', verdict: 'warn', weight: 1 })).toBe('0.5/1');
  });

  it('renders integer points without a decimal', () => {
    expect(scoreText({ name: 'spf', verdict: 'warn', weight: 4 })).toBe('2/4');
    expect(scoreText({ name: 'spf', verdict: 'pass', weight: 5 })).toBe('5/5');
    expect(scoreText({ name: 'spf', verdict: 'fail', weight: 5 })).toBe('0/5');
  });
});
