// Tests for SDD §7.6: embed snippets must use ?d= (not ?domain=) as the check URL param.
import { describe, expect, it } from 'vitest';
import { buildHtmlSnippet, buildMarkdownSnippet } from './badge';

const BADGE_URL = 'https://lens.netray.info/badge/example.com.svg';
const DOMAIN = 'example.com';

describe('buildHtmlSnippet — link format (SDD §7.6)', () => {
  it('wrapping href uses ?d= query param', () => {
    const html = buildHtmlSnippet(BADGE_URL, DOMAIN);
    expect(html).toContain(`/?d=${DOMAIN}`);
    expect(html).not.toContain('/?domain=');
  });

  it('img alt matches SDD exact format', () => {
    const html = buildHtmlSnippet(BADGE_URL, DOMAIN);
    expect(html).toContain(`alt="lens grade for ${DOMAIN}"`);
  });
});

describe('buildMarkdownSnippet — link format (SDD §7.6)', () => {
  it('check URL uses ?d= query param', () => {
    const md = buildMarkdownSnippet(BADGE_URL, DOMAIN);
    expect(md).toContain(`/?d=${DOMAIN}`);
    expect(md).not.toContain('/?domain=');
  });
});
