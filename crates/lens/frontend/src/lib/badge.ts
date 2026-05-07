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

export function buildHtmlSnippet(badgeUrl: string, domain: string): string {
  return `<a href="${escapeHtml(location.origin)}/?d=${escapeHtml(domain)}"><img src="${escapeHtml(badgeUrl)}" alt="lens grade for ${escapeHtml(domain)}" /></a>`;
}

export function buildMarkdownSnippet(badgeUrl: string, domain: string): string {
  const checkUrl = `${location.origin}/?d=${domain}`;
  return `[![lens grade for ${domain}](${badgeUrl})](${checkUrl})`;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
