import { describe, it, expect } from 'vitest';
import { inlineCode } from './utils';

describe('inlineCode', () => {
  it('returns an empty string for an empty value (no backticks)', () => {
    expect(inlineCode('')).toBe('');
  });

  it.each([
    ['LF', 'a\nb'],
    ['CR', 'a\rb'],
    ['CRLF', 'a\r\nb'],
  ])('replaces a %s line ending with one space inside a single code span', (_name, value) => {
    const out = inlineCode(value);
    expect(out).not.toMatch(/[\r\n]/);
    expect(out).toMatch(/^(`+)a b\1$/);
  });

  it('uses a longer fence than the longest backtick run', () => {
    const out = inlineCode('a ``b`` c');
    const fence = out.match(/^`+/)![0];
    expect(fence.length).toBeGreaterThan(2);
    expect(out).toContain('a ``b`` c');
  });
});
