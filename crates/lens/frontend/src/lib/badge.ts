export function buildOgUrl(baseUrl: string, domain: string): string {
  return `${baseUrl}/og/${encodeURIComponent(domain)}.png`;
}

export function buildOgMetaTag(ogUrl: string): string {
  return `<meta property="og:image" content="${escapeHtml(ogUrl)}">`;
}

export function buildOgMarkdown(baseUrl: string, domain: string): string {
  return `[![lens domain health card for ${domain}](${buildOgUrl(baseUrl, domain)})](${buildRerunUrl(baseUrl, domain)})`;
}

export function buildRerunUrl(baseUrl: string, domain: string): string {
  return `${baseUrl}/?d=${encodeURIComponent(domain)}`;
}

export function buildRerunMarkdown(baseUrl: string, domain: string): string {
  return `[Re-run lens domain health check for ${domain}](${buildRerunUrl(baseUrl, domain)})`;
}

export function buildBadgeUrl(
  baseUrl: string,
  domain: string,
  opts: { style?: "flat" | "for-the-badge"; label?: string } = {},
): string {
  const url = new URL(`/badge/${encodeURIComponent(domain)}.svg`, baseUrl);
  if (opts.style && opts.style !== "flat") url.searchParams.set("style", opts.style);
  if (opts.label) url.searchParams.set("label", opts.label);
  return url.toString();
}

export function buildHtmlSnippet(badgeUrl: string, domain: string, origin = ''): string {
  return `<a href="${escapeHtml(origin)}/?d=${escapeHtml(domain)}"><img src="${escapeHtml(badgeUrl)}" alt="lens grade for ${escapeHtml(domain)}" /></a>`;
}

export function buildMarkdownSnippet(badgeUrl: string, domain: string, origin = ''): string {
  const checkUrl = `${origin}/?d=${domain}`;
  return `[![lens grade for ${domain}](${badgeUrl})](${checkUrl})`;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
