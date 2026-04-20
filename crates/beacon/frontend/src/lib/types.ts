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
  duration_ms?: number;
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
  // Infrastructure
  'mx', 'fcrdns', 'dnsbl',
  // Authentication
  'spf', 'dkim', 'dmarc',
  // Transport Security
  'mta_sts', 'tls_rpt', 'dane', 'dnssec',
  // Brand & Policy
  'bimi', 'cross_validation',
];

export type Group = 'infrastructure' | 'authentication' | 'transport_security' | 'brand_policy';

export const GROUP_ORDER: Group[] = [
  'infrastructure', 'authentication', 'transport_security', 'brand_policy',
];

export const GROUP_LABELS: Record<Group, string> = {
  infrastructure: 'Infrastructure',
  authentication: 'Authentication',
  transport_security: 'Transport Security',
  brand_policy: 'Brand & Policy',
};

export const GROUP_CATEGORIES: Record<Group, Category[]> = {
  infrastructure: ['mx', 'fcrdns', 'dnsbl'],
  authentication: ['spf', 'dkim', 'dmarc'],
  transport_security: ['mta_sts', 'tls_rpt', 'dane', 'dnssec'],
  brand_policy: ['bimi', 'cross_validation'],
};

export const VERDICT_ORDER: Record<Verdict, number> = {
  skip: 0,
  pass: 1,
  info: 2,
  warn: 3,
  fail: 4,
};

const SUB_CHECK_LABELS: Record<string, string> = {
  // MX
  no_mx: 'No MX records',
  null_mx: 'Null MX (RFC 7505)',
  mx_cname: 'MX points to CNAME',
  mx_no_addr: 'MX host unresolvable',
  single_mx: 'Single MX record',
  no_ipv6: 'No IPv6 on MX hosts',
  low_network_diversity: 'Low network diversity',
  mx_ok: 'MX records',
  // SPF
  no_spf: 'No SPF record',
  multiple_spf: 'Multiple SPF records',
  permissive_all: 'Permissive all-mechanism',
  neutral_all: 'Neutral all-mechanism',
  spf_loop: 'SPF loop detected',
  lookup_count: 'DNS lookup limit',
  void_lookups: 'Void DNS lookups',
  ptr_mechanism: 'PTR mechanism used',
  overlapping_cidrs: 'Overlapping IP ranges',
  spf_ok: 'SPF record',
  depth_exceeded: 'SPF recursion too deep',
  // DKIM
  cname_loop: 'CNAME loop in selector',
  selector_not_found: 'Selector not found',
  key_revoked: 'Key revoked',
  ed25519_key: 'Ed25519 key',
  weak_rsa_key: 'Weak RSA key (<1024 bits)',
  short_rsa_key: 'Short RSA key (<2048 bits)',
  rsa_key_ok: 'RSA key',
  key_parse_error: 'Key parse error',
  no_dkim: 'No selectors checked',
  // DMARC
  no_dmarc: 'No DMARC record',
  multiple_dmarc: 'Multiple DMARC records',
  policy_reject: 'Policy: reject',
  policy_quarantine: 'Policy: quarantine',
  policy_none: 'Policy: none',
  policy_missing: 'Policy tag missing',
  pct_zero: 'Enforcement at 0%',
  pct_partial: 'Partial enforcement',
  rua_auth: 'RUA authorization',
  no_rua: 'No aggregate reports (rua)',
  ruf_auth: 'RUF authorization',
  no_ruf: 'No forensic reports (ruf)',
  fo: 'Failure reporting options',
  ri: 'Report interval',
  // MTA-STS
  absent: 'Policy absent',
  ssrf_blocked: 'SSRF blocked',
  https_fetch_failed: 'Policy fetch failed',
  https_redirect: 'Unexpected redirect',
  wrong_content_type: 'Wrong content-type',
  body_truncated: 'Policy body truncated',
  mode: 'Enforcement mode',
  max_age_low: 'max_age too low',
  // TLS-RPT
  invalid_syntax: 'Invalid syntax',
  invalid_rua: 'Invalid RUA URI',
  valid: 'Record valid',
  // DANE
  invalid_usage: 'Invalid usage field',
  invalid_selector: 'Invalid selector field',
  invalid_matching: 'Invalid matching type',
  // DNSSEC
  ad_set: 'DNSSEC validated',
  ad_not_set: 'Not DNSSEC validated',
  // BIMI
  logo_ssrf_blocked: 'Logo SSRF blocked',
  logo_redirect_ssrf_blocked: 'Logo redirect blocked',
  logo_reachable: 'Logo reachable',
  logo_unreachable: 'Logo unreachable',
  no_logo: 'No logo URL',
  vmc_present: 'VMC certificate',
  // FCrDNS
  no_ips: 'No IPs resolved',
  fcrdns_fail: 'FCrDNS mismatch',
  fcrdns_pass: 'FCrDNS verified',
  // DNSBL
  zone_unreachable: 'Zone unreachable',
  clean: 'Not listed',
  // Cross-validation
  dane_without_dnssec: 'DANE without DNSSEC',
  mta_sts_without_tls_rpt: 'MTA-STS without TLS-RPT',
  dane_without_tls_rpt: 'DANE without TLS-RPT',
  spf_mx_coverage: 'SPF covers MX hosts',
  bimi_dmarc_policy: 'BIMI DMARC policy',
  null_mx_spf: 'Null MX SPF consistency',
  reject_no_dkim: 'reject without DKIM',
  mta_sts_id_mismatch: 'MTA-STS ID mismatch',
  mta_sts_mx_coverage: 'MTA-STS covers MX hosts',
  dmarc_rua_auth: 'DMARC RUA authorization',
  dmarc_sp_gap: 'DMARC subdomain policy gap',
  fcrdns_mismatch: 'FCrDNS mismatch',
};

const SUB_CHECK_EXPLANATIONS: Record<string, string> = {
  // MX
  no_mx: 'No MX records found. This domain cannot receive email.',
  null_mx: 'Null MX (RFC 7505) declares this domain does not accept email. Correct if intentional.',
  mx_cname: 'RFC 5321 §5.1 prohibits MX hostnames that are CNAME aliases.',
  mx_no_addr: 'The MX hostname has no A or AAAA record — mail cannot be delivered to it.',
  single_mx: 'Only one MX host means no redundancy. If it goes down, delivery fails.',
  no_ipv6: 'None of the MX hosts have AAAA records. IPv6-only senders cannot deliver mail.',
  low_network_diversity: 'All MX hosts share the same /24 subnet, reducing fault tolerance.',
  mx_ok: 'MX records are present and appear valid.',
  // SPF
  no_spf: 'No SPF TXT record found. Any server can send mail claiming to be this domain.',
  multiple_spf: 'Only one SPF TXT record is allowed per domain (RFC 7208 §3.2). Receivers pick one arbitrarily.',
  permissive_all: '~all (softfail) or +all allow unauthorized senders to pass. Use -all to reject them.',
  neutral_all: '?all treats unauthorized senders as neutral — effectively no enforcement. Prefer -all.',
  spf_loop: 'SPF includes form a cycle, causing infinite recursion and permerror.',
  lookup_count: 'More than 10 DNS lookups during SPF evaluation causes permerror (RFC 7208 §4.6.4).',
  void_lookups: 'DNS lookups that return no results count toward a limit of 2 (RFC 7208 §4.6.4).',
  ptr_mechanism: 'The ptr mechanism is deprecated (RFC 7208 §5.5) and causes excessive DNS lookups.',
  overlapping_cidrs: 'IP ranges in the SPF record overlap, which is redundant.',
  spf_ok: 'SPF record is syntactically valid and authorizes senders.',
  depth_exceeded: 'The SPF include chain is too deep and may cause evaluation errors.',
  // DKIM
  cname_loop: 'The DKIM selector TXT name follows a CNAME loop and cannot be resolved.',
  selector_not_found: 'No TXT record exists at this DKIM selector name.',
  key_revoked: 'An empty p= tag revokes the key. Messages signed with it will fail validation.',
  ed25519_key: 'Ed25519 is a modern, strong algorithm. Verify that receiving MTAs support it.',
  weak_rsa_key: 'RSA keys under 1024 bits are cryptographically broken and should not be used.',
  short_rsa_key: 'RSA keys under 2048 bits are below current best-practice minimums.',
  rsa_key_ok: 'RSA key length meets current best-practice requirements.',
  key_parse_error: 'The DKIM key record is malformed and cannot be parsed.',
  no_dkim: 'No DKIM selectors were provided or discovered. Add selectors to check DKIM signing.',
  // DMARC
  no_dmarc: 'No DMARC TXT record at _dmarc. Receivers cannot enforce an authentication policy.',
  multiple_dmarc: 'Only one DMARC record is allowed (RFC 7489 §6.6.3). Receivers may ignore all of them.',
  policy_reject: 'p=reject is the strongest DMARC policy — unauthorized mail is rejected outright.',
  policy_quarantine: 'p=quarantine routes unauthorized mail to spam. Consider upgrading to reject.',
  policy_none: 'p=none is monitoring-only. No mail is rejected or quarantined.',
  policy_missing: 'DMARC record has no p= tag, making it invalid (RFC 7489 §6.3).',
  pct_zero: 'pct=0 means the policy is never applied. Increase to enforce protection.',
  pct_partial: 'pct < 100 applies the policy to only a fraction of failing messages.',
  rua_auth: 'A third-party RUA domain must publish a DMARC record authorizing the reports.',
  no_rua: 'Without rua=, you receive no aggregate DMARC feedback reports.',
  ruf_auth: 'A third-party RUF domain must authorize forensic report delivery.',
  no_ruf: 'Without ruf=, you receive no per-message forensic reports.',
  fo: 'fo= controls which failures trigger forensic reports.',
  ri: 'ri= overrides the default 86400-second report interval.',
  // MTA-STS
  absent: 'No policy record found.',
  ssrf_blocked: 'The fetch URL resolves to an internal/reserved address and was blocked.',
  https_fetch_failed: 'The MTA-STS policy at /.well-known/mta-sts.txt could not be fetched over HTTPS.',
  https_redirect: 'RFC 8461 §3.3 requires the policy URL to not redirect.',
  wrong_content_type: 'MTA-STS policy must be served as text/plain (RFC 8461 §3.2).',
  body_truncated: 'Policy body was capped at 64 KB. Ensure it fits within this limit.',
  mode: 'enforce: TLS required. testing: report-only. none: disabled.',
  max_age_low: 'max_age below 86400s (1 day) reduces caching effectiveness.',
  // TLS-RPT
  invalid_syntax: 'The TLS-RPT record does not follow the required syntax (RFC 8460).',
  invalid_rua: 'The rua= URI is not a valid mailto: or https: endpoint.',
  valid: 'Record is present and syntactically valid.',
  // DANE
  invalid_usage: 'TLSA usage field should be 2 (DANE-TA) or 3 (DANE-EE) for SMTP.',
  invalid_selector: 'Unrecognized TLSA selector field value.',
  invalid_matching: 'Unrecognized TLSA matching type field value.',
  // DNSSEC
  ad_set: 'The AD (Authenticated Data) bit confirms that DNSSEC validation succeeded.',
  ad_not_set: 'DNSSEC signatures were not validated — the domain may not be signed.',
  // BIMI
  logo_ssrf_blocked: 'The BIMI logo URI resolves to an internal address and was blocked.',
  logo_redirect_ssrf_blocked: 'A BIMI logo redirect target resolves to an internal address.',
  logo_reachable: 'The BIMI logo SVG is accessible.',
  logo_unreachable: 'The BIMI logo URI returned an error or timed out.',
  no_logo: 'BIMI record has no l= tag — a logo URL is required for display.',
  vmc_present: 'A Verified Mark Certificate vouches for the brand logo\'s authenticity.',
  // FCrDNS
  no_ips: 'No IPs could be resolved for the MX hostnames.',
  fcrdns_fail: 'At least one MX IP\'s PTR record does not point back to the MX hostname.',
  fcrdns_pass: 'All MX IPs have matching forward-confirmed reverse DNS entries.',
  // DNSBL
  zone_unreachable: 'This DNSBL zone did not respond in time.',
  clean: 'No MX IP addresses are listed in any checked blocklist zone.',
  // Cross-validation
  dane_without_dnssec: 'DANE TLSA records are only meaningful when DNSSEC is enabled.',
  mta_sts_without_tls_rpt: 'Without TLS-RPT, MTA-STS policy failures are invisible.',
  dane_without_tls_rpt: 'DANE is deployed without TLS-RPT reporting for visibility.',
  spf_mx_coverage: 'SPF authorizes outbound senders. Listing inbound MX IPs is only meaningful when those same hosts also send mail on the domain\'s behalf — which is unusual for managed providers like Google Workspace or Microsoft 365, where inbound and outbound IP ranges are intentionally separate.',
  bimi_dmarc_policy: 'BIMI requires a DMARC policy of quarantine or reject.',
  null_mx_spf: 'A null-MX domain should have an SPF record ending in -all.',
  reject_no_dkim: 'DMARC reject without DKIM risks rejecting legitimate mail if SPF alignment fails.',
  mta_sts_id_mismatch: 'The id= in DNS and the policy file differ — senders may reject the policy.',
  mta_sts_mx_coverage: 'MX hosts not listed in the MTA-STS policy may be rejected by enforcing senders.',
  dmarc_rua_auth: 'The external RUA destination has not authorized receiving DMARC reports.',
  dmarc_sp_gap: 'DMARC has a strict policy but sp= defaults to none for subdomains.',
  fcrdns_mismatch: 'Cross-validation detected an FCrDNS mismatch between MX and PTR records.',
};

export function subCheckLabel(name: string): string {
  if (name in SUB_CHECK_LABELS) return SUB_CHECK_LABELS[name];
  if (name.startsWith('listed_')) return `Listed: ${name.slice(7).replace(/_/g, '.')}`;
  if (name.startsWith('policy_response_')) return `Policy response: ${name.slice(16).replace(/_/g, '.')}`;
  return name.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

export function subCheckExplanation(name: string): string | undefined {
  if (name in SUB_CHECK_EXPLANATIONS) return SUB_CHECK_EXPLANATIONS[name];
  if (name.startsWith('listed_')) return `This IP is listed in the ${name.slice(7).replace(/_/g, '.')} blocklist zone.`;
  if (name.startsWith('policy_response_')) return `The ${name.slice(16).replace(/_/g, '.')} zone returned a policy/error response (typically because the query came via a public DNS resolver). Results from this zone are unreliable — switch beacon to a local resolver.`;
  return undefined;
}
