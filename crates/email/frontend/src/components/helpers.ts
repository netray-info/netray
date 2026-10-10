import type { Category, CheckResult, SubCheck } from '../lib/types';
import type { MetaResponse } from '../lib/api';

export type Ecosystem = NonNullable<MetaResponse['ecosystem']>;

export interface EcoLink {
  href: string;
  label: string;
}

export const DKIM_SELECTOR_RE = /selector '([A-Za-z0-9-]{1,63})'/;
export const DANE_HOST_RE = /^([A-Za-z0-9.-]+):\s/;

export function firstMatch(subChecks: SubCheck[], re: RegExp): string | null {
  for (const sc of subChecks) {
    const m = sc.detail.match(re);
    if (m) return m[1];
  }
  return null;
}

export function categoryHeaderLink(
  cat: Category,
  domain: string,
  eco: Ecosystem | undefined,
  result?: CheckResult,
): EcoLink | null {
  if (!domain || !eco) return null;

  const trim = (u: string) => u.replace(/\/+$/, '');
  const dnsBase = eco.dns_base_url ? trim(eco.dns_base_url) : null;
  const tlsBase = eco.tls_base_url ? trim(eco.tls_base_url) : null;

  const dnsQ = (name: string, type: string): EcoLink | null =>
    dnsBase
      ? { href: `${dnsBase}/?q=${encodeURIComponent(`${name} ${type}`)}&ref=beacon`, label: `DNS ↗` }
      : null;
  const tlsH = (host: string): EcoLink | null =>
    tlsBase
      ? { href: `${tlsBase}/?h=${encodeURIComponent(host)}&ref=beacon`, label: `TLS ↗` }
      : null;

  switch (cat) {
    case 'mx':      return dnsQ(domain, 'MX');
    case 'spf':     return dnsQ(domain, 'TXT');
    case 'dmarc':   return dnsQ(`_dmarc.${domain}`, 'TXT');
    case 'dnssec':  return dnsQ(domain, 'DNSKEY');
    case 'tls_rpt': return dnsQ(`_smtp._tls.${domain}`, 'TXT');
    case 'bimi':    return dnsQ(`default._bimi.${domain}`, 'TXT');
    case 'mta_sts': return tlsH(`mta-sts.${domain}`);
    case 'dkim': {
      const sel = result ? firstMatch(result.sub_checks, DKIM_SELECTOR_RE) : null;
      return sel ? dnsQ(`${sel}._domainkey.${domain}`, 'TXT') : null;
    }
    case 'dane': {
      const host = result ? firstMatch(result.sub_checks, DANE_HOST_RE) : null;
      return host ? dnsQ(`_25._tcp.${host}`, 'TLSA') : null;
    }
    default:        return null;
  }
}

export function durationClass(ms: number): string {
  if (ms < 500) return 'overview__value overview__value--fast';
  if (ms < 2000) return 'overview__value overview__value--ok';
  return 'overview__value overview__value--slow';
}
