import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';

const urls = resolveEnv();
const siteUrl = urls.site;

// Parse sitemap.xml to get all URLs
async function getSitemapUrls(request: any): Promise<string[]> {
  const response = await request.get(`${siteUrl}/sitemap.xml`);
  expect(response.status()).toBe(200);
  const xml = await response.text();
  const locMatches = xml.matchAll(/<loc>([^<]+)<\/loc>/g);
  return Array.from(locMatches, (m: RegExpMatchArray) => m[1]);
}

// Static site pages are on the main site origin (not tool subdomains)
function isStaticSitePage(pageUrl: string): boolean {
  const siteOrigin = new URL(siteUrl).origin;
  return pageUrl.startsWith(siteOrigin);
}

test('sitemap.xml is accessible', async ({ request }) => {
  const response = await request.get(`${siteUrl}/sitemap.xml`);
  expect(response.status()).toBe(200);
});

// TODO: remove filter once beacon is deployed behind Traefik
const isEmailOrigin = (url: string) => new URL(url).hostname === 'email.netray.info';

test('all sitemap URLs return 200 with title', async ({ request }) => {
  const sitemapUrls = await getSitemapUrls(request);
  expect(sitemapUrls.length).toBeGreaterThan(0);

  for (const pageUrl of sitemapUrls.filter(u => !isEmailOrigin(u))) {
    const response = await request.get(pageUrl);
    expect(response.status(), `${pageUrl} returns 200`).toBe(200);

    const html = await response.text();

    // Non-empty <title>
    const titleMatch = html.match(/<title>([^<]+)<\/title>/);
    expect(titleMatch, `${pageUrl} has <title>`).toBeTruthy();
    expect(titleMatch![1].trim().length, `${pageUrl} <title> non-empty`).toBeGreaterThan(0);

    // No external font references
    expect(html, `External font reference found in ${pageUrl}`).not.toContain('fonts.googleapis.com');
    expect(html, `External font reference found in ${pageUrl}`).not.toContain('fonts.gstatic.com');
  }
});

test('static site pages have meta description', async ({ request }) => {
  const sitemapUrls = await getSitemapUrls(request);
  const staticPages = sitemapUrls.filter(isStaticSitePage);

  for (const pageUrl of staticPages) {
    const response = await request.get(pageUrl);
    const html = await response.text();

    const descMatch = html.match(/<meta\s+name=["']description["']\s+content=["']([^"']+)["']/i);
    expect(descMatch, `${pageUrl} has <meta description>`).toBeTruthy();
    expect(descMatch![1].trim().length, `${pageUrl} <meta description> non-empty`).toBeGreaterThan(0);
  }
});

test('CSS files contain no external font references', async ({ request }) => {
  // guide/style.css is the shared stylesheet for guide and API pages
  const cssUrl = `${siteUrl}/guide/style.css`;
  const response = await request.get(cssUrl);
  expect(response.status(), `${cssUrl} returns 200`).toBe(200);
  const css = await response.text();
  expect(css, `External font reference found in ${cssUrl}`).not.toContain('fonts.googleapis.com');
  expect(css, `External font reference found in ${cssUrl}`).not.toContain('fonts.gstatic.com');
});

test('guide and API pages have correct canonical links', async ({ request }) => {
  const sitemapUrls = await getSitemapUrls(request);
  const guidesAndApi = sitemapUrls.filter(u => u.includes('/guide/') || u.includes('/api/'));

  for (const pageUrl of guidesAndApi) {
    const response = await request.get(pageUrl);
    const html = await response.text();

    const canonicalMatch = html.match(/<link\s+rel=["']canonical["']\s+href=["']([^"']+)["']/i);
    expect(canonicalMatch, `${pageUrl} has <link rel="canonical">`).toBeTruthy();

    const canonical = canonicalMatch![1];
    const normalizedCanonical = canonical.replace(/\/$/, '');
    const normalizedPageUrl = pageUrl.replace(/\/$/, '');
    expect(normalizedCanonical, `${pageUrl} canonical self-referencing`).toBe(normalizedPageUrl);
  }
});
