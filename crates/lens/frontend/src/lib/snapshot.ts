export function buildSnapshotUrl(baseUrl: string, shortid: string): string {
  return `${baseUrl}/r/${encodeURIComponent(shortid)}`;
}

export function buildShareSnippets(
  domain: string,
  shortid: string,
  baseUrl = window.location.origin,
): { plainUrl: string; markdown: string } {
  const plainUrl = buildSnapshotUrl(baseUrl, shortid);
  const markdown = `[Domain health snapshot for ${domain}](${plainUrl})`;
  return { plainUrl, markdown };
}
