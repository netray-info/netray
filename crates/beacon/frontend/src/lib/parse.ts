/// Shared query-string parsing for the beacon input box.
///
/// A query is a whitespace-separated tokenisation: the first token is the
/// domain, the remaining tokens are DKIM selectors. Selectors follow
/// `validate_dkim_selector` in the backend (1-63 ASCII alphanumerics plus
/// hyphen, no dots).

export const MAX_SELECTORS = 5;

export interface ParsedQuery {
  domain: string;
  selectors: string[];
}

export type ParseResult = ParsedQuery | { error: string };

const SELECTOR_RE = /^[A-Za-z0-9-]{1,63}$/;

export function parseQuery(raw: string): ParseResult {
  const tokens = raw.trim().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return { error: 'enter a domain' };

  const [domain, ...rest] = tokens;
  if (!domain.includes('.')) {
    return { error: `"${domain}" is not a valid domain (must contain a dot)` };
  }

  const seen = new Set<string>();
  const selectors: string[] = [];
  for (const tok of rest) {
    if (tok.includes('.')) {
      return { error: `"${tok}" is not a valid DKIM selector (contains a dot)` };
    }
    if (!SELECTOR_RE.test(tok)) {
      return { error: `"${tok}" is not a valid DKIM selector` };
    }
    if (!seen.has(tok)) {
      seen.add(tok);
      selectors.push(tok);
    }
  }
  if (selectors.length > MAX_SELECTORS) {
    return { error: `too many DKIM selectors (max ${MAX_SELECTORS})` };
  }
  return { domain: domain.toLowerCase().replace(/\.$/, ''), selectors };
}

export function formatQuery(domain: string, selectors: string[]): string {
  return selectors.length ? `${domain} ${selectors.join(' ')}` : domain;
}
