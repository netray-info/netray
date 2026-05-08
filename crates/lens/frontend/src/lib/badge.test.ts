import { describe, expect, it } from 'vitest';
import type { MetaFeatures } from './types';
import {
  buildBadgeUrl,
  buildHtmlSnippet,
  buildMarkdownSnippet,
  buildOgMarkdown,
  buildOgUrl,
  buildRerunMarkdown,
  buildRerunUrl,
} from './badge';

const BASE = 'https://example.com';
const DOMAIN = 'example.com';

describe('MetaFeatures.badges', () => {
  it('badges field accepts boolean values', () => {
    const withBadges: MetaFeatures = { badges: true };
    const withoutBadges: MetaFeatures = { badges: false };
    const absent: MetaFeatures = {};
    expect(withBadges.badges).toBe(true);
    expect(withoutBadges.badges).toBe(false);
    expect(absent.badges).toBeUndefined();
  });
});

describe('buildBadgeUrl', () => {
  it('produces a .svg URL for the domain', () => {
    const url = buildBadgeUrl(BASE, DOMAIN);
    expect(url).toContain('/badge/');
    expect(url).toContain('.svg');
  });

  it('omits style param for default flat', () => {
    const url = buildBadgeUrl(BASE, DOMAIN);
    expect(url).not.toContain('style=');
  });

  it('includes style param for for-the-badge', () => {
    const url = buildBadgeUrl(BASE, DOMAIN, { style: 'for-the-badge' });
    expect(url).toContain('style=for-the-badge');
  });

  it('includes label param when provided', () => {
    const url = buildBadgeUrl(BASE, DOMAIN, { label: 'myco' });
    expect(url).toContain('label=myco');
  });
});

describe('buildHtmlSnippet', () => {
  it('contains an <img> tag with the badge URL', () => {
    const url = buildBadgeUrl(BASE, DOMAIN);
    const html = buildHtmlSnippet(url, DOMAIN);
    expect(html).toContain('<img');
    expect(html).toContain(url);
  });

  it('contains an <a> wrapping link', () => {
    const url = buildBadgeUrl(BASE, DOMAIN);
    const html = buildHtmlSnippet(url, DOMAIN);
    expect(html).toContain('<a ');
    expect(html).toContain('</a>');
  });
});

describe('buildMarkdownSnippet', () => {
  it('has markdown image syntax', () => {
    const url = buildBadgeUrl(BASE, DOMAIN);
    const md = buildMarkdownSnippet(url, DOMAIN);
    expect(md).toMatch(/!\[.*\]\(.*\)/);
  });

  it('wraps image in a link', () => {
    const url = buildBadgeUrl(BASE, DOMAIN);
    const md = buildMarkdownSnippet(url, DOMAIN);
    expect(md).toMatch(/\[!\[.*\]\(.*\)\]\(.*\)/);
  });
});

describe('buildRerunUrl', () => {
  it('points at the apex with ?d=domain', () => {
    expect(buildRerunUrl(BASE, DOMAIN)).toBe(`${BASE}/?d=${DOMAIN}`);
  });

  it('percent-encodes the domain', () => {
    expect(buildRerunUrl(BASE, 'a b.example')).toContain('a%20b.example');
  });
});

describe('buildRerunMarkdown', () => {
  it('is a markdown link wrapping the rerun URL', () => {
    const md = buildRerunMarkdown(BASE, DOMAIN);
    expect(md).toMatch(/^\[.*\]\(.*\)$/);
    expect(md).toContain(buildRerunUrl(BASE, DOMAIN));
    expect(md).toContain(DOMAIN);
  });
});

describe('buildOgMarkdown', () => {
  it('wraps the OG image in a link to the rerun URL', () => {
    const md = buildOgMarkdown(BASE, DOMAIN);
    expect(md).toMatch(/^\[!\[.*\]\(.*\)\]\(.*\)$/);
    expect(md).toContain(buildOgUrl(BASE, DOMAIN));
    expect(md).toContain(buildRerunUrl(BASE, DOMAIN));
  });
});
