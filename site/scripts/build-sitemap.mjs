#!/usr/bin/env node
// Generate site/sitemap.xml from the HTML files actually on disk.
//
// Usage:
//   node site/scripts/build-sitemap.mjs            # write to site/sitemap.xml
//   node site/scripts/build-sitemap.mjs --stdout   # print to stdout (for CI diff)
//
// `<lastmod>` is the file's most recent git commit timestamp; if git is not
// available (or the file is untracked), it falls back to the file's mtime.

import { readdirSync, statSync, writeFileSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SITE_ROOT = resolve(__dirname, '..');
const REPO_ROOT = resolve(SITE_ROOT, '..');
const ORIGIN = 'https://netray.info';

// Five drill-down tool subdomains. lens.netray.info is intentionally absent —
// post-repositioning the apex is lens, and lens.netray.info 301s to it.
const TOOL_DOMAINS = [
  'ip', 'dns', 'tls', 'http', 'email',
];

const writeStdout = process.argv.includes('--stdout');

function gitLastmod(absPath) {
  try {
    const rel = relative(REPO_ROOT, absPath);
    const out = execSync(`git log -1 --format=%cI -- "${rel}"`, {
      cwd: REPO_ROOT,
      stdio: ['ignore', 'pipe', 'ignore'],
    }).toString().trim();
    if (out) return out;
  } catch {
    // ignore
  }
  return new Date(statSync(absPath).mtimeMs).toISOString();
}

function urlFor(htmlPath) {
  // htmlPath is absolute. Convert to a sitemap <loc>.
  const rel = relative(SITE_ROOT, htmlPath);
  if (rel === 'index.html') return `${ORIGIN}/`;
  if (rel.endsWith('/index.html')) {
    return `${ORIGIN}/${rel.replace(/\/index\.html$/, '/')}`;
  }
  // Slug pages: drop the .html extension, matching existing sitemap convention.
  return `${ORIGIN}/${rel.replace(/\.html$/, '')}`;
}

function priorityFor(htmlPath) {
  const rel = relative(SITE_ROOT, htmlPath);
  if (rel === 'index.html') return '1.0';
  if (rel === 'tools/index.html') return '0.9';
  if (rel.startsWith('guide/')) return '0.8';
  if (rel.startsWith('api/')) return '0.7';
  return '0.8';
}

function listHtml(dir) {
  const out = [];
  const walk = (d) => {
    for (const entry of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, entry.name);
      if (entry.isDirectory()) {
        if (entry.name === 'scripts' || entry.name === 'og') continue;
        walk(p);
        continue;
      }
      if (!entry.isFile()) continue;
      if (!entry.name.endsWith('.html')) continue;
      // Don't surface error pages.
      if (entry.name === '404.html' || entry.name === '50x.html') continue;
      out.push(p);
    }
  };
  walk(dir);
  return out.sort();
}

function urlEntry({ loc, lastmod, changefreq, priority }) {
  const lines = [
    '  <url>',
    `    <loc>${loc}</loc>`,
  ];
  if (lastmod) lines.push(`    <lastmod>${lastmod}</lastmod>`);
  if (changefreq) lines.push(`    <changefreq>${changefreq}</changefreq>`);
  if (priority) lines.push(`    <priority>${priority}</priority>`);
  lines.push('  </url>');
  return lines.join('\n');
}

const entries = [];

// Landing page first. lens serves the apex, so its frontend dates the landing page.
const landing = join(SITE_ROOT, 'index.html');
entries.push({
  loc: urlFor(landing),
  lastmod: gitLastmod(join(REPO_ROOT, 'crates', 'lens', 'frontend', 'index.html')),
  changefreq: 'monthly',
  priority: '1.0',
});

// Tool SPAs (static entries; sources live in sibling repos, so no on-disk file).
for (const tool of TOOL_DOMAINS) {
  entries.push({
    loc: `https://${tool}.netray.info/`,
    changefreq: 'monthly',
    priority: '0.9',
  });
}

// Everything under site/ that's a guide or api page.
const onDisk = listHtml(SITE_ROOT)
  .filter((p) => p !== landing); // already added
for (const f of onDisk) {
  entries.push({
    loc: urlFor(f),
    lastmod: gitLastmod(f),
    changefreq: 'monthly',
    priority: priorityFor(f),
  });
}

const xml = [
  '<?xml version="1.0" encoding="UTF-8"?>',
  '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">',
  ...entries.map(urlEntry),
  '</urlset>',
  '',
].join('\n');

if (writeStdout) {
  process.stdout.write(xml);
} else {
  writeFileSync(join(SITE_ROOT, 'sitemap.xml'), xml);
  console.error(`wrote ${entries.length} URLs to site/sitemap.xml`);
}
