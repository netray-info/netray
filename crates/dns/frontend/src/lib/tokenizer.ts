/**
 * Simple tokenizer for DNS query syntax highlighting.
 *
 * This is cosmetic only — misclassification produces wrong colors, not wrong
 * queries. The Rust parser on the backend is the single source of truth.
 */

export enum TokenType {
  Domain = 'domain',
  RecordType = 'record-type',
  Server = 'server',
  Flag = 'flag',
  Unknown = 'unknown',
}

export interface Token {
  type: TokenType;
  value: string;
  from: number;
  to: number;
}

const RECORD_TYPES = new Set([
  'A', 'AAAA', 'MX', 'TXT', 'NS', 'SOA', 'CNAME', 'CAA',
  'SRV', 'PTR', 'HTTPS', 'SVCB', 'SSHFP', 'TLSA', 'NAPTR',
  'HINFO', 'OPENPGPKEY', 'DNSKEY', 'DS', 'ALL',
]);

/**
 * Tokenize a DNS query string into classified tokens.
 *
 * Rules:
 * - The first non-whitespace token is always classified as Domain.
 * - Tokens starting with `@` are classified as Server.
 * - Tokens starting with `+` are classified as Flag.
 * - Tokens matching a known record type name (case-insensitive) are RecordType.
 * - Everything else is Unknown.
 */
export function tokenize(input: string): Token[] {
  const tokens: Token[] = [];
  const re = /\S+/g;
  let match: RegExpExecArray | null;
  let isFirst = true;

  while ((match = re.exec(input)) !== null) {
    const value = match[0];
    const from = match.index;
    const to = from + value.length;
    let type: TokenType;

    if (isFirst) {
      type = TokenType.Domain;
      isFirst = false;
    } else if (value.startsWith('@')) {
      type = TokenType.Server;
    } else if (value.startsWith('+')) {
      type = TokenType.Flag;
    } else if (RECORD_TYPES.has(value.toUpperCase())) {
      type = TokenType.RecordType;
    } else {
      type = TokenType.Unknown;
    }

    tokens.push({ type, value, from, to });
  }

  return tokens;
}
