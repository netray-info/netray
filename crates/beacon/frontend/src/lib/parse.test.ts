import { describe, it, expect } from 'vitest';
import { parseQuery, formatQuery, MAX_SELECTORS } from './parse';

describe('parseQuery', () => {
  it('rejects empty input', () => {
    const r = parseQuery('');
    expect('error' in r).toBe(true);
    if ('error' in r) expect(r.error).toContain('enter a domain');
  });

  it('rejects whitespace-only input', () => {
    const r = parseQuery('   \t\n  ');
    expect('error' in r).toBe(true);
  });

  it('parses a bare domain with no selectors', () => {
    const r = parseQuery('example.com');
    expect('error' in r).toBe(false);
    if ('domain' in r) {
      expect(r.domain).toBe('example.com');
      expect(r.selectors).toEqual([]);
    }
  });

  it('lowercases the domain', () => {
    const r = parseQuery('EXAMPLE.COM');
    if ('domain' in r) expect(r.domain).toBe('example.com');
    else throw new Error(r.error);
  });

  it('strips a trailing dot from the domain', () => {
    const r = parseQuery('example.com.');
    if ('domain' in r) expect(r.domain).toBe('example.com');
    else throw new Error(r.error);
  });

  it('rejects a domain without a dot', () => {
    const r = parseQuery('localhost');
    expect('error' in r).toBe(true);
    if ('error' in r) expect(r.error).toContain('valid domain');
  });

  it('parses multiple whitespace-separated selectors', () => {
    const r = parseQuery('example.com google s1 mc1');
    if ('domain' in r) {
      expect(r.domain).toBe('example.com');
      expect(r.selectors).toEqual(['google', 's1', 'mc1']);
    } else {
      throw new Error(r.error);
    }
  });

  it('splits on arbitrary whitespace (tabs, newlines, multi-space)', () => {
    const r = parseQuery('example.com\tgoogle\n   s1');
    if ('domain' in r) {
      expect(r.selectors).toEqual(['google', 's1']);
    } else {
      throw new Error(r.error);
    }
  });

  it('deduplicates repeated selectors while preserving first-seen order', () => {
    const r = parseQuery('example.com google s1 google s2 s1');
    if ('domain' in r) {
      expect(r.selectors).toEqual(['google', 's1', 's2']);
    } else {
      throw new Error(r.error);
    }
  });

  it('rejects a selector containing a dot', () => {
    // Regression guard for the host-spoofing CVE class: selectors with dots
    // would let an attacker query an arbitrary name under _domainkey.
    const r = parseQuery('example.com evil.co._domainkey.victim');
    expect('error' in r).toBe(true);
    if ('error' in r) expect(r.error).toContain('contains a dot');
  });

  it('rejects a selector with invalid characters', () => {
    const r = parseQuery('example.com bad_selector');
    expect('error' in r).toBe(true);
  });

  it('rejects a selector longer than 63 characters', () => {
    const long = 'a'.repeat(64);
    const r = parseQuery(`example.com ${long}`);
    expect('error' in r).toBe(true);
  });

  /// parseQuery splits on whitespace, not commas. A comma-joined selector list
  /// (e.g. from a paste) is treated as one token and rejected as an invalid
  /// selector. This test pins that current behaviour so any future change to
  /// support comma-separation is intentional.
  it('does not split on commas (comma-joined selectors are rejected as one token)', () => {
    const r = parseQuery('example.com google,s1,mc1');
    expect('error' in r).toBe(true);
  });

  it(`rejects more than MAX_SELECTORS (${MAX_SELECTORS}) selectors`, () => {
    const selectors = Array.from({ length: MAX_SELECTORS + 1 }, (_, i) => `sel${i}`).join(' ');
    const r = parseQuery(`example.com ${selectors}`);
    expect('error' in r).toBe(true);
    if ('error' in r) expect(r.error).toMatch(/too many/i);
  });

  it('accepts exactly MAX_SELECTORS selectors', () => {
    const selectors = Array.from({ length: MAX_SELECTORS }, (_, i) => `sel${i}`).join(' ');
    const r = parseQuery(`example.com ${selectors}`);
    if ('domain' in r) {
      expect(r.selectors).toHaveLength(MAX_SELECTORS);
    } else {
      throw new Error(r.error);
    }
  });
});

describe('formatQuery', () => {
  it('emits the bare domain when no selectors', () => {
    expect(formatQuery('example.com', [])).toBe('example.com');
  });

  it('joins domain and selectors with spaces', () => {
    expect(formatQuery('example.com', ['google', 's1'])).toBe('example.com google s1');
  });

  it('is the inverse of parseQuery for valid inputs', () => {
    const round = formatQuery('example.com', ['google', 's1']);
    const r = parseQuery(round);
    if ('domain' in r) {
      expect(r.domain).toBe('example.com');
      expect(r.selectors).toEqual(['google', 's1']);
    } else {
      throw new Error(r.error);
    }
  });
});
