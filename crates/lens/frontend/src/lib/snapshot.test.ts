import { describe, it, expect } from 'vitest';
import { buildSnapshotUrl, buildShareSnippets } from './snapshot';

describe('buildSnapshotUrl', () => {
  it('builds absolute URL with shortid', () => {
    const url = buildSnapshotUrl('https://example.com', 'Ab3c9XyZ');
    expect(url).toBe('https://example.com/r/Ab3c9XyZ');
  });

  it('encodes special characters in shortid', () => {
    const url = buildSnapshotUrl('https://example.com', 'Ab3c/XyZ');
    expect(url).toContain('/r/');
    expect(url).not.toContain('/r/Ab3c/XyZ');
  });
});

describe('buildShareSnippets', () => {
  it('returns plain URL and markdown', () => {
    const { plainUrl, markdown } = buildShareSnippets('example.com', 'Ab3c9XyZ', 'https://example.com');
    expect(plainUrl).toBe('https://example.com/r/Ab3c9XyZ');
    expect(markdown).toContain('https://example.com/r/Ab3c9XyZ');
    expect(markdown).toContain('example.com');
  });

  it('markdown is a link', () => {
    const { markdown } = buildShareSnippets('example.com', 'Ab3c9XyZ', 'https://example.com');
    expect(markdown).toMatch(/^\[.+\]\(.+\)$/);
  });
});
