export type Verdict = 'skip' | 'pass' | 'info' | 'warn' | 'fail';
export type Grade = 'A' | 'B' | 'C' | 'D' | 'F';
export type Category =
  | 'mx' | 'spf' | 'dkim' | 'dmarc' | 'mta_sts' | 'tls_rpt'
  | 'dane' | 'dnssec' | 'bimi' | 'fcrdns' | 'dnsbl' | 'cross_validation';

export interface SubCheck {
  name: string;
  verdict: Verdict;
  detail: string;
}

export interface IpEnrichment {
  ip: string;
  asn?: number;
  org?: string;
  ip_type?: string;
}

export interface CheckResult {
  type: 'category';
  category: Category;
  verdict: Verdict;
  title: string;
  detail: string;
  sub_checks: SubCheck[];
  enrichment?: IpEnrichment[];
}

export interface SummaryEvent {
  type: 'summary';
  grade: Grade;
  verdicts: Record<string, Verdict>;
}

export type SseEvent = CheckResult | SummaryEvent;

export const CATEGORY_LABELS: Record<Category, string> = {
  mx: 'MX Records',
  spf: 'SPF',
  dkim: 'DKIM',
  dmarc: 'DMARC',
  mta_sts: 'MTA-STS',
  tls_rpt: 'TLS-RPT',
  dane: 'DANE',
  dnssec: 'DNSSEC',
  bimi: 'BIMI',
  fcrdns: 'FCrDNS',
  dnsbl: 'DNSBL',
  cross_validation: 'Cross-Validation',
};

export const CATEGORY_ORDER: Category[] = [
  'mx', 'spf', 'dkim', 'dmarc', 'mta_sts', 'tls_rpt',
  'dane', 'dnssec', 'bimi', 'fcrdns', 'dnsbl', 'cross_validation',
];

export const VERDICT_ORDER: Record<Verdict, number> = {
  skip: 0,
  pass: 1,
  info: 2,
  warn: 3,
  fail: 4,
};
